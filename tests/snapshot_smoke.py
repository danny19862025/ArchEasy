"""Exercise real ZIP round trips and hostile archives, without installing packages."""
import importlib.util
import errno
import json
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

HELPER = Path(__file__).resolve().parents[1]/'scripts/portable-snapshot.py'
spec = importlib.util.spec_from_file_location('snapshot', HELPER)
snapshot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(snapshot)


class SnapshotTests(unittest.TestCase):
    def request(self, root, paths):
        return {'home': str(root/'home'), 'destination': str(root/'backup.zip'), 'paths': [str(p) for p in paths], 'helper': HELPER.read_text(), 'script': '#!/bin/bash\necho test\n'}

    def test_roundtrip_binary_empty_directory_modes_and_standalone_restore(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root/'home/Projects/app'
            (source/'empty').mkdir(parents=True)
            program = source/'run'
            program.write_bytes(b'\x00\xff' * 1000)
            program.chmod(0o750)
            (source/'link').symlink_to('/etc/passwd')
            req = self.request(root, [source, program])
            snapshot.create(req)
            self.assertEqual(stat.S_IMODE((root/'backup.zip').stat().st_mode), 0o600)
            with zipfile.ZipFile(root/'backup.zip') as archive:
                manifest = json.loads(archive.read('manifest.json'))
                self.assertEqual(manifest['skipped'], 1)
                self.assertEqual(len(manifest['entries']), 3)
                helper = root/'restore-snapshot.py'
                helper.write_bytes(archive.read('restore-snapshot.py'))
            result = subprocess.run(['python3', str(helper), 'restore', str(root/'backup.zip'), str(root/'restored')], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            restored = root/'restored/home/Projects/app'
            self.assertEqual((restored/'run').read_bytes(), program.read_bytes())
            self.assertEqual(stat.S_IMODE((restored/'run').stat().st_mode), 0o750)
            self.assertTrue((restored/'empty').is_dir())
            self.assertFalse((restored/'link').exists())
            self.assertEqual((root/'restored/my-system.sh').read_text(), req['script'])
            with self.assertRaises(FileExistsError):
                snapshot.create(req)
            with self.assertRaises(ValueError):
                snapshot.restore(root/'backup.zip', root/'restored')

    def test_selection_and_destination_validation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            home = root/'home'
            home.mkdir()
            (root/'outside').write_text('private')
            (home/'linked').symlink_to(root)
            for path in [home, root/'outside', home/'linked/outside']:
                with self.subTest(path=path), self.assertRaises(ValueError):
                    snapshot.create(self.request(root, [path]))
            (home/'files').mkdir()
            req = self.request(root, [home/'files'])
            req['destination'] = str(home/'files/backup.zip')
            with self.assertRaises(ValueError):
                snapshot.create(req)

    def test_sensitive_descendants_are_excluded_and_policy_matches_cloud(self):
        policy = json.loads((HELPER.parent.parent/'data/backup-exclusions.json').read_text())
        self.assertEqual([str(p) for p in snapshot.EXCLUDED], policy)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = root/'home/.config'
            for name in ('google-chrome/Default/Cookies', 'rclone/rclone.conf', 'kitty/kitty.conf'):
                path = config/name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('fixture')
            ssh = root/'home/.ssh'
            ssh.mkdir()
            (ssh/'id_ed25519').write_text('private')
            snapshot.create(self.request(root, [config, ssh]))
            with zipfile.ZipFile(root/'backup.zip') as archive:
                names = archive.namelist()
                self.assertIn('home/.config/kitty/kitty.conf', names)
                self.assertFalse(any('google-chrome' in n or '/rclone' in n or '/.ssh' in n for n in names))
                self.assertEqual(json.loads(archive.read('manifest.json'))['skipped'], 3)

    def test_no_hard_links_falls_back_without_overwriting_files_or_links(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root/'home/file'
            source.parent.mkdir()
            source.write_bytes(b'\xff\0binary')
            req = self.request(root, [source])
            target = root/'backup.zip'
            for code in (errno.EOPNOTSUPP, errno.EPERM, errno.ENOSYS, errno.EXDEV):
                with self.subTest(errno=code), mock.patch.object(snapshot.os, 'link', side_effect=OSError(code, 'unsupported')):
                    snapshot.create(req)
                    with zipfile.ZipFile(target) as archive:
                        self.assertEqual(archive.read('home/file'), source.read_bytes())
                    self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o600)
                    original = target.read_bytes()
                    with self.assertRaises(FileExistsError):
                        snapshot.create(req)
                    self.assertEqual(target.read_bytes(), original)
                    target.unlink()
                    target.symlink_to(root/'missing')
                    with self.assertRaises(FileExistsError):
                        snapshot.create(req)
                    self.assertTrue(target.is_symlink())
                    self.assertFalse((root/'missing').exists())
                    target.unlink()

    def test_fallback_copy_failure_removes_partial_archive(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root/'home/file'
            source.parent.mkdir()
            source.write_text('original')
            def fail_copy(source, output):
                output.write(b'partial')
                raise OSError(errno.ENOSPC, 'full')
            with mock.patch.object(snapshot.os, 'link', side_effect=OSError(errno.EOPNOTSUPP, 'unsupported')), mock.patch.object(snapshot.shutil, 'copyfileobj', side_effect=fail_copy):
                with self.assertRaises(OSError):
                    snapshot.create(self.request(root, [source]))
            self.assertFalse((root/'backup.zip').exists())
            self.assertEqual(list(root.glob('.archeasy-snapshot-*')), [])
            self.assertEqual(source.read_text(), 'original')

    def test_tampering_traversal_links_and_manifest_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root/'home/file'
            source.parent.mkdir()
            source.write_text('original')
            snapshot.create(self.request(root, [source]))
            with zipfile.ZipFile(root/'backup.zip') as archive:
                original = {name: archive.read(name) for name in archive.namelist()}
            for kind in ['checksum', 'traversal', 'link', 'unlisted']:
                contents = dict(original)
                if kind == 'checksum':
                    contents['home/file'] = b'tampered'
                elif kind == 'traversal':
                    contents['../escaped'] = b'bad'
                elif kind == 'unlisted':
                    contents['home/unlisted'] = b'bad'
                archive_path = root/(kind+'.zip')
                with zipfile.ZipFile(archive_path, 'w') as archive:
                    for name, data in contents.items():
                        if kind == 'link' and name == 'home/file':
                            info = zipfile.ZipInfo(name)
                            info.external_attr = (stat.S_IFLNK | 0o777) << 16
                            archive.writestr(info, data)
                        else:
                            archive.writestr(name, data)
                with self.subTest(kind=kind), self.assertRaises(ValueError):
                    snapshot.restore(archive_path, root/(kind+'-out'))
            self.assertFalse((root/'escaped').exists())
            self.assertEqual(source.read_text(), 'original')

    def test_files_only_and_size_failure_publish_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            file = root/'home/file'
            file.parent.mkdir()
            file.write_text('data')
            req = self.request(root, [file])
            req['script'] = ''
            snapshot.create(req)
            with zipfile.ZipFile(root/'backup.zip') as archive:
                self.assertNotIn('my-system.sh', archive.namelist())
            req['destination'] = str(root/'too-large.zip')
            old_limit = snapshot.LIMIT
            try:
                snapshot.LIMIT = 1
                with self.assertRaises(ValueError):
                    snapshot.create(req)
            finally:
                snapshot.LIMIT = old_limit
            self.assertFalse((root/'too-large.zip').exists())
            self.assertEqual(list(root.glob('.archeasy-snapshot-*')), [])


if __name__ == '__main__':
    unittest.main()
