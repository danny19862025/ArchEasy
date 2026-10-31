"""Gaming checks use fixtures only; never invoke a real package installation."""
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('gaming', ROOT / 'scripts/gaming.py')
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)


def session():
    s = g.Session.__new__(g.Session)
    s.messages = queue.Queue()
    s.cancelled = False
    return s


class GamingTests(unittest.TestCase):
    def test_distribution_detection(self):
        for value in ['ID=arch', 'ID=endeavouros\nID_LIKE=arch', 'ID=manjaro\nID_LIKE="arch linux"']:
            self.assertTrue(g.supported(value))
        for value in ['ID=debian', 'ID=archfake', 'ID_LIKE=archlinux']:
            self.assertFalse(g.supported(value))

    def test_gpu_pairs_and_hybrid(self):
        for vendor, driver, pair in [('0x1002', 'amdgpu', 'vulkan-radeon'), ('0x8086', 'xe', 'vulkan-intel'), ('0x8086', 'i915', 'vulkan-intel'), ('0x10de', 'nvidia', 'nvidia-utils'), ('0x10de', 'nouveau', 'vulkan-nouveau')]:
            packages = g.graphics([(vendor, driver)])
            self.assertIn(pair, packages)
            self.assertIn('lib32-' + pair, packages)
            self.assertFalse(any(p.startswith('nvidia-open') or p.endswith('dkms') for p in packages))
        packages = g.graphics([('0x8086', 'i915'), ('0x10de', 'nvidia')])
        self.assertIn('lib32-vulkan-intel', packages)
        self.assertIn('lib32-nvidia-utils', packages)
        self.assertEqual(len(packages), len(set(packages)))
        for devices in [[], [('0x1002', 'radeon')], [('0x10de', '')], [('0xbeef', 'unknown')]]:
            with self.assertRaises(RuntimeError):
                g.graphics(devices)

    def test_skip_installed_native_and_flatpak(self):
        self.assertEqual(g.plan(['steam'], {'steam'}, set(), {}, []), ([], []))
        self.assertEqual(g.plan(['steam'], {'flatpak'}, {'com.valvesoftware.Steam'}, {}, []), ([], []))
        targets, fallback = g.plan(['heroic'], {'flatpak'}, set(), {}, [])
        self.assertEqual(targets, [])
        self.assertEqual(fallback, [('Heroic Games Launcher', 'com.heroicgameslauncher.hgl')])

    def test_official_preferred_partial_groups_and_unavailable(self):
        available = {'steam': 'multilib/steam', 'mangohud': 'extra/mangohud', 'lib32-mangohud': 'multilib/lib32-mangohud', 'flatpak': 'extra/flatpak'}
        self.assertEqual(g.plan(['steam'], set(), set(), available, [])[0], ['multilib/steam'])
        self.assertEqual(g.plan(['mangohud'], {'mangohud'}, set(), available, [])[0], ['multilib/lib32-mangohud'])
        self.assertEqual(g.plan(['heroic'], set(), set(), available, [])[0], ['extra/flatpak'])
        with self.assertRaisesRegex(RuntimeError, 'umu-launcher'):
            g.plan(['umu'], set(), set(), available, [])
        with self.assertRaisesRegex(RuntimeError, 'lib32-mesa'):
            g.plan(['steam'], {'steam'}, set(), {}, ['lib32-mesa'])

    def test_third_party_repositories_not_used(self):
        def fake(*args):
            if args[0] == 'pacman-conf':
                return 0, 'cachyos\ncore\nextra\nmultilib\ncustom\n', ''
            self.assertIn(args[-1], g.OFFICIAL)
            return 0, args[-1] + ' steam 1.0\n', ''
        with patch.object(g, 'query', side_effect=fake):
            self.assertEqual(g.availability(), {'steam': 'core/steam'})

    def test_cancellation_and_db_lock(self):
        s = session()
        s.messages.put({'cancel': True})
        with self.assertRaisesRegex(RuntimeError, 'Cancelled'):
            s.checkpoint()
        s = session()
        with patch.object(Path, 'exists', return_value=True):
            with self.assertRaisesRegex(RuntimeError, 'Another package operation'):
                s.checkpoint()

    def test_permission_declined_and_yes_is_explicit(self):
        for answer in ['no', '', '1']:
            s = session()
            with patch.object(s, 'checkpoint'), patch.object(s.messages, 'get', return_value={'answer': answer}), patch.object(g, 'emit'):
                with self.assertRaisesRegex(RuntimeError, 'declined'):
                    s.consent('permission')
        s = session()
        with patch.object(s, 'checkpoint'), patch.object(s.messages, 'get', return_value={'answer': 'yes'}), patch.object(g, 'emit'):
            s.consent('permission')

    def test_streaming_interactive_transaction_and_cancellation(self):
        # A fake package manager prints a prompt without newline, then reads a GUI response.
        s = session()
        events = []
        def emit(**event):
            events.append(event)
            if event.get('log') == 'Proceed? ':
                s.messages.put({'cancel': True})
                s.messages.put({'answer': 'no'})
        with patch.object(Path, 'exists', return_value=False), patch.object(g, 'emit', side_effect=emit):
            with self.assertRaisesRegex(RuntimeError, 'Cancelled'):
                s.run([sys.executable, '-u', '-c', 'import sys; assert sys.stdin.isatty() and sys.stdout.isatty(); print("Proceed? ", end="", flush=True); print("answer=" + input())'])
        self.assertTrue(any('answer=no' in e.get('log', '') for e in events))

    def test_failed_transaction_and_disconnect(self):
        s = session()
        with patch.object(Path, 'exists', return_value=False), patch.object(g, 'emit'):
            with self.assertRaisesRegex(RuntimeError, 'failed'):
                s.run([sys.executable, '-c', 'raise SystemExit(5)'])
        s = session()
        def emit(**event):
            if event.get('log') == 'waiting':
                s.messages.put({'disconnect': True, 'cancel': True})
        with patch.object(Path, 'exists', return_value=False), patch.object(g, 'emit', side_effect=emit):
            with self.assertRaises(RuntimeError):
                s.run([sys.executable, '-u', '-c', 'print("waiting",end="",flush=True); input()'])

    def test_multilib_backup_preserves_existing_config_and_refuses_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'pacman.conf'
            old = '[options]\nColor\n# custom comment\n#[multilib]\n#Include = /etc/pacman.d/mirrorlist\n'
            path.write_text(old)
            path.chmod(0o640)
            helper = g.MULTILIB_HELPER.replace("'/etc/pacman.conf'", repr(str(path))).replace("dir='/etc'", 'dir=' + repr(directory))
            def run(expected):
                return subprocess.run([sys.executable, '-I', '-c', helper, expected], capture_output=True, text=True)
            self.assertNotEqual(run('stale').returncode, 0)
            self.assertEqual(path.read_text(), old)
            result = run(old)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(path.with_name('pacman.conf.archeasy-gaming.bak').read_text(), old)
            self.assertIn('\n[multilib]\nInclude = /etc/pacman.d/mirrorlist', path.read_text())
            self.assertIn('# custom comment', path.read_text())
            self.assertEqual(path.stat().st_mode & 0o777, 0o640)
            path.write_text(old)
            self.assertNotEqual(run(old).returncode, 0)  # backup is never overwritten
            self.assertEqual(path.read_text(), old)

    def test_catalog_protocol_and_invalid_selection(self):
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/gaming.py'), 'catalog'], capture_output=True, text=True, check=True)
        catalog = json.loads(result.stdout.splitlines()[0])['catalog']
        self.assertEqual(len(catalog), 12)
        self.assertFalse(next(c for c in catalog if c['id'] == 'gamescope')['recommended'])
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/gaming.py'), 'install', '--bad'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn('Invalid gaming request', result.stdout)

    def test_full_plan_retry_and_shared_lock(self):
        class FakeSession:
            commands = []
            def checkpoint(self): pass
            def consent(self, text): pass
            def run(self, args): self.commands.append(args)
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, XDG_RUNTIME_DIR=directory), patch.object(g, 'Session', FakeSession), patch.object(g, 'check', return_value=(set(), set(), [])), patch.object(g, 'repositories', return_value=['core', 'extra', 'multilib']), patch.object(g, 'availability', return_value={'steam': 'multilib/steam'}), patch.object(g, 'installed', side_effect=[(set(), set()), ({'steam'}, set()), ({'steam'}, set()), ({'steam'}, set())]), patch.object(g, 'query', return_value=(0, '', '')), patch.object(g, 'emit'):
            g.main('install', ['steam'])
            self.assertEqual(FakeSession.commands, [['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-Syu', '--needed', '--', 'multilib/steam']])
            self.assertNotIn('--noconfirm', FakeSession.commands[0])
            FakeSession.commands.clear()
            with patch.object(g, 'check', return_value=({'steam'}, set(), [])):
                g.main('install', ['steam'])
                self.assertEqual(FakeSession.commands, [])
            with open(Path(directory) / 'archeasy-packages.lock', 'a') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaisesRegex(RuntimeError, 'already active'):
                    g.main('install', ['steam'])

    def test_multilib_only_when_needed(self):
        self.assertTrue(g.needs_multilib(['steam'], set(), set(), []))
        self.assertFalse(g.needs_multilib(['steam'], {'steam'}, set(), []))
        self.assertFalse(g.needs_multilib(['steam'], set(), {'com.valvesoftware.Steam'}, []))
        self.assertTrue(g.needs_multilib([], set(), set(), ['lib32-mesa']))

    def test_nvidia_library_mismatch_fails_without_installing(self):
        for version, okay in [('580.10-1', True), ('580.10-2', True), ('590.10-1', False), ('', False)]:
            with patch.object(g, 'query', side_effect=[(0, 'nvidia-utils 580.10-1', ''), (0, 'Version : ' + version, '')]):
                args = ({'nvidia-utils'}, {'lib32-nvidia-utils': 'multilib/lib32-nvidia-utils'}, ['nvidia-utils'])
                if okay:
                    g.validate_nvidia_pair(*args)
                else:
                    with self.assertRaisesRegex(RuntimeError, 'does not match'):
                        g.validate_nvidia_pair(*args)

    def test_native_then_flatpak_and_remote_validation(self):
        class FakeSession:
            commands = []
            def checkpoint(self): pass
            def consent(self, text): pass
            def run(self, args): self.commands.append(args)
        def query(*args):
            if args[0:2] == ('flatpak', 'remotes'):
                return 0, 'flathub\thttps://dl.flathub.org/repo/\n', ''
            return 0, '', ''
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, XDG_RUNTIME_DIR=directory), patch.object(g, 'Session', FakeSession), patch.object(g, 'check', return_value=(set(), set(), [])), patch.object(g, 'repositories', return_value=['multilib']), patch.object(g, 'availability', return_value={'flatpak': 'extra/flatpak'}), patch.object(g, 'installed', side_effect=[(set(), set()), ({'flatpak'}, {'com.heroicgameslauncher.hgl'})]), patch.object(g, 'query', side_effect=query), patch.object(g, 'emit'):
            g.main('install', ['heroic'])
            self.assertEqual(FakeSession.commands[0], ['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-Syu', '--needed', '--', 'extra/flatpak'])
            self.assertEqual(FakeSession.commands[1], ['flatpak', 'install', '--user', 'flathub', 'com.heroicgameslauncher.hgl'])
            self.assertEqual(len(FakeSession.commands), 2)
            FakeSession.commands.clear()
            with patch.object(g, 'installed', return_value=({'flatpak'}, set())), patch.object(g, 'query', return_value=(0, 'flathub\thttps://untrusted.invalid/repo\n', '')):
                with self.assertRaisesRegex(RuntimeError, 'unexpected URL'):
                    g.main('install', ['heroic'])
            self.assertEqual(FakeSession.commands, [])

    def test_multilib_decline_never_executes_privileged_helper(self):
        class Decline:
            def checkpoint(self): pass
            def consent(self, text): raise RuntimeError('declined')
            def run(self, args): raise AssertionError('must not run')
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, XDG_RUNTIME_DIR=directory), patch.object(g, 'Session', Decline), patch.object(g, 'check', return_value=(set(), set(), [])), patch.object(g, 'repositories', return_value=['core', 'extra']), patch.object(Path, 'read_text', return_value='#[multilib]\n#Include = /etc/pacman.d/mirrorlist'):
            with self.assertRaisesRegex(RuntimeError, 'declined'):
                g.main('install', ['steam'])

    def test_system_check_reports_gpu_tools_and_missing_vulkan(self):
        events = []
        def query(*args):
            if args[0] == 'lspci':
                return 0, '00:02.0 VGA compatible controller: Intel GPU\n\tKernel driver in use: i915\n00:03.0 Ethernet controller: Network\n\tKernel driver in use: network\n', ''
            return 127, '', 'vulkaninfo is missing'
        with patch.object(Path, 'read_text', return_value='ID=arch'), patch.object(g, 'devices', return_value=[('0x8086', 'i915')]), patch.object(g, 'installed', return_value=({'steam'}, set())), patch.object(g, 'query', side_effect=query), patch.object(g, 'emit', side_effect=lambda **event: events.append(event)):
            native, flat, dependencies = g.check()
        logs = ''.join(e.get('log', '') for e in events)
        self.assertIn('Intel GPU', logs)
        self.assertNotIn('driver in use: network', logs)
        self.assertIn('Steam: installed', logs)
        self.assertIn('Vulkan: unavailable or unverified', logs)
        self.assertIn('Missing graphics dependencies:', logs)
        self.assertIn('lib32-vulkan-intel', dependencies)
        for vendor, driver, native, message in [('0x1002', 'amdgpu', {'amdvlk-git'}, 'AMDVLK'), ('0x1002', 'amdgpu', {'vulkan-amdgpu-pro'}, 'proprietary AMD'), ('0x10de', 'nvidia', {'nvidia-470xx-utils'}, 'legacy NVIDIA')]:
            with patch.object(Path, 'read_text', return_value='ID=arch'), patch.object(g, 'devices', return_value=[(vendor, driver)]), patch.object(g, 'installed', return_value=(native, set())), patch.object(g, 'query', return_value=(0, '', '')), patch.object(g, 'emit'):
                with self.assertRaisesRegex(RuntimeError, message):
                    g.check()


if __name__ == '__main__':
    unittest.main()
