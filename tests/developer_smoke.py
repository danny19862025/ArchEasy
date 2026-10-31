"""Pure planning and fixture integration tests. Never use a real package manager."""
import contextlib
import fcntl
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('archeasy_setup', ROOT / 'scripts/gaming.py')
runtime = importlib.util.module_from_spec(spec)
sys.modules['archeasy_setup'] = runtime
spec.loader.exec_module(runtime)
spec = importlib.util.spec_from_file_location('developer', ROOT / 'scripts/developer.py')
d = importlib.util.module_from_spec(spec)
spec.loader.exec_module(d)
d.MANIFEST = json.loads((ROOT / 'data/developer-tools.json').read_text())
AVAILABLE = {p: 'extra/' + p for item in d.MANIFEST['tools'] for p in item['packages']}
AVAILABLE['flatpak'] = 'extra/flatpak'


def request(*selected, **kwargs):
    return dict(selected=list(selected), sources={}, **kwargs)


class PlanningTests(unittest.TestCase):
    def test_manifest_defaults_and_all_four_presets(self):
        d.validate_manifest()
        self.assertEqual(sum(i.get('recommended', False) for i in d.MANIFEST['tools']), 8)
        for name in ['Recommended', 'Advanced', 'Basic Developer', 'Full Stack Developer', 'Linux App Developer', 'AI Developer']:
            plan = d.plan(request(*d.MANIFEST['presets'][name]), set(), set(), AVAILABLE)
            self.assertTrue(plan['selected'])
        self.assertNotIn('ollama', d.MANIFEST['presets']['AI Developer'])
        self.assertEqual(len(d.tools()), 52)

    def test_only_selected_and_required_dependencies(self):
        plan = d.plan(request('compose'), set(), set(), AVAILABLE)
        self.assertEqual(plan['selected'], ['docker', 'compose'])
        self.assertEqual(plan['targets'], ['extra/docker', 'extra/docker-compose'])
        self.assertNotIn('git', plan['selected'])
        self.assertEqual(d.plan(request('pip'), set(), set(), AVAILABLE)['selected'], ['python', 'pip'])
        self.assertEqual(d.plan(request('distrobox'), {'docker'}, set(), AVAILABLE)['selected'], ['distrobox'])
        self.assertEqual(d.plan(request('distrobox'), set(), set(), AVAILABLE)['selected'], ['podman', 'distrobox'])

    def test_partial_packages_are_skipped_and_targets_deduplicated(self):
        plan = d.plan(request('git', 'git'), {'git'}, set(), AVAILABLE)
        self.assertEqual(plan['targets'], ['extra/git-lfs'])
        self.assertEqual(plan['selected'], ['git'])
        self.assertEqual(d.plan(request('git'), {'git', 'git-lfs'}, set(), AVAILABLE)['operations'], [])

    def test_editors_remain_distinct_and_native_conflicts_rejected(self):
        plan = d.plan(request('code-oss', 'vscode'), set(), set(), AVAILABLE)
        self.assertIn('extra/code', plan['targets'])
        self.assertEqual(plan['steps'][0]['app'], 'com.visualstudio.code')
        req = request('code-oss', 'vscode', allow_aur=True)
        req['sources']['vscode'] = 'aur'
        with self.assertRaisesRegex(ValueError, 'Conflicting native editors'):
            d.plan(req, set(), set(), AVAILABLE)
        self.assertFalse(d.is_installed(d.tools()['vscode'], 'flatpak', {'code'}, set()))
        self.assertTrue(d.is_installed(d.tools()['vscode'], 'flatpak', set(), {'com.visualstudio.code'}))

    def test_aur_explicit_opt_in_and_official_never_substituted(self):
        with self.assertRaisesRegex(ValueError, 'Unavailable official'):
            d.plan(request('redis'), set(), set(), {})
        req = request('redis'); req['sources']['redis'] = 'aur'
        with self.assertRaisesRegex(ValueError, 'AUR is disabled'):
            d.plan(req, set(), set(), AVAILABLE)
        req['allow_aur'] = True
        plan = d.plan(req, set(), set(), AVAILABLE)
        self.assertEqual(plan['selected'], ['git', 'build', 'redis'])
        self.assertEqual(plan['steps'][0]['package'], 'redis')
        self.assertNotIn('valkey', str(plan))

    def test_rustup_conflict_preserves_distribution_rust(self):
        with self.assertRaisesRegex(ValueError, 'Existing Rust will not be replaced'):
            d.plan(request('rust'), {'rust'}, set(), AVAILABLE)
        with patch.object(d, 'query', return_value=(1, '', 'no stable toolchain')):
            plan = d.plan(request('rust'), {'rustup'}, set(), AVAILABLE)
            self.assertEqual(plan['targets'], [])
            self.assertIn(['rustup', 'toolchain', 'install', 'stable', '--profile', 'minimal'], plan['operations'])

    def test_upstream_codex_is_explicit_and_adds_node(self):
        plan = d.plan(request('codex'), set(), set(), AVAILABLE)
        self.assertEqual(plan['targets'], ['extra/openai-codex'])
        self.assertEqual(plan['steps'], [])
        req = request('codex'); req['sources']['codex'] = 'upstream'
        plan = d.plan(req, set(), set(), AVAILABLE)
        self.assertEqual(plan['selected'], ['node', 'codex'])
        self.assertIn('--ignore-scripts', plan['operations'][-1])
        self.assertNotIn('sudo', str(plan))

    def test_invalid_empty_and_unsupported_source_requests(self):
        for req in [request(), request('--root=/tmp'), request('missing'), {'selected': 'git'}, {'selected': [4]}, {'selected': ['git'], 'sources': {'git': 'shell'}}]:
            with self.subTest(req=req), self.assertRaises(ValueError):
                d.plan(req, set(), set(), AVAILABLE)

    def test_dependency_cycles_are_rejected(self):
        manifest = json.loads(json.dumps(d.MANIFEST))
        manifest['tools'][0]['requires'] = ['git']
        with patch.object(d, 'MANIFEST', manifest), self.assertRaisesRegex(ValueError, 'Cyclic'):
            d.expand(['git'])

    def test_native_preflight_exact_metadata_and_conflict(self):
        with patch.object(d, 'query', side_effect=[(0, 'Name : git\nConflicts With : None\n', ''), (0, 'extra/git 1.0\n', '')]):
            self.assertIn('extra/git', d.native_preflight(['extra/git'], set()))
        with patch.object(d, 'query', side_effect=[(0, 'Name : rustup\nConflicts With : rust\n', ''), (0, '', '')]), self.assertRaisesRegex(RuntimeError, 'conflicts with installed'):
            d.native_preflight(['extra/rustup'], {'rust'})
        with patch.object(d, 'query', return_value=(1, '', 'signature error')), self.assertRaisesRegex(RuntimeError, 'metadata'):
            d.native_preflight(['extra/git'], set())
        with patch.object(d, 'query', side_effect=[(0, 'Name : git\nConflicts With : None\n', ''), (1, '', 'dependency broken')]), self.assertRaisesRegex(RuntimeError, 'Dependency resolution failed'):
            d.native_preflight(['extra/git'], set())

    def test_manifest_injection_and_invalid_package(self):
        manifest = json.loads(json.dumps(d.MANIFEST)); manifest['tools'][0]['packages'] = ['--root=/tmp']
        with patch.object(d, 'MANIFEST', manifest), self.assertRaises(ValueError): d.validate_manifest()


