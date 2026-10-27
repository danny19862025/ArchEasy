"""Exercise clone/build sequencing with fake tools; no package installation."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class GitHubBuildTests(unittest.TestCase):
    def run_build(self, mode='file', answer='y\n', repo='fixture-owner/dotfiles'):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root / 'tools'
            tools.mkdir()
            for tool in ('mktemp', 'cat'):
                (tools / tool).symlink_to('/usr/bin/' + tool)
            scripts = {
                'git': '''#!/bin/bash
printf '%s\\n' "$*" > "$TRACE"
[[ $MODE != clone-fails ]] || exit 7
dest=${@: -1}
printf '%s' "$dest" > "$DESTINATION"
/usr/bin/mkdir -- "$dest"
case $MODE in
 file) printf '# reviewed fixture\\n' > "$dest/PKGBUILD" ;;
 symlink) /usr/bin/ln -s /dev/null "$dest/PKGBUILD" ;;
esac
''',
                'makepkg': '''#!/bin/bash
printf 'makepkg %s uid=%s cwd=%s\\n' "$*" "$EUID" "$PWD" >> "$TRACE"
exit "${BUILD_EXIT:-0}"
''',
            }
            for name, text in scripts.items():
                (tools / name).write_text(text)
                (tools / name).chmod(0o700)
            trace = root / 'trace'
            destination = root / 'destination'
            result = subprocess.run(
                ['/usr/bin/bash', str(ROOT / 'scripts/github-build.sh'), repo],
                input=answer, text=True, capture_output=True,
                env={**os.environ, 'PATH': str(tools), 'TRACE': str(trace),
                     'DESTINATION': str(destination), 'MODE': mode},
            )
            lines = trace.read_text().splitlines() if trace.exists() else []
            if destination.exists():
                workspace = Path(destination.read_text()).parent
            else:
                # clone-fails prints the fresh workspace before the fake git exits.
                workspace = Path(result.stdout.split(' into ', 1)[1].splitlines()[0]).parent if ' into ' in result.stdout else None
            if workspace is not None:
                self.assertTrue(str(workspace).startswith('/tmp/archeasy-github-fixture-owner-dotfiles.'))
                self.assertEqual(workspace.stat().st_mode & 0o777, 0o700)
                shutil.rmtree(workspace)
            return result, lines

    def test_clone_then_review_then_makepkg(self):
        result, lines = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('clone -- https://github.com/fixture-owner/dotfiles.git ', lines[0])
        self.assertIn('# reviewed fixture', result.stdout)
        self.assertEqual(len(lines), 2)
        self.assertIn('makepkg -si uid=' + str(os.getuid()), lines[1])
        self.assertTrue(lines[1].endswith('/dotfiles'))

    def test_decline_skips_makepkg(self):
        result, lines = self.run_build(answer='n\n')
        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(lines), 1)

    def test_missing_linked_or_failed_clone_never_builds(self):
        for mode in ('missing', 'symlink', 'clone-fails'):
            with self.subTest(mode=mode):
                result, lines = self.run_build(mode=mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(lines), 1)

    def test_invalid_repo_never_clones(self):
        for repo in ('../dotfiles', 'owner/..', 'owner/repo;id', 'owner/repo/extra'):
            with self.subTest(repo=repo):
                result, lines = self.run_build(repo=repo)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(lines, [])


if __name__ == '__main__':
    unittest.main()
