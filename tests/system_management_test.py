"""Pure/mocked management tests. Never modifies host packages or snapshots."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('management', ROOT / 'scripts/system-management.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class ManagementTests(unittest.TestCase):
    def test_filesystem_detection_and_malformed_output(self):
        mount = {'target': '/', 'fstype': 'btrfs', 'source': '/dev/test', 'options': 'subvol=/@'}
        self.assertEqual(m.filesystem(lambda _: json.dumps({'filesystems': [mount]})), mount)
        for value in ['bad', '{}', '{"filesystems":[{"target":"/home"}]}']:
            with self.subTest(value=value), self.assertRaises((ValueError, m.OperationError)):
                m.filesystem(lambda _: value)

    def test_snapshot_eligibility_and_conflicts(self):
        for fs, snapper, timeshift, expected in [
            ('btrfs', True, False, 'snapper'), ('btrfs', True, True, 'conflict'),
            ('ext4', True, False, 'files'), ('xfs', False, False, 'files'),
            ('unknown', False, False, 'files'), ('ext4', False, True, 'timeshift')]:
            self.assertEqual(m.snapshot_backend({'fstype': fs}, snapper, timeshift), expected)

    def test_destination_validation(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            self.assertEqual(m.destination(str(path / 'new')), str(path / 'new'))
            (path / 'link').symlink_to(path, target_is_directory=True)
            for value in ['/', str(Path.home()), 'relative', str(path / '../bad'), str(path), str(path / 'link/new'), 'a\x00b']:
                with self.subTest(value=value), self.assertRaises(m.OperationError):
                    m.destination(value)

    def test_update_plan_and_partial_upgrade_prevention(self):
        calls = []
        def fake(args, **kwargs):
            calls.append(args)
            return 'linux 6.1 -> 6.2\nmesa 1 -> 2'
        plan = m.update_plan(fake)
        self.assertEqual(plan['transaction'], ['sudo', 'pacman', '-Syu'])
        self.assertEqual(plan['critical_or_reboot_review'], ['linux', 'mesa'])
        self.assertEqual(calls, [['checkupdates']])
        for value in ['linux ???', '--root=/ 1 -> 2', 'foo;touch 1 -> 2']:
            with self.assertRaises(m.OperationError):
                m.parse_updates(value)

    def test_lock_process_disk_and_database_errors(self):
        with tempfile.TemporaryDirectory() as folder:
            lock = Path(folder) / 'db.lck'
            lock.touch()
            with self.assertRaisesRegex(m.OperationError, 'lock exists'):
                m.preflight(lock, 10**10, lambda _: '')
            self.assertTrue(lock.exists())
            lock.unlink()
            with self.assertRaisesRegex(m.OperationError, 'Another package'):
                m.preflight(lock, 10**10, lambda _: 'pacman\n')
            with self.assertRaisesRegex(m.OperationError, '2 GiB'):
                m.preflight(lock, 10, lambda _: '')
            with self.assertRaisesRegex(m.OperationError, 'repositories'):
                m.preflight(lock, 10**10, lambda _: '')
            def broken(args):
                if args == ['pacman', '-Dk']:
                    raise m.OperationError('database broken')
                return 'core' if args[0] == 'pacman-conf' else ''
            with self.assertRaisesRegex(m.OperationError, 'database broken'):
                m.preflight(lock, 10**10, broken)

    def test_pci_parser_separates_graphics_from_cpu_and_network(self):
        pci = '00:00.0 Host bridge: Intel CPU\n00:02.0 VGA compatible controller: AMD GPU\n\tKernel driver in use: amdgpu\n01:00.0 Network controller: Intel WiFi\n'
        graphics = m.graphics_devices(pci)
        self.assertIn('amdgpu', graphics)
        self.assertNotIn('Intel', graphics)
        self.assertEqual(m.graphics_devices('malformed output'), '')

    def test_driver_advice_requires_vendor_and_repository_evidence(self):
        self.assertIn('Review vulkan-radeon', m.driver_advice('AMD VGA', [], ['vulkan-radeon']))
        self.assertNotIn('Review vulkan-radeon', m.driver_advice('Intel VGA', [], ['vulkan-radeon']))
        self.assertNotIn('Review mesa:', m.driver_advice('Intel VGA', ['mesa'], ['mesa']))
        report = m.driver_advice('NVIDIA VGA', ['nvidia-dkms', 'linux', 'linux-lts', 'linux-headers'], [])
        self.assertIn('Missing linux-lts-headers', report)
        self.assertNotIn('Missing linux-headers', report)
        self.assertIn('Secure Boot', report)

    def test_missing_utilities_network_failure_timeout_and_cancel(self):
        with self.assertRaisesRegex(m.OperationError, 'unavailable'):
            m.run(['/nonexistent/archeasy-tool'])
        with self.assertRaisesRegex(m.OperationError, 'exit 7'):
            m.run(['/usr/bin/python3', '-c', 'import sys; print("network failed", file=sys.stderr); sys.exit(7)'])
        with self.assertRaisesRegex(m.OperationError, 'timed out'):
            m.run(['/usr/bin/python3', '-c', 'import time; time.sleep(5)'], timeout=.05)
        cancel = threading.Event()
        cancel.set()
        with self.assertRaisesRegex(m.OperationError, 'Cancelled before'):
            m.run(['/nonexistent'], cancel=cancel)
        cancel.clear()
        timer = threading.Timer(.1, cancel.set)
        timer.start()
        try:
            with self.assertRaisesRegex(m.OperationError, 'Cancelled'):
                m.run(['/usr/bin/python3', '-c', 'import time; time.sleep(5)'], cancel=cancel)
        finally:
            timer.join()

    def test_privilege_failure_and_mutations_require_terminal(self):
        with self.assertRaisesRegex(m.OperationError, 'interactive terminal'):
            m.dispatch({'action': 'snapshot'})
        with patch.object(m.os, 'geteuid', return_value=1000), patch.object(m, 'snapshot_context', return_value=({'fstype': 'btrfs'}, 'snapper')), patch.object(m, 'run', side_effect=m.OperationError('sudo denied')):
            with self.assertRaisesRegex(m.OperationError, 'sudo denied'):
                m.dispatch({'action': 'snapshot'}, interactive=True)

    def test_official_update_rechecks_and_stops_on_snapshot_failure(self):
        with patch.object(m.os, 'geteuid', return_value=1000), patch.object(m, 'preflight') as preflight, patch.object(m, 'update_plan', return_value={}), patch('builtins.input', return_value='UPGRADE'), patch.object(m, 'run', return_value='') as run:
            m.dispatch({'action': 'official'}, interactive=True)
            self.assertEqual(preflight.call_count, 2)
            run.assert_called_once_with(['sudo', 'pacman', '-Syu'], interactive=True)
        with patch.object(m.os, 'geteuid', return_value=1000), patch.object(m, 'preflight'), patch.object(m, 'update_plan', return_value={}), patch('builtins.input', return_value='UPGRADE'), patch.object(m, 'snapshot_context', return_value=({}, 'conflict')), patch.object(m, 'run') as run:
            with self.assertRaises(m.OperationError):
                m.dispatch({'action': 'official', 'snapshot': 'snapper'}, interactive=True)
            run.assert_not_called()

    def test_persistent_failures_and_interrupted_state(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'state_dir', return_value=Path(folder)), patch.object(m, 'dispatch', side_effect=m.OperationError('network failed')):
            with self.assertRaises(m.OperationError):
                m.execute({'action': 'updates'})
            record = json.loads(next(Path(folder).glob('*.json')).read_text())
            self.assertIn('failed', record['status'])
            self.assertIn('network failed', m.history())
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'state_dir', return_value=Path(folder)), patch.object(m, 'dispatch', side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                m.execute({'action': 'updates'})
            self.assertIn('completion is unknown', m.history())

    def test_interactive_output_is_logged_without_keyboard_input(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'TERMINAL_LOG', Path(folder) / 'output.log'):
            result = m.run(['/usr/bin/python3', '-c', 'print("transaction fixture")'], interactive=True)
            self.assertEqual(result, '')
            self.assertIn('transaction fixture', (Path(folder) / 'output.log').read_text())

    def test_history_does_not_recursively_record_its_own_output(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'state_dir', return_value=Path(folder)):
            self.assertEqual(m.execute({'action': 'history'}), 'No recorded management operations.')
            self.assertEqual(list(Path(folder).glob('*.json')), [])

    def test_restic_restore_uses_new_folder_and_clears_password_environment(self):
        with tempfile.TemporaryDirectory() as folder:
            repo = Path(folder) / 'repo'
            repo.mkdir()
            target = Path(folder) / 'restore'
            request = {'action': 'restic-restore', 'repository': str(repo), 'destination': str(target), 'snapshot': 'abcdef12'}
            with patch.object(m.os, 'geteuid', return_value=1000), patch.dict(os.environ, {'RESTIC_PASSWORD': 'not-a-real-password', 'RESTIC_PASSWORD_COMMAND': 'must-never-run'}), patch.object(m, 'run', return_value='') as run:
                m.dispatch(request, interactive=True)
                self.assertNotIn('RESTIC_PASSWORD', os.environ)
                self.assertNotIn('RESTIC_PASSWORD_COMMAND', os.environ)
                run.assert_called_once_with(['restic', '--repo', str(repo), 'restore', 'abcdef12', '--target', str(target), '--verify'], interactive=True)
                with self.assertRaises(m.OperationError):
                    m.dispatch(request, interactive=True)
                self.assertTrue(target.is_dir())

    def test_cli_with_missing_tools_is_non_destructive(self):
        with tempfile.TemporaryDirectory() as folder:
            result = subprocess.run(['/usr/bin/python3', str(ROOT / 'scripts/system-management.py')],
                                    input=json.dumps({'action': 'hardware'}), text=True, capture_output=True,
                                    env={**os.environ, 'PATH': folder, 'XDG_STATE_HOME': folder})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('Unavailable', result.stdout)
            self.assertTrue(list(Path(folder).rglob('*.json')))


if __name__ == '__main__':
    unittest.main()