class IntegrationTests(unittest.TestCase):
    @contextlib.contextmanager
    def environment(self, req, native=None, flat=None, fail=False, decline=False):
        installed = set(native or [])
        apps = set(flat or [])
        events, commands, permissions = [], [], []
        class Session:
            def checkpoint(self): pass
            def consent(self, message):
                permissions.append(message)
                if decline: raise RuntimeError('declined')
            def run(self, args):
                commands.append(args)
                if fail:
                    installed.add('git')
                    raise RuntimeError('fixture transaction failed')
                if '/usr/bin/pacman' in args:
                    installed.update(value.split('/')[-1] for value in args[args.index('--') + 1:])
                elif args[:2] == ['flatpak', 'install']:
                    apps.add(args[-1])
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, XDG_RUNTIME_DIR=directory), patch.object(d, 'Session', Session), patch.object(d, 'installed', side_effect=lambda: (installed.copy(), apps.copy())), patch.object(d, 'availability', return_value=AVAILABLE), patch.object(d, 'native_preflight', return_value='resolved dependencies'), patch.object(d, 'emit', side_effect=lambda **event: events.append(event)), patch.object(Path, 'read_text', return_value='ID=arch'):
            yield installed, apps, events, commands, permissions

    def test_full_install_review_and_repeat_skips(self):
        req = request('git')
        with self.environment(req) as (native, flat, events, commands, permissions):
            d.main('install', req)
            self.assertEqual(commands, [['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-S', '--needed', '--', 'extra/git', 'extra/git-lfs']])
            self.assertIn('extra/git-lfs', permissions[0])
            self.assertFalse(any('-Sy' == value or '--noconfirm' == value for value in commands[0]))
            self.assertTrue(any('completed' in event for event in events))
            commands.clear(); d.main('install', req); self.assertEqual(commands, [])

    def test_decline_no_mutations(self):
        req = request('git')
        with self.environment(req, decline=True) as (_, _, events, commands, _):
            with self.assertRaisesRegex(RuntimeError, 'declined'): d.main('install', req)
            self.assertEqual(commands, [])
            self.assertEqual(next(e['failed'] for e in events if 'failed' in e), ['git'])

    def test_partial_failure_recovers_and_retries_missing_only(self):
        req = request('git')
        with self.environment(req, fail=True) as (native, _, events, commands, _):
            with self.assertRaisesRegex(RuntimeError, 'fixture transaction'): d.main('install', req)
            self.assertIn('git', native)
            self.assertIn({'tool_status': {'id': 'git', 'state': 'Failed'}}, events)
        with self.environment(req, native={'git'}) as (_, _, _, commands, _):
            d.main('install', req)
            self.assertEqual(commands[0][-1], 'extra/git-lfs')
            self.assertNotIn('extra/git', commands[0])

    def test_native_finishes_before_flatpak(self):
        req = request('vscode')
        with self.environment(req) as (_, _, _, commands, permissions):
            def q(*args):
                if args[:2] == ('flatpak', 'remotes'): return 0, 'flathub\thttps://dl.flathub.org/repo/\n', ''
                return 0, 'Microsoft Visual Studio Code', ''
            with patch.object(d, 'query', side_effect=q): d.main('install', req)
            self.assertEqual(commands[0][-1], 'extra/flatpak')
            self.assertEqual(commands[1], ['flatpak', 'install', '--user', 'flathub', 'com.visualstudio.code'])
            self.assertEqual(len(commands), 2)

    def test_shared_lock_prevents_developer_and_gaming_overlap(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, XDG_RUNTIME_DIR=directory):
            with open(Path(directory) / 'archeasy-packages.lock', 'a') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaisesRegex(RuntimeError, 'Another ArchEasy'):
                    with d.operation_lock(): self.fail('must not acquire')

    def test_untrusted_flatpak_remote_and_unavailable_app(self):
        session = unittest.mock.Mock()
        with patch.object(d, 'query', return_value=(0, 'flathub\thttps://untrusted.invalid/repo\n', '')), self.assertRaisesRegex(RuntimeError, 'unverified'):
            d.flathub(session, 'com.visualstudio.code')
        session.run.assert_not_called()
        with patch.object(d, 'query', side_effect=[(0, 'flathub\thttps://dl.flathub.org/repo/\n', ''), (1, '', 'missing')]), self.assertRaisesRegex(RuntimeError, 'unavailable'):
            d.flathub(session, 'com.visualstudio.code')
        session.run.assert_not_called()

    def test_official_flathub_added_only_after_permission_and_reverified(self):
        session = unittest.mock.Mock()
        with patch.object(d, 'query', side_effect=[(0, '', ''), (0, 'flathub\thttps://dl.flathub.org/repo\n', ''), (0, 'verified app', '')]):
            d.flathub(session, 'com.visualstudio.code')
        self.assertEqual(session.consent.call_count, 2)
        self.assertEqual(session.run.call_args_list[0].args[0], ['flatpak', 'remote-add', '--user', '--if-not-exists', 'flathub', 'https://dl.flathub.org/repo/flathub.flatpakrepo'])

    def test_rust_existing_default_preserved_and_missing_toolchain_approved(self):
        session = unittest.mock.Mock()
        with patch.object(d, 'query', side_effect=[(0, 'stable-x86_64-unknown-linux-gnu\nnightly-x86_64-unknown-linux-gnu (default)', ''), (0, 'rustc 1.0', '')]), patch.object(d, 'emit'):
            d.rust_toolchain(session)
        session.run.assert_not_called()
        with patch.object(d, 'query', side_effect=[(0, '', ''), (0, 'rustc 1.0', '')]), patch.object(d, 'emit'):
            d.rust_toolchain(session)
        self.assertEqual(session.run.call_args.args[0], ['rustup', 'toolchain', 'install', 'stable', '--profile', 'minimal'])
        self.assertNotIn('default', str(session.run.call_args))

    def test_npm_pins_version_disables_scripts_and_rejects_invalid_metadata(self):
        session = unittest.mock.Mock()
        metadata = json.dumps({'name': '@openai/codex', 'version': '1.2.3', 'dist.integrity': 'sha512-example'})
        with patch.object(d, 'query', side_effect=[(0, metadata, ''), (0, 'codex-cli 1.2.3', '')]), patch.object(d, 'emit'):
            d.npm_install(session)
        args = session.run.call_args.args[0]
        self.assertIn('--ignore-scripts', args); self.assertEqual(args[-1], '@openai/codex@1.2.3')
        self.assertNotIn('sudo', args); self.assertNotIn('-g', args)
        for invalid in ['{}', '{"name":"other","version":"1.2.3"}']:
            with patch.object(d, 'query', return_value=(0, invalid, '')), self.assertRaisesRegex(RuntimeError, 'Unexpected'):
                d.npm_install(session)

    def test_broken_upstream_binary_is_not_treated_as_installed(self):
        with patch.object(Path, 'is_file', return_value=True), patch.object(d, 'query', return_value=(1, '', 'missing executable dependency')):
            self.assertFalse(d.is_installed(d.tools()['codex'], 'upstream', set(), set()))

    def test_readiness_versions_toolkits_editors_and_missing_dependencies(self):
        events = []
        with patch.object(d, 'query', return_value=(0, 'fixture version 1.0', '')), patch.object(d, 'emit', side_effect=lambda **event: events.append(event)):
            d.report({'git', 'git-lfs', 'gtk4', 'libadwaita'}, {'com.visualstudio.code'}, {})
        text = ''.join(e.get('log', '') for e in events)
        for word in ['GCC', 'Python', 'Rust', 'Go', 'Node.js', 'Docker', 'Podman', 'Neovim', 'GTK4/libadwaita: installed', 'Missing essential development dependencies:']:
            self.assertIn(word, text)
        self.assertIn({'tool_status': {'id': 'vscode', 'state': 'Installed'}}, events)

    def test_readiness_panel_receives_report_and_probes_user_local_codex(self):
        events = []
        with patch.object(d, 'query', return_value=(0, 'fixture 1.0', '')) as probe, patch.object(Path, 'is_file', return_value=True), patch.object(d, 'emit', side_effect=lambda **event: events.append(event)):
            d.report(set(), set(), {'codex': 'upstream'})
        report = next(e for e in events if 'readiness_report' in e)
        self.assertIn('Codex: fixture 1.0', report['readiness_report'])
        self.assertIn('base-devel', report['missing_dependencies'])
        self.assertEqual(report['readiness_report'], ''.join(e.get('log', '') for e in events))
        self.assertTrue(any(call.args == (str(d.PREFIX / 'node_modules/.bin/codex'), '--version') for call in probe.call_args_list))

    def test_cancellation_after_native_stage_prevents_flatpak(self):
        req = request('vscode')
        with self.environment(req) as (native, _, events, commands, _):
            with patch.object(d.Session, 'checkpoint', side_effect=[None, None, RuntimeError('cancelled between stages')]):
                with self.assertRaisesRegex(RuntimeError, 'cancelled'):
                    d.main('install', req)
            self.assertEqual(len(commands), 1)
            self.assertIn('flatpak', native)
            self.assertIn({'failed': ['vscode']}, events)

    def test_conflict_probe_failure_is_not_interpreted_as_safe(self):
        with patch.object(d, 'query', return_value=(1, '', 'database error')), self.assertRaisesRegex(RuntimeError, 'Cannot verify'):
            d.conflict_present('rust')
        with patch.object(d, 'query', return_value=(127, 'rust\n', '')):
            self.assertFalse(d.conflict_present('rust'))


