"""Package operations use fake executables; never touch the host package manager."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class StoreTests(unittest.TestCase):
    def test_queue_order_and_failure_stop(self):
        for fail in ('0', '1'):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory() as directory:
                path = Path(directory)
                trace = path / 'trace'
                for name in ('sudo', 'paru', 'flatpak'):
                    command = path / name
                    command.write_text('#!/bin/bash\nprintf "%s %s\\n" "${0##*/}" "$*" >> "$TRACE"\n[[ ${0##*/} != paru || $FAIL != 1 ]]\n')
                    command.chmod(0o700)
                result = subprocess.run(['/usr/bin/bash', str(ROOT/'scripts/install-queue.sh'), 'Pacman', '', 'fish', 'AUR', '', 'example-bin', 'Flatpak', 'flathub', 'org.example.App', 'Pacman', '', 'kitty'], env={**os.environ, 'PATH':directory, 'TRACE':str(trace), 'FAIL':fail}, stdin=subprocess.DEVNULL, capture_output=True)
                lines = trace.read_text().splitlines()
                self.assertEqual(result.returncode, int(fail))
                self.assertEqual(lines[:2], ['sudo pacman -Syu --needed -- fish', 'paru -Syu --needed -- example-bin'])
                self.assertEqual(len(lines), 2 if fail == '1' else 4)
                if fail == '0':
                    self.assertEqual(lines[2:], ['flatpak install --user -- flathub org.example.App', 'sudo pacman -S --needed -- kitty'])
    def test_queue_rejects_entire_invalid_batch_before_installing(self):
        result = subprocess.run(['/usr/bin/bash', str(ROOT/'scripts/install-queue.sh'), 'Pacman', '', 'fish', 'Pacman', '', '--root=/tmp'], stdin=subprocess.DEVNULL, capture_output=True)
        self.assertNotEqual(result.returncode, 0)

class FirstRunTests(unittest.TestCase):
    def test_fresh_setup_builds_paru_as_user_and_stops_on_failed_pacman(self):
        for fail in ('0', '1'):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory() as directory:
                path = Path(directory)
                tools = path / 'tools'
                tools.mkdir()
                for name in ('mktemp', 'cat'):
                    (tools/name).symlink_to('/usr/bin/' + name)
                scripts = {
                    'sudo': '#!/bin/bash\nprintf "sudo %s\\n" "$*" >> "$TRACE"\nexit "$FAIL"\n',
                    'git': '#!/bin/bash\nprintf "git\\n" >> "$TRACE"\n/usr/bin/mkdir -p "${@: -1}"\nprintf "# review me" > "${@: -1}/PKGBUILD"\n',
                    'makepkg': '#!/bin/bash\nprintf "makepkg uid=%s\\n" "$EUID" >> "$TRACE"\nprintf "#!/bin/bash\\nexit 0\\n" > "$TOOLS/paru"\n/usr/bin/chmod 700 "$TOOLS/paru"\n',
                    'flatpak': '#!/bin/bash\nprintf "flatpak %s\\n" "$*" >> "$TRACE"\n',
                }
                for name, contents in scripts.items():
                    command = tools/name
                    command.write_text(contents)
                    command.chmod(0o700)
                result = subprocess.run(['/usr/bin/bash', str(ROOT/'scripts/first-run.sh')], input='y\n', text=True, capture_output=True, env={**os.environ, 'PATH':str(tools), 'TMPDIR':directory, 'TOOLS':str(tools), 'TRACE':str(path/'trace'), 'FAIL':fail})
                trace = (path/'trace').read_text()
                self.assertEqual(result.returncode, int(fail))
                self.assertTrue(trace.startswith('sudo pacman -Syu --needed -- base-devel git flatpak python'))
                self.assertEqual('makepkg' in trace, fail == '0')
                self.assertEqual('flatpak remote-add --user --if-not-exists flathub' in trace, fail == '0')
                self.assertNotIn('makepkg uid=0', trace)


class PortableRestoreTests(unittest.TestCase):
    def run_restore(self, root, plan):
        import base64
        import json
        return subprocess.run(['/usr/bin/python3', str(ROOT/'scripts/restore-exported-configs.py')], env={**os.environ, 'XDG_CONFIG_HOME':str(root/'config'), 'XDG_DATA_HOME':str(root/'data'), 'ARCHEASY_CONFIG_PAYLOAD':base64.b64encode(json.dumps(plan).encode()).decode()}, capture_output=True)
    def test_restore_makes_backup_and_rebases_config_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'config/fish').mkdir(parents=True)
            (root/'config/fish/config.fish').write_text('original')
            plan={'files':[{'path':'fish/config.fish','bytes':list(b'new __CONFIG_DIR__ config')}]}
            result=self.run_restore(root,plan)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual((root/'config/fish/config.fish').read_text(),'new '+str(root/'config')+' config')
            snapshots=list((root/'data/archeasy/system-restore').glob('*/files/fish/config.fish'))
            self.assertEqual(len(snapshots),1)
            self.assertEqual(snapshots[0].read_text(),'original')
    def test_restore_rejects_linked_targets_before_any_write(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'managed').mkdir()
            (root/'managed/config.fish').write_text('manager original')
            (root/'config').mkdir()
            (root/'config/fish').symlink_to(root/'managed')
            result=self.run_restore(root,{'files':[{'path':'fish/config.fish','bytes':[65]}]})
            self.assertNotEqual(result.returncode,0)
            self.assertEqual((root/'managed/config.fish').read_text(),'manager original')

    def test_restore_preserves_binary_bytes_and_only_rebases_text(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            contents = {'app/font.ttf': b'\xff__CONFIG_DIR__\x80',
                        'app/cache.db': b'\0__CONFIG_DIR__',
                        'app/settings.conf': b'path=__CONFIG_DIR__/app'}
            plan = {'files': [{'path': name, 'bytes': list(data)} for name, data in contents.items()]}
            result = self.run_restore(root, plan)
            self.assertEqual(result.returncode, 0, result.stderr)
            for name, original in contents.items():
                expected = original.replace(b'__CONFIG_DIR__', str(root/'config').encode()) if name.endswith('.conf') else original
                self.assertEqual((root/'config'/name).read_bytes(), expected)

if __name__ == '__main__':
    unittest.main()
