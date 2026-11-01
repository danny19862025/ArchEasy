"""Offline sign-in/security/protocol tests; no real account or OpenAI traffic."""
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('ask_ai', ROOT / 'scripts/ask-ai.py')
ai = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ai)


class ClientTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from cryptography.hazmat.primitives.asymmetric import rsa
        cls.key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        numbers = cls.key.public_key().public_numbers()
        cls.jwks = {'keys': [{'kid': 'test-key', 'kty': 'RSA', 'alg': 'RS256',
                             'n': ai.b64encode(numbers.n.to_bytes(256, 'big')),
                             'e': ai.b64encode(numbers.e.to_bytes(3, 'big'))}]}

    def token(self, **overrides):
        from cryptography.hazmat.primitives import hashes
        from cryptography.hazmat.primitives.asymmetric import padding
        claims = {'iss': ai.ISSUER, 'sub': 'subject', 'aud': 'issued-client',
                  'nonce': 'nonce', 'exp': time.time()+3600, 'iat': time.time(), **overrides}
        h = ai.b64encode(json.dumps({'kid': 'test-key', 'alg': 'RS256'}).encode())
        p = ai.b64encode(json.dumps(claims).encode())
        message = (h+'.'+p).encode()
        return h+'.'+p+'.'+ai.b64encode(self.key.sign(message, padding.PKCS1v15(), hashes.SHA256()))

    def test_signed_identity_and_claim_rejection(self):
        self.assertEqual(ai.validate_identity(self.token(), 'issued-client', 'nonce', self.jwks)['sub'], 'subject')
        for change in [{'iss': 'https://evil.example'}, {'exp': 1}, {'nonce': 'wrong'},
                       {'aud': 'other'}, {'sub': ''}, {'iat': time.time()+3600},
                       {'aud': ['issued-client', 'other'], 'azp': 'other'}]:
            with self.subTest(change=change), self.assertRaises(ai.UserError):
                ai.validate_identity(self.token(**change), 'issued-client', 'nonce', self.jwks)
        token = self.token().split('.')
        token[1] = ai.b64encode(b'{"sub":"forged"}')
        with self.assertRaises(ai.UserError):
            ai.validate_identity('.'.join(token), 'issued-client', 'nonce', self.jwks)

    def test_callback_state_duplicate_path_and_client_binding(self):
        self.assertIsNone(ai.callback_values('/auth/callback?state=wrong&code=x', 'state', None))
        self.assertIsNone(ai.callback_values('/other?state=state&code=x', 'state', None))
        self.assertIsNone(ai.callback_values('/auth/callback?state=state&state=state&code=x', 'state', None))
        self.assertIsNone(ai.callback_values('/auth/callback?state=%E2%98%83', 'state', None))
        for query, client in [('state=state&code=x', None),
                              ('state=state&code=x&client_id=other', 'issued-client'),
                              ('state=state&error=access_denied', None)]:
            with self.subTest(query=query), self.assertRaises(ai.UserError):
                ai.callback_values('/auth/callback?'+query, 'state', client)
        self.assertEqual(ai.callback_values('/auth/callback?state=state&code=x&client_id=issued-client', 'state', None), ('x', 'issued-client'))
        self.assertEqual(ai.callback_values('/auth/callback?state=state&code=x', 'state', 'issued-client'), ('x', 'issued-client'))

    def test_private_atomic_storage_and_stable_host(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)/'private'
            with ai.storage(directory) as (path, state):
                host = state['host_id']
                state['accounts'].append({'client_id': 'issued', 'access_token': 'secret'})
                ai.save(path, state)
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                self.assertEqual(directory.stat().st_mode & 0o777, 0o700)
            with ai.storage(directory) as (path, state):
                self.assertEqual(state['host_id'], host)
                self.assertEqual(state['accounts'][0]['access_token'], 'secret')
            path.chmod(0o644)
            with self.assertRaises(ai.UserError):
                ai.secure_read(path)

    def test_symlink_credentials_are_not_followed(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            original = root/'original'
            original.write_text('private')
            link = root/'link'
            link.symlink_to(original)
            with self.assertRaises(OSError):
                ai.secure_read(link)
            self.assertEqual(original.read_text(), 'private')

    def account(self):
        return {'client_id': 'issued-client', 'subject': 'subject', 'email': 'user@example.test',
                'access_token': 'old', 'refresh_token': 'refresh', 'id_token': 'validated',
                'expires_at': 1, 'scopes': [ai.PERMISSION]}

    def test_refresh_rotation_keeps_identity_and_requires_permission(self):
        account = self.account()
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'accounts.json'
            state = {'accounts':[account]}
            with patch.object(ai, 'request_json', return_value={
                    'access_token':'new', 'refresh_token':'replacement', 'id_token':'unvalidated',
                    'token_type':'Bearer', 'expires_in':3600}) as request:
                self.assertEqual(ai.access(path, state, account), 'new')
                self.assertEqual(request.call_args.kwargs['data']['client_id'], 'issued-client')
            self.assertEqual(account['refresh_token'], 'replacement')
            self.assertEqual(account['id_token'], 'validated')
            self.assertEqual(json.loads(path.read_text())['accounts'][0]['access_token'], 'new')
            account['scopes'] = []
            with self.assertRaises(ai.UserError):
                ai.access(path, state, account)

    def test_sse_requires_completed_and_handles_refusal_failure(self):
        def stream(events):
            return io.BytesIO(b''.join(('data: '+json.dumps(e)+'\n\n').encode() for e in events))
        chunks = []
        events = [{'type':'response.output_text.delta','delta':'Hello'},
                  {'type':'response.output_text.delta','delta':' world'},
                  {'type':'response.completed','response':{'status':'completed'}}]
        self.assertEqual(ai.response_text(stream(events), chunks.append), 'Hello world')
        self.assertEqual(chunks, ['Hello',' world'])
        for bad in [events[:-1], [{'type':'response.failed'}],
                    [{'type':'response.completed','response':{'status':'incomplete'}}]]:
            with self.assertRaises(ai.UserError):
                ai.response_text(stream(bad), lambda _: None)

    def test_ask_sends_only_explicit_chat_and_no_tools_or_store(self):
        account = self.account()
        account['expires_at'] = time.time()+3600
        state = {'accounts':[account], 'active':'issued-client'}
        response = io.BytesIO(b'data: {"type":"response.output_text.delta","delta":"Hi"}\n\ndata: {"type":"response.completed","response":{"status":"completed"}}\n\n')
        messages = [{'role':'user','content':'Explain ghostty'}]
        with patch.object(ai, 'open_request', return_value=response) as request, patch.object(ai, 'emit'):
            ai.ask(Path('/unused'), state, {'model':'account-model','messages':messages})
            self.assertEqual(request.call_args.args[0], ai.RESOURCE+'/responses')
            data = request.call_args.kwargs['data']
            self.assertFalse(data['store'])
            self.assertTrue(data['stream'])
            self.assertEqual(data['input'], messages)
            self.assertNotIn('tools', data)
            self.assertNotIn('previous_response_id', data)
        with self.assertRaises(ai.UserError):
            ai.ask(Path('/unused'), state, {'model':'x','messages':[{'role':'system','content':'x'}]})

    def test_model_catalog_is_account_specific_and_not_hardcoded(self):
        with patch.object(ai, 'request_json', return_value={'models':[
                {'slug':'first','display_name':'First','visibility':'list'},
                {'slug':'hidden','visibility':'hidden'},
                {'slug':'second','display_name':'Second','visibility':'list'}]}) as request:
            self.assertEqual(ai.list_models('token'), [{'slug':'first','name':'First'},{'slug':'second','name':'Second'}])
            self.assertEqual(request.call_args.kwargs['token'], 'token')

    def test_local_signout_preserves_registration_but_clears_tokens(self):
        account = self.account()
        state = {'host_id':'stable', 'accounts':[account], 'active':'issued-client'}
        with tempfile.TemporaryDirectory() as temp, patch.object(ai, 'request_json', side_effect=ai.UserError('offline')), patch.object(ai, 'emit') as emit:
            ai.sign_out(Path(temp)/'accounts.json', state, 'issued-client')
            self.assertNotIn('access_token', account)
            self.assertNotIn('refresh_token', account)
            self.assertNotIn('id_token', account)
            self.assertEqual(account['subject'], 'subject')
            self.assertEqual(state['host_id'], 'stable')
            self.assertIn('not confirmed', emit.call_args.kwargs['text'])

    def test_real_loopback_dynamic_registration_exchange_failure_and_retry(self):
        import http.client
        import threading
        import urllib.parse
        captured, requests, threads = {}, [], []
        fail_once = [True]

        def emit(event, **fields):
            if event != 'authorize':
                return
            query = urllib.parse.parse_qs(urllib.parse.urlsplit(fields['url']).query)
            captured.update({k:v[0] for k,v in query.items()})
            redirect = urllib.parse.urlsplit(captured['redirect_uri'])
            def callback():
                conn = http.client.HTTPConnection(redirect.hostname, redirect.port, timeout=5)
                args = urllib.parse.urlencode({'code':'one-use-code','state':captured['state'],'client_id':'issued-client'})
                conn.request('GET', '/auth/callback?'+args)
                self.assertEqual(conn.getresponse().status, 200)
                conn.close()
            thread = threading.Thread(target=callback)
            threads.append(thread)
            thread.start()

        def request(url, **kwargs):
            requests.append((url,kwargs))
            if url.endswith('jwks.json'):
                return self.jwks
            self.assertEqual(kwargs['data']['client_id'], 'issued-client')
            self.assertEqual(kwargs['data']['redirect_uri'], captured['redirect_uri'])
            if fail_once:
                fail_once.pop()
                raise ai.UserError('Simulated failed one-use code exchange')
            verifier = kwargs['data']['code_verifier']
            self.assertEqual(ai.b64encode(ai.hashlib.sha256(verifier.encode()).digest()), captured['code_challenge'])
            return {'token_type':'Bearer', 'access_token':'access', 'refresh_token':'refresh',
                    'expires_in':3600, 'scope':ai.SCOPES, 'id_token':self.token(nonce=captured['nonce'])}

        with tempfile.TemporaryDirectory() as temp, patch.object(ai, 'emit', side_effect=emit), patch.object(ai, 'request_json', side_effect=request):
            with ai.storage(Path(temp)/'private') as (path,state):
                with self.assertRaises(ai.UserError):
                    ai.sign_in(path,state)
                self.assertEqual(captured['client_id'], 'dynamic_agent_client')
                self.assertEqual(captured['agent_name_hint'], 'ArchEasy')
                self.assertEqual(state['accounts'], [])
                self.assertEqual(state['pending'], [{'client_id':'issued-client'}])
                self.assertIsNone(state['active'])
                self.assertFalse(ai.public_state(state)['accounts'][0]['connected'])
                captured.clear()
                account = ai.sign_in(path,state,'issued-client')
                self.assertEqual(state['pending'], [])
                self.assertEqual(account['subject'], 'subject')
                self.assertEqual(state['active'], 'issued-client')
                self.assertEqual(captured['client_id'], 'issued-client')
                self.assertNotIn('agent_name_hint', captured)
                self.assertTrue(captured['redirect_uri'].startswith('http://127.0.0.1:'))
                self.assertNotIn('client_secret', requests[0][1]['data'])
        for thread in threads:
            thread.join(timeout=5)


class ProviderTests(unittest.TestCase):
    def test_provider_auth_uses_headers_not_query_or_other_provider_tokens(self):
        for provider, host, header in [('gemini','generativelanguage.googleapis.com','x-goog-api-key'),
                                       ('claude','api.anthropic.com','x-api-key')]:
            with self.subTest(provider=provider), patch.object(ai, 'request_json', return_value={}) as request:
                ai.api_request(provider, 'models', 'private-test-key')
                url = request.call_args.args[0]
                options = request.call_args.kwargs
                self.assertIn(host, url)
                self.assertNotIn('private-test-key', url)
                self.assertEqual(options['extra_headers'][header], 'private-test-key')
                self.assertNotIn('Authorization', options['extra_headers'])
                self.assertNotIn('token', options)

    def test_gemini_catalog_filters_and_paginates_without_hardcoded_models(self):
        pages = [{'models':[{'name':'models/gemini-test','displayName':'Test','supportedGenerationMethods':['generateContent']},
                            {'name':'models/embedding','supportedGenerationMethods':['embedContent']}], 'nextPageToken':'next + page'},
                 {'models':[{'name':'models/other','supportedGenerationMethods':['generateContent']}]}]
        with patch.object(ai, 'api_request', side_effect=pages) as request:
            result = ai.api_models('gemini','key')
            self.assertEqual([m['slug'] for m in result], ['models/gemini-test','models/other'])
            self.assertIn('pageToken=next+%2B+page',request.call_args.args[1])

    def test_claude_catalog_paginates_in_server_order(self):
        with patch.object(ai, 'api_request', side_effect=[
                {'data':[{'id':'one','display_name':'One'}],'has_more':True,'last_id':'one'},
                {'data':[{'id':'two','display_name':'Two'}],'has_more':False}]) as request:
            self.assertEqual([m['slug'] for m in ai.api_models('claude','key')],['one','two'])
            self.assertIn('after_id=one',request.call_args.args[1])

    def test_gemini_payload_roles_no_tools_and_only_completed_visible_text(self):
        messages=[{'role':'user','content':'Explain Ghostty'},{'role':'assistant','content':'A terminal'},
                  {'role':'user','content':'What does that mean?'}]
        answer={'candidates':[{'finishReason':'STOP','content':{'parts':[{'text':'private thought','thought':True},{'text':'A terminal runs command-line programs.'}]}}]}
        with patch.object(ai,'api_request',return_value=answer) as request, patch.object(ai,'emit') as emit:
            ai.api_reply('gemini','private-test-credential',{'messages':messages,'model':'models/test/model'})
            args=request.call_args.args
            self.assertEqual(args[1],'models/test%2Fmodel:generateContent')
            body=args[3]
            self.assertEqual([m['role'] for m in body['contents']],['user','model','user'])
            self.assertNotIn('tools',body)
            self.assertNotIn('private-test-credential',str(body))
            self.assertEqual(emit.call_args.kwargs['text'],'A terminal runs command-line programs.')
        with patch.object(ai,'api_request',return_value={'candidates':[{'finishReason':'MAX_TOKENS'}]}), self.assertRaises(ai.UserError):
            ai.api_reply('gemini','key',{'messages':messages,'model':'models/test'})

    def test_claude_payload_text_reply_and_output_limit(self):
        messages=[{'role':'user','content':'Explain a package'}]
        with patch.object(ai,'api_request',return_value={'stop_reason':'end_turn','content':[{'type':'text','text':'Hello'}]}) as request, patch.object(ai,'emit') as emit:
            ai.api_reply('claude','key',{'messages':messages,'model':'claude-account-model'})
            body=request.call_args.args[3]
            self.assertEqual(body['messages'],messages)
            self.assertEqual(body['model'],'claude-account-model')
            self.assertFalse(body['stream'])
            self.assertNotIn('tools',body)
            self.assertEqual(emit.call_args.kwargs['text'],'Hello')
        with patch.object(ai,'api_request',return_value={'stop_reason':'max_tokens','content':[]}), self.assertRaises(ai.UserError):
            ai.api_reply('claude','key',{'messages':messages,'model':'claude-account-model'})

    def test_api_actions_require_safe_keys_and_do_not_write_credentials(self):
        for key in ['', 'secret\r\nAuthorization: malicious', 'a'*513, '☃']:
            with self.subTest(key_length=len(key)), self.assertRaises(ai.UserError), patch.object(ai,'api_models') as models:
                ai.api_action('gemini','models',{'api_key':key})
            models.assert_not_called()
        with patch.object(ai,'api_models',return_value=[]), patch.object(ai,'save') as save, patch.object(ai,'emit') as emit:
            ai.api_action('gemini','models',{'api_key':'private-key'})
            save.assert_not_called()
            self.assertNotIn('private-key',str(emit.call_args_list))

    def test_provider_error_does_not_echo_key_or_response_body(self):
        import urllib.error
        error=urllib.error.HTTPError('https://api.anthropic.com/v1/messages',401,'bad',{},io.BytesIO(b'private-secret-key'))
        with patch.object(ai.urllib.request,'build_opener') as opener:
            opener.return_value.open.side_effect=error
            with self.assertRaises(ai.UserError) as result:
                ai.open_request('https://api.anthropic.com/v1/messages',provider='Claude')
            self.assertIn('Claude',str(result.exception))
            self.assertNotIn('private-secret-key',str(result.exception))


if __name__ == '__main__':
    unittest.main()