class AurTests(unittest.TestCase):
    def run_fixture(self, action=None):
        commit = 'a' * 40
        root_holder, commands, reviews = [], [], []
        def run(args):
            commands.append(args)
            if 'clone' in args:
                root = Path(args[-1]); root.mkdir(); root_holder.append(root)
                (root / 'PKGBUILD').write_text('pkgname=redis\npkgver=1\n')
                (root / '.SRCINFO').write_text('pkgbase = redis\n\tpkgname = redis\n')
            if args[0] == 'makepkg':
                (root_holder[0] / 'redis.pkg.tar.zst').write_text('fixture archive, not a real package')
        def consent(text):
            reviews.append(text)
            if action: action(root_holder[0], text)
        def query(*args):
            if 'rev-parse' in args: return 0, commit + '\n', ''
            if 'ls-files' in args: return 0, 'PKGBUILD\n.SRCINFO\n', ''
            if args[0] == 'makepkg': return 0, str(root_holder[0] / 'redis.pkg.tar.zst'), ''
            if args[:2] == ('pacman', '-Qp'): return 0, 'redis 1.0', ''
            return 0, '', ''
        session = unittest.mock.Mock(); session.run.side_effect = run; session.consent.side_effect = consent
        return commit, session, query, commands, reviews

    def test_aur_review_precedes_build_and_root_archive_install(self):
        commit, session, query, commands, reviews = self.run_fixture()
        with patch.object(d, 'query', side_effect=query), patch.object(d, 'availability', return_value={}), patch.object(os, 'geteuid', return_value=1000):
            d.aur_install(session, 'redis', commit)
        self.assertIn('PKGBUILD', reviews[0]); self.assertIn('Revision: ' + commit, reviews[0])
        self.assertEqual(commands[1], ['makepkg', '--nodeps', '--cleanbuild'])
        self.assertEqual(commands[2][:6], ['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-U', '--needed', '--'])
        self.assertTrue(commands[2][6].endswith('redis.pkg.tar.zst'))
        self.assertEqual(len(commands), 3)

    def test_changed_review_files_abort_before_build(self):
        def tamper(root, text):
            if 'Run the reviewed build' in text: (root / 'PKGBUILD').write_text('changed after review')
        commit, session, query, commands, reviews = self.run_fixture(tamper)
        with patch.object(d, 'query', side_effect=query), patch.object(d, 'availability', return_value={}), patch.object(os, 'geteuid', return_value=1000), self.assertRaisesRegex(RuntimeError, 'changed after review'):
            d.aur_install(session, 'redis', commit)
        self.assertEqual(len(commands), 1)

    def test_aur_missing_or_changed_repository_and_root_build_refused(self):
        with patch.object(d, 'query', return_value=(1, '', 'not found')), patch.object(d.shutil, 'which', return_value='/usr/bin/git'), self.assertRaisesRegex(RuntimeError, 'Cannot verify AUR'):
            d.aur_preflight('redis')
        with patch.object(os, 'geteuid', return_value=0), self.assertRaisesRegex(RuntimeError, 'normal desktop user'):
            d.aur_install(unittest.mock.Mock(), 'redis', 'a' * 40)
        commit, session, query, commands, reviews = self.run_fixture()
        with patch.object(d, 'query', return_value=(0, 'b' * 40, '')), patch.object(os, 'geteuid', return_value=1000), self.assertRaisesRegex(RuntimeError, 'changed since planning'):
            d.aur_install(session, 'redis', commit)

    def test_aur_dependency_unavailable_stops_before_build(self):
        commit, session, original_query, commands, _ = self.run_fixture()
        def query(*args):
            if args[:2] == ('pacman', '-T'):
                return 127, 'not-official>=1\n', ''
            return original_query(*args)
        with patch.object(d, 'query', side_effect=query), patch.object(d, 'srcinfo', return_value={'pkgname': ['redis'], 'depends': ['not-official>=1']}), patch.object(d, 'availability', return_value={}), patch.object(os, 'geteuid', return_value=1000), self.assertRaisesRegex(RuntimeError, 'unavailable officially'):
            d.aur_install(session, 'redis', commit)
        self.assertEqual(len(commands), 1)

    def test_aur_dependency_query_errors_never_start_build_or_install(self):
        for result in [(1, 'git\n', 'database failure'), (127, 'git\n', 'database failure'), (127, '', ''), (0, 'git\n', '')]:
            with self.subTest(result=result):
                commit, session, original_query, commands, _ = self.run_fixture()
                def query(*args):
                    if args[:2] == ('pacman', '-T'):
                        return result
                    return original_query(*args)
                with patch.object(d, 'query', side_effect=query), patch.object(d, 'srcinfo', return_value={'pkgname': ['redis'], 'depends': ['git']}), patch.object(os, 'geteuid', return_value=1000), self.assertRaisesRegex(RuntimeError, 'Cannot inspect AUR dependencies'):
                    d.aur_install(session, 'redis', commit)
                self.assertEqual(len(commands), 1)

    def test_aur_wrong_archive_identity_never_installs(self):
        commit, session, original_query, commands, _ = self.run_fixture()
        def query(*args):
            if args[:2] == ('pacman', '-Qp'): return 0, 'unrelated-package 1.0', ''
            return original_query(*args)
        with patch.object(d, 'query', side_effect=query), patch.object(d, 'availability', return_value={}), patch.object(os, 'geteuid', return_value=1000), self.assertRaisesRegex(RuntimeError, 'identity differs'):
            d.aur_install(session, 'redis', commit)
        self.assertFalse(any('-U' in command for command in commands))

    def test_aur_availability_on_system_without_git(self):
        payload = json.dumps({'results': [{'Name': 'redis', 'PackageBase': 'redis'}]}).encode()
        with patch.object(d.shutil, 'which', return_value=None), patch.object(d.urllib.request, 'urlopen', return_value=io.BytesIO(payload)):
            self.assertIsNone(d.aur_preflight('redis'))


if __name__ == '__main__':
    unittest.main()
