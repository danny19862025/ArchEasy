"""Exercise maintenance transactions with fake tools, never the host package manager."""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class MaintenanceTests(unittest.TestCase):
    def test_orphans_are_rechecked_and_only_selected_names_are_passed(self):
        for selected, expected in [(['old-lib'], 0), (['now-explicit'], 1), (['old-lib', 'now-explicit'], 1), (['--root=/'], 1), ([], 1)]:
            with self.subTest(selected=selected), tempfile.TemporaryDirectory() as directory:
                path = Path(directory)
                (path/'pacman').write_text('#!/bin/bash\n[[ $* == -Qdtq ]] || exit 99\nprintf "old-lib\\nother-lib\\n"\n')
                (path/'sudo').write_text('#!/bin/bash\nprintf "%s\\n" "$@" > "$TRACE"\n')
                for name in ['pacman', 'sudo']:
                    (path/name).chmod(0o700)
                trace = path/'trace'
                result = subprocess.run(['/usr/bin/bash', str(ROOT/'scripts/remove-orphans.sh'), *selected], env={**os.environ, 'PATH':directory, 'TRACE':str(trace)}, capture_output=True)
                self.assertEqual(result.returncode, expected, result.stderr)
                if expected == 0:
                    self.assertEqual(trace.read_text().splitlines(), ['pacman', '-Rs', '--', 'old-lib'])
                else:
                    self.assertFalse(trace.exists())

    def test_empty_scan_and_scan_failure_never_remove_packages(self):
        for result_code in [1, 2]:
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory)
                (path/'pacman').write_text(f'#!/bin/bash\nexit {result_code}\n')
                (path/'sudo').write_text('#!/bin/bash\ntouch "$TRACE"\n')
                for name in ['pacman', 'sudo']:
                    (path/name).chmod(0o700)
                trace = path/'trace'
                result = subprocess.run(['/usr/bin/bash', str(ROOT/'scripts/remove-orphans.sh'), 'old-lib'], env={**os.environ, 'PATH':directory, 'TRACE':str(trace)}, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(trace.exists())

    def test_cache_cleanup_requires_confirmation_and_keeps_three_versions(self):
        for answer, du_exit in [('n\n', 0), ('y\n', 0), ('y\n', 1)]:
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory)
                (path/'du').write_text(f'#!/bin/bash\nprintf "2G /var/cache/pacman/pkg\\n"\nexit {du_exit}\n')
                (path/'paccache').write_text('#!/bin/bash\nexit 0\n')
                (path/'sudo').write_text('#!/bin/bash\nprintf "%s\\n" "$@" > "$TRACE"\n')
                for name in ['du', 'paccache', 'sudo']:
                    (path/name).chmod(0o700)
                trace = path/'trace'
                result = subprocess.run(['/usr/bin/bash', str(ROOT/'scripts/clean-cache.sh')], input=answer, text=True, env={**os.environ, 'PATH':directory, 'TRACE':str(trace)}, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                if answer == 'y\n':
                    self.assertEqual(trace.read_text().splitlines(), ['paccache', '--remove', '--keep', '3'])
                else:
                    self.assertFalse(trace.exists())

if __name__ == '__main__':
    unittest.main()
