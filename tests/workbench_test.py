"""Mocked workbench tests. No privileged or destructive host operations."""
import base64
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module
core = load('management', ROOT / 'scripts/system-management.py')
w = load('workbench', ROOT / 'scripts/workbench.py')


class WorkbenchTests(unittest.TestCase):
    def test_memory_missing_malformed_and_zero(self):
        self.assertIsNone(w.memory('MemTotal: bad')['used_percent'])
        self.assertEqual(w.memory('MemTotal: 100 kB\nMemAvailable: 25 kB')['used_percent'], 75)
        self.assertIsNone(w.memory('MemTotal: 0 kB')['used_percent'])
        self.assertEqual(w.memory('SwapTotal: 10 kB\nSwapFree: 4 kB')['swap_used'], 6 * 1024)

    def test_cpu_ticks_excludes_guest_and_accepts_missing_iowait(self):
        self.assertEqual(w.cpu_ticks('cpu 1 2 3 4 5 6 7 8 99 99'), {'total': 36, 'idle': 9})
        self.assertIsNone(w.cpu_ticks('cpu bad values'))
        self.assertIsNone(w.cpu_ticks('cpu -1 0 0 0'))

    def test_network_counters(self):
        text = ' enp0s1: 100 1 0 0 0 0 0 0 200 1 0 0 0 0 0 0\nlo: 1 1 0 0 0 0 0 0 1 0 0 0 0 0 0 0\nbad: blah'
        self.assertEqual(w.net_counters(text), {'enp0s1': {'rx': 100, 'tx': 200}})

    def test_missing_and_present_sensor(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.assertEqual(w.sensor_values(root), [])
            device = root / 'hwmon0'; device.mkdir()
            (device / 'name').write_text('fixture')
            (device / 'temp1_input').write_text('41500')
            (device / 'temp2_input').write_text('bad')
            self.assertEqual(w.sensor_values(root)[0]['celsius'], 41.5)

    def test_desktop_detection_multiple_installed_is_not_active(self):
        with tempfile.TemporaryDirectory() as folder:
            sessions = Path(folder) / 'xsessions'; sessions.mkdir()
            (sessions / 'gnome.desktop').touch(); (sessions / 'plasma.desktop').touch()
            result = w.desktop_detect({'XDG_CURRENT_DESKTOP': 'KDE'}, sessions)
            self.assertEqual(result['detected'], ['kde'])
            self.assertEqual(len(result['installed_sessions']), 2)
            self.assertEqual(w.desktop_detect({}, sessions)['active'], 'Unavailable')

    def test_bootloader_evidence_does_not_infer_active_from_files(self):
        report = w.bootloader_evidence(True, True, True, 'unknown runtime')
        self.assertIn('GRUB files', report)
        self.assertIn('systemd-boot', report)
        self.assertIn('does not prove', report)
        self.assertIn('unknown', w.bootloader_evidence(True, False, False))
        self.assertIn('legacy', w.bootloader_evidence(False, False, False))

    def test_disk_hierarchy_and_corrupt_output(self):
        rows = w.disk_rows(json.dumps({'blockdevices': [{'name': 'nvme0n1', 'type': 'disk', 'children': [{'name': 'cryptroot', 'type': 'crypt'}]}]}))
        self.assertEqual(rows[1]['depth'], 1)
        self.assertEqual(rows[1]['type'], 'crypt')
        for value in ['not json', '{}', '{"blockdevices":[{}]}']:
            with self.assertRaises((ValueError, w.Error)):
                w.disk_rows(value)

    def test_firewall_conflict_and_unknown(self):
        self.assertIn('Both', w.firewall_state(True, True))
        self.assertIn('unknown', w.firewall_state(False, False))
        self.assertIn('firewalld', w.firewall_state(True, False))

    def test_battery_detection_and_units_are_not_mixed(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); battery = root / 'BAT0'; battery.mkdir()
            self.assertEqual(w.laptop_detect(root), [])
            (battery / 'type').write_text('Battery')
            (battery / 'energy_full').write_text('80')
            (battery / 'charge_full_design').write_text('100')
            self.assertNotIn('health_percent', w.battery_info(battery))
            (battery / 'energy_full_design').write_text('100')
            self.assertEqual(w.battery_info(battery)['health_percent'], 80)
            self.assertEqual(w.laptop_detect(root), [battery])
            self.assertEqual(w.battery_info(battery)['cycle_count'], 'Unavailable')

    def test_creator_selection_no_arbitrary_packages_or_duplicates(self):
        self.assertEqual(w.creator_selection({'packages': ['gimp', 'gimp']}), ['gimp'])
        for selected in [[], ['--overwrite=*'], ['not-in-catalog'], 'gimp']:
            with self.assertRaises(w.Error):
                w.creator_selection({'packages': selected})

    def test_creator_metadata_failure_stops_before_install(self):
        with patch.object(w, 'run', side_effect=w.Error('network/metadata unavailable')):
            with self.assertRaises(w.Error):
                w.creator_plan({'packages': ['gimp']})

    def test_mutations_need_terminal_and_non_root(self):
        for action in ['creator-install', 'appearance', 'migration-export', 'power-profile', 'brightness']:
            with self.assertRaises(w.Error):
                w.dispatch({'action': action})
        with patch.object(w.os, 'geteuid', return_value=0), self.assertRaises(w.Error):
            w.dispatch({'action': 'brightness', 'percent': '50'}, interactive=True)

    def test_conflicting_power_managers_and_invalid_brightness(self):
        with patch.object(w.os, 'geteuid', return_value=1000), patch.object(w, 'run', return_value='active') as runner:
            with self.assertRaisesRegex(w.Error, 'Conflicting'):
                w.dispatch({'action': 'power-profile', 'profile': 'balanced'}, True)
            self.assertFalse(any('set' in call.args[0] for call in runner.call_args_list))
            for value in ['0', '101', '50; reboot', 40]:
                with self.assertRaises(w.Error):
                    w.dispatch({'action': 'brightness', 'percent': value}, True)

    def test_appearance_backup_conflict_and_revert(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(core, 'state_dir', return_value=Path(folder)), patch.object(w, 'gnome_key', return_value=['gsettings', 'schema', 'color-scheme']):
            with patch.object(w, 'run', return_value="'default'") as runner:
                w.appearance_change('Modern Dark')
                runner.assert_any_call(['gsettings', 'set', 'schema', 'color-scheme', "'prefer-dark'"])
            with patch.object(w, 'run', return_value="'prefer-light'"), self.assertRaisesRegex(w.Error, 'outside ArchEasy'):
                w.appearance_change(restore=True)
            with patch.object(w, 'run', return_value="'prefer-dark'") as runner:
                w.appearance_change(restore=True)
                runner.assert_any_call(['gsettings', 'set', 'schema', 'color-scheme', "'default'"])

    def test_migration_secret_and_unsafe_paths(self):
        for name in ['../.bashrc', '/etc/fstab', '.config/../../etc/passwd', '.ssh/id_rsa', '.config/rclone/rclone.conf', '.config/x/key.pem', '.config/hypr/hyprland.conf']:
            with self.assertRaises(w.Error):
                w.safe_relative(name)
        for data in [b'password=hello', b'-----BEGIN OPENSSH PRIVATE KEY-----', b'url = https://user:password@example.com']:
            self.assertTrue(w.has_secret(data))
        self.assertFalse(w.has_secret(b'font_size = 14'))

    def manifest(self, path, name='.bashrc', data=b'# review before use\n'):
        import hashlib
        entry = {'path': name, 'content': base64.b64encode(data).decode(), 'sha256': hashlib.sha256(data).hexdigest()}
        path.write_text(json.dumps({'format': 'archeasy-migration-v1', 'source': {}, 'files': [entry], 'packages': {}}))
        return entry

    def test_manifest_validation_checksum_duplicates_and_traversal(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'manifest.json'
            self.manifest(path)
            self.assertEqual(len(w.load_manifest(path)['files']), 1)
            for name in ['../../escape', '/etc/passwd']:
                self.manifest(path, name)
                with self.assertRaises(w.Error): w.load_manifest(path)
            entry = self.manifest(path)
            manifest = json.loads(path.read_text()); manifest['files'].append(entry); path.write_text(json.dumps(manifest))
            with self.assertRaises(w.Error): w.load_manifest(path)
            self.manifest(path); manifest = json.loads(path.read_text()); manifest['files'][0]['sha256'] = 'bad'; path.write_text(json.dumps(manifest))
            with self.assertRaises(w.Error): w.load_manifest(path)

    def test_migration_staging_never_overwrites_live_config(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); home = root / 'home'; home.mkdir(); (home / '.bashrc').write_text('live')
            source = root / 'manifest.json'; self.manifest(source)
            target = root / 'stage'
            request = {'source': str(source), 'destination': str(target), 'paths': ['.bashrc']}
            with patch.object(Path, 'home', return_value=home):
                self.assertIn('conflict', w.migration_preview(request))
                w.migration_stage(request)
                self.assertEqual((home / '.bashrc').read_text(), 'live')
                self.assertEqual((target / '.bashrc').read_text(), '# review before use\n')
                with self.assertRaises(w.Error): w.migration_stage(request)
                (home / '.config').symlink_to(root, target_is_directory=True)
                with self.assertRaises(w.Error): w.home_path('.config/file')

    def test_diagnostic_dispatch_degrades_missing_tools_without_privilege(self):
        with patch.object(w, 'optional', return_value='Unavailable: missing utility'):
            for action in ['network', 'security', 'laptop', 'connectivity', 'processes', 'services']:
                self.assertIn('Unavailable', w.dispatch({'action': action}))

    def test_package_lock_coordinates_other_tools(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, {'XDG_RUNTIME_DIR': folder}):
            with w.package_lock():
                with self.assertRaises(w.Error):
                    with w.package_lock(): pass
            with w.package_lock(): pass

    def test_creator_atomic_transaction_and_cancel_before_start(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, {'XDG_RUNTIME_DIR': folder}), patch.object(w.os, 'geteuid', return_value=1000), patch.object(core, 'preflight') as preflight, patch.object(w, 'creator_plan', return_value=[{'package': 'gimp'}]), patch.object(w, 'run', return_value='') as runner:
            with patch('builtins.input', return_value='INSTALL'):
                w.dispatch({'action': 'creator-install', 'packages': ['gimp']}, True)
            self.assertEqual(preflight.call_count, 2)
            runner.assert_called_once_with(['sudo', 'pacman', '-Syu', '--needed', '--', 'gimp'], interactive=True)
            runner.reset_mock()
            with patch('builtins.input', return_value='n'), self.assertRaises(w.Error):
                w.dispatch({'action': 'creator-install', 'packages': ['gimp']}, True)
            runner.assert_not_called()

    def test_low_disk_space_stops_creator_before_metadata_or_transaction(self):
        with patch.object(core, 'preflight', side_effect=w.Error('Low disk space')), patch.object(w, 'run') as runner:
            with self.assertRaises(w.Error): w.creator_install({'packages': ['gimp']})
            runner.assert_not_called()

    def test_diagnostic_cancellation_flag_stops_tools(self):
        with tempfile.TemporaryDirectory() as folder:
            flag = Path(folder) / 'cancel'; flag.touch()
            with patch.object(w, 'CANCEL_PATH', flag), self.assertRaisesRegex(w.Error, 'Cancelled'):
                w.run(['/nonexistent/must-not-start'])

    def test_native_settings_rejects_ambiguous_desktops(self):
        with patch.object(w, 'desktop_detect', return_value={'detected': ['gnome', 'kde']}), patch.object(w.subprocess, 'Popen') as popen:
            with self.assertRaises(w.Error): w.native_settings()
            popen.assert_not_called()

    def test_ssh_diagnostics_never_reads_private_keys(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(Path, 'home', return_value=Path(folder)), patch.object(w, 'optional', return_value='Unavailable: no tools') as command:
            ssh = Path(folder) / '.ssh'; ssh.mkdir(); (ssh / 'id_ed25519').write_text('DO NOT READ PRIVATE KEY')
            report = w.security_report()
            self.assertNotIn('DO NOT READ', report)
            self.assertFalse(any('id_ed25519' in str(call) for call in command.call_args_list))

    def test_migration_export_roundtrip_and_secret_refusal(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(w, 'compatibility', return_value={}), patch.object(w, 'optional', return_value='Unavailable: missing package tool'):
            home = Path(folder) / 'home'; home.mkdir()
            config = home / '.bashrc'; config.write_text('# harmless configuration')
            target = Path(folder) / 'migration.json'
            with patch.object(Path, 'home', return_value=home):
                w.manifest_export({'paths': ['.bashrc'], 'destination': str(target)})
                self.assertEqual(w.load_manifest(target)['packages']['official'], [])
                self.assertEqual(target.stat().st_mode & 0o777, 0o600)
                config.write_text('TOKEN=secret')
                with self.assertRaises(w.Error): w.manifest_export({'paths': ['.bashrc'], 'destination': str(Path(folder) / 'secret.json')})
                self.assertFalse((Path(folder) / 'secret.json').exists())

    def test_bad_smart_device_is_not_executed(self):
        with patch.object(w, 'optional') as runner:
            for value in ['--format', '/dev/sda;reboot', '/dev/mapper/root', '/etc/passwd']:
                with self.assertRaises(w.Error): w.smart(value)
            runner.assert_not_called()

    def test_software_broker_preserves_explicit_removal_mode_and_full_upgrade(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, {'XDG_RUNTIME_DIR': folder}), patch.object(w.os, 'geteuid', return_value=1000), patch.object(core, 'preflight'), patch.object(w, 'run', return_value='') as runner:
            with patch('builtins.input', return_value='INSTALL'):
                w.dispatch({'action': 'software-install', 'package': 'gimp'}, True)
            runner.assert_any_call(['sudo', 'pacman', '-Syu', '--needed', '--', 'gimp'], interactive=True)
            runner.reset_mock()
            with patch('builtins.input', return_value='REMOVE'):
                w.dispatch({'action': 'software-remove', 'package': 'gimp', 'clean': False}, True)
            runner.assert_any_call(['sudo', 'pacman', '-R', '--', 'gimp'], interactive=True)
            runner.reset_mock()
            with patch('builtins.input', return_value='REMOVE'):
                w.dispatch({'action': 'software-remove', 'package': 'gimp', 'clean': True}, True)
            runner.assert_any_call(['sudo', 'pacman', '-Rns', '--', 'gimp'], interactive=True)

    def test_flatpak_scope_and_remote_validation(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, {'XDG_RUNTIME_DIR': folder}), patch.object(w.os, 'geteuid', return_value=1000), patch.object(w, 'run', return_value='') as runner:
            w.dispatch({'action': 'software-flatpak-remove', 'package': 'org.example.App', 'scope': '--user'}, True)
            runner.assert_called_once_with(['flatpak', 'uninstall', '--user', '--', 'org.example.App'], interactive=True)
            for scope in ['--installation=system', '--delete-data']:
                with self.assertRaises(w.Error): w.dispatch({'action': 'software-flatpak-remove', 'package': 'org.example.App', 'scope': scope}, True)
            with self.assertRaises(w.Error): w.dispatch({'action': 'software-flatpak-install', 'package': '--delete-data', 'remote': 'flathub'}, True)

    def test_existing_management_fallback_is_preserved(self):
        self.assertIn('offline', w.dispatch({'action': 'recovery'}))


if __name__ == '__main__':
    unittest.main()
