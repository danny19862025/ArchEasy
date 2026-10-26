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
-Qq) printf 'ghostty\\n';;
esac
''',
 'paru':'#!/bin/bash\nprintf "aur-app 1 -> 2\\n"\n',
 'flatpak':'#!/bin/bash\nexit 0\n',
 'curl':'#!/bin/bash\nprintf \'{"results": []}\'\n',
 }
 for name,text in commands.items():
  p=tools/name;p.write_text(text);p.chmod(0o755)
 env={**os.environ,'PATH':str(tools)+':'+os.environ['PATH'],'XDG_CONFIG_HOME':str(root/'config'),'XDG_DATA_HOME':str(root/'data'),'XDG_CACHE_HOME':str(root/'cache'),'ARCHEASY_UI_FIXTURE':'1','ARCHEASY_UI_SCREENSHOT':os.environ.get('ARCHEASY_UI_SCREENSHOT', '/tmp/archeasy-native-preview.png')}
 subprocess.run(['cargo','test','--offline','ux_tests::native_search_and_guided_flows','--','--ignored','--exact','--test-threads=1','--nocapture'],env=env,check=True,cwd=ROOT)
