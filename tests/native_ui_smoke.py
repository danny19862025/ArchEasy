"""Isolated desktop checks using fake package tools; requires a graphical session."""
from pathlib import Path
import os, tempfile, subprocess
ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='archeasy-ui-') as directory:
 root=Path(directory); tools=root/'tools'; tools.mkdir()
 commands={
 'pacman': '''#!/bin/bash
case "$1" in
-Ss) if [[ "$*" == *fire* ]]; then sleep 0.6; printf 'extra/firefox 1.0\\n    Browser\\n'; else printf 'extra/ghostty 1.0 [installed]\\n    Terminal\\n'; fi;;
-Si) printf 'Name : ghostty\\nDownload Size : 120 KiB\\n';;
-Quq) printf 'firefox\\nghostty\\n';;
-Qen|-Qem) if [[ ${ARCHEASY_BROKEN_REPO:-0} == 1 ]]; then printf 'error: fixture: missing required signature\\n' >&2; exit 1; fi
if [[ $1 == -Qen ]]; then printf 'ghostty 1.0\\nfish 1.0\\n'; else printf 'local-tool 1.0\\n'; fi;;
-Qe) printf 'ghostty 1.0\\nfish 1.0\\nlocal-tool 1.0\\n';;
-Qq) printf 'ghostty\\n';;
esac
''',
 'paru':'#!/bin/bash\nprintf "aur-app 1 -> 2\\n"\n',
 'du':'#!/bin/bash\nfor ((i=0;i<100;i++)); do printf "du: Permission denied\\n" >&2; done\nprintf "2.5G /var/cache/pacman/pkg\\n"\nexit 1\n',
 'flatpak':'#!/bin/bash\nexit 0\n',
 'curl':'#!/bin/bash\nif [[ "$*" == *releases/latest* ]]; then printf \'{"tag_name":"v9.0.0"}\'; else printf \'{"results": []}\'; fi\n',
 }
 for name,text in commands.items():
  p=tools/name;p.write_text(text);p.chmod(0o755)
 env={**os.environ,'PATH':str(tools)+':'+os.environ['PATH'],'XDG_CONFIG_HOME':str(root/'config'),'XDG_DATA_HOME':str(root/'data'),'XDG_CACHE_HOME':str(root/'cache'),'ARCHEASY_UI_FIXTURE':'1','ARCHEASY_UI_SCREENSHOT':os.environ.get('ARCHEASY_UI_SCREENSHOT', '/tmp/archeasy-native-preview.png')}
 for i, test in enumerate(['ux_tests::native_search_and_guided_flows', 'configs::workflow_tests::configs_workflow_ui', 'appcleaner::tests::embedded_cleaner_workflow', 'cloud::tests::cloud_tab_workflow', 'store::installed::tests::installed_rings_animate_settle_and_respect_reduced_motion', 'snapshots::tests::snapshot_dialog_selection_and_empty_backup_guard']):
  case_env={**env,'XDG_CONFIG_HOME':str(root/f'config-{i}'),'XDG_DATA_HOME':str(root/f'data-{i}'),'XDG_CACHE_HOME':str(root/f'cache-{i}')}
  home = root/f'home-{i}'
  runtime = root/f'runtime-{i}'
  home.mkdir()
  runtime.mkdir(mode=0o700)
  case_env.update(HOME=str(home), XDG_RUNTIME_DIR=str(runtime), GSETTINGS_BACKEND='memory', CARGO_HOME=os.environ.get('CARGO_HOME', str(Path.home()/'.cargo')), RUSTUP_HOME=os.environ.get('RUSTUP_HOME', str(Path.home()/'.rustup')))
  if test == 'appcleaner::tests::embedded_cleaner_workflow':
   isolated = home/'.cache/archeasy-cleaner-smoke-ci'
   isolated.mkdir(parents=True)
   case_env.update(ARCHEASY_CLEANER_TEST_ROOT=str(isolated), XDG_DATA_HOME=str(isolated/'data'))
  if os.environ.get('ARCHEASY_PREBUILT_TESTS') == '1':
   binaries = [p for p in (ROOT/'target/debug/deps').glob('archeasy-*') if p.is_file() and os.access(p, os.X_OK)]
   binary = max(binaries, key=lambda p: p.stat().st_mtime_ns)
   command = [str(binary), test, '--ignored', '--exact', '--test-threads=1', '--nocapture']
  else:
   command = ['cargo', 'test', '--offline', test, '--', '--ignored', '--exact', '--test-threads=1', '--nocapture']
  subprocess.run(command, env=case_env, check=True, cwd=ROOT)
