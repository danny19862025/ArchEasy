"""Portable ZIP transfer. Never extract archive paths onto the filesystem."""
import json
import os
from pathlib import Path
import stat
import sys
import zipfile

mode, source, root = sys.argv[1:4]
source, root = Path(source), Path(root)
LIMIT = 8 * 1024 * 1024

def normal(name):
    p = Path(name)
    if not name or p.is_absolute() or '..' in p.parts or '.' in name.split('/') or '' in name.split('/'):
        raise ValueError('Invalid config path')
    return p

if mode == 'read':
    with zipfile.ZipFile(source) as archive:
        info = archive.getinfo('archeasy-portable.json')
        if info.file_size > 40 * 1024 * 1024:
            raise ValueError('Portable manifest too large')
        plan = json.loads(archive.read(info))
        print(json.dumps(plan))
else:
    output = Path(sys.argv[4])
    # Reserve a private destination; never overwrite an existing archive.
    fd = os.open(output, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        with os.fdopen(fd, 'wb') as stream, zipfile.ZipFile(stream, 'w', zipfile.ZIP_DEFLATED) as archive:
            paths = set()
            count = size = 0
            for directory, folders, files in os.walk(source, followlinks=False):
                folders[:] = [n for n in folders if not (Path(directory) / n).is_symlink()]
                for filename in files:
                    path = Path(directory) / filename
                    meta = path.lstat()
                    if not stat.S_ISREG(meta.st_mode):
                        continue
                    count += 1
                    size += meta.st_size
                    if count > 100000 or size > 512 * 1024 * 1024:
                        raise ValueError('Backup history exceeds 512 MiB / 100,000 files')
                    archive.write(path, 'history/' + str(path.relative_to(source)))
                    if filename == 'manifest.json':
                        manifest = json.loads(path.read_text())
                        for entry in manifest.get('entries', []):
                            paths.add(str(normal(entry['path'])))
            if not count:
                raise ValueError('No backups yet. Apply a preset first.')
            files = []
            total = 0
            for name in sorted(paths):
                path = root / normal(name)
                # Linked config folders/files stay managed by their original manager.
                if any(p.is_symlink() for p in [path, *list(path.parents)[:len(Path(name).parts)-1]]):
                    continue
                if not path.is_file():
                    continue
                if path.stat().st_size > LIMIT:
                    raise ValueError('Config exceeds 8 MiB: ' + name)
                data = path.read_bytes()
                try:
                    text = data.decode('utf-8')
                except UnicodeDecodeError:
                    pass
                else:
                    if '\0' not in text:
                        data = text.replace(str(root), '__CONFIG_DIR__').encode('utf-8')
                total += len(data)
                if total > LIMIT or len(files) >= 100:
                    raise ValueError('Portable config exceeds 8 MiB / 100 files')
                files.append({'path': name, 'bytes': list(data)})
            # For a pre-apply ZIP, preserve only the before-state, also in history/.
            archive.writestr('archeasy-portable.json', json.dumps({'id': 'portable-restore', 'name': 'Portable config backup', 'dependencies': [], 'files': files}))
        print(str(output))
    except BaseException:
        output.unlink(missing_ok=True)
        raise
