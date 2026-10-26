import base64
import json
import os
from pathlib import Path
import shutil
import stat
import time
import zipfile

payload = globals().get('PLAN_PAYLOAD_B64') or os.environ['ARCHEASY_CONFIG_PAYLOAD']
plan = json.loads(base64.b64decode(payload))
root = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home()/'.config')))
files = plan.get('files', [])
if len(files) > 100 or sum(len(f['bytes']) for f in files) > 8*1024*1024:
    raise ValueError('Config bundle exceeds limits')
for entry in files:
    name = entry['path']
    if not name or name.startswith('/') or any(p in ('', '.', '..') for p in name.split('/')):
        raise ValueError('Invalid config path')
    target = root / name
    for parent in [target, *list(target.parents)[:len(Path(name).parts)-1]]:
        if parent.is_symlink():
            raise ValueError('Linked config path needs manual review: ' + str(parent))
    if target.exists() and not target.is_file():
        raise ValueError('Config target is not a file: ' + str(target))
if not files:
    print('No applied configs were included.')
else:
    backup = Path(os.environ.get('XDG_DATA_HOME', str(Path.home()/'.local/share')))/'archeasy/system-restore'/str(time.time_ns())
    backup.mkdir(parents=True, mode=0o700)
    manifest = []
    for entry in files:
        target = root/entry['path']
        manifest.append({'path':entry['path'], 'existed':target.exists(), 'mode':stat.S_IMODE(target.stat().st_mode) if target.exists() else 0o600})
        if target.exists():
            saved = backup/'files'/entry['path']
            saved.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(target, saved)
            saved.chmod(0o600)
    (backup/'manifest.json').write_text(json.dumps({'root':str(root),'entries':manifest}))
    with zipfile.ZipFile(backup.with_suffix('.zip'), 'x', zipfile.ZIP_DEFLATED) as archive:
        for source in backup.rglob('*'):
            if source.is_file(): archive.write(source, str(source.relative_to(backup)))
    backup.with_suffix('.zip').chmod(0o600)
    try:
        for entry in files:
            target = root/entry['path']
            target.parent.mkdir(parents=True, exist_ok=True)
            data = bytes(entry['bytes'])
            if not entry['path'].endswith('.png'):
                data = data.decode('utf-8').replace('__CONFIG_DIR__', str(root)).encode('utf-8')
            temporary = target.with_name('.archeasy-restore-' + str(time.time_ns()))
            fd = os.open(temporary, os.O_WRONLY|os.O_CREAT|os.O_EXCL, 0o600)
            with os.fdopen(fd, 'wb') as stream: stream.write(data)
            os.replace(temporary, target)
    except BaseException:
        for entry in manifest:
            target = root/entry['path']
            if entry['existed']:
                shutil.copyfile(backup/'files'/entry['path'], target)
                target.chmod(entry['mode'])
            else:
                target.unlink(missing_ok=True)
        raise
    print('Configs restored. Original files and manifest saved at', backup)
