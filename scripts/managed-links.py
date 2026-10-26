"""Detach explicitly approved config links without writing through to their manager."""
import json
import os
from pathlib import Path
import shutil
import stat
import sys
import uuid

mode, root, backup, encoded = sys.argv[1:5]
root, backup = Path(root), Path(backup)
links = json.loads(encoded)

def checked(name):
    if not name or name.startswith('/') or any(p in ('', '.', '..') for p in name.split('/')):
        raise ValueError('Invalid linked config path')
    return root/name

if mode == 'detach':
    sources = {}
    for name, target in links:
        path = checked(name)
        if not path.is_symlink() or os.readlink(path) != target:
            raise ValueError('Managed link changed after review: ' + name)
        source = path.resolve(strict=True)
        if not source.is_dir(): raise ValueError('Managed parent is not a directory: ' + name)
        sources[name] = source
    moved = []
    try:
        total = count = 0
        for name, target in links:
            path = checked(name)
            source = sources[name]
            stage = root/('.archeasy-detach-' + uuid.uuid4().hex)
            for directory, dirs, files in os.walk(source, followlinks=False):
                for filename in [*dirs, *files]:
                    item = Path(directory)/filename
                    meta = item.lstat()
                    if not (stat.S_ISREG(meta.st_mode) or stat.S_ISDIR(meta.st_mode) or stat.S_ISLNK(meta.st_mode)):
                        raise ValueError('Special file in managed config: ' + str(item))
                    count += 1
                    if stat.S_ISREG(meta.st_mode): total += meta.st_size
                    if total > 512*1024*1024 or count > 100000: raise ValueError('Managed directory exceeds 512 MiB / 100,000 entries')
            shutil.copytree(source, stage, symlinks=True)
            stage.chmod(0o700)
            # Unchanged nested links continue pointing at their original targets.
            for directory, dirs, files in os.walk(stage, followlinks=False):
                for filename in [*dirs, *files]:
                    item = Path(directory)/filename
                    if item.is_symlink() and not os.path.isabs(os.readlink(item)):
                        original = source/item.relative_to(stage)
                        resolved = original.resolve(strict=False)
                        item.unlink()
                        item.symlink_to(resolved)
            current = os.readlink(path)
            if current not in (target, str(source)):
                raise ValueError('Managed link changed while copying: ' + name)
            saved = backup/'managed-links'/name
            saved.parent.mkdir(parents=True, exist_ok=True)
            os.replace(path, saved)
            moved.append((path, saved))
            os.replace(stage, path)
    except BaseException:
        for path, saved in reversed(moved):
            if path.exists() or path.is_symlink():
                retired = backup/'managed-retained'/uuid.uuid4().hex
                retired.parent.mkdir(parents=True, exist_ok=True)
                os.replace(path, retired)
            os.replace(saved, path)
        raise
elif mode == 'restore':
    for name, target in reversed(links):
        path = checked(name)
        if path.is_symlink() and os.readlink(path) == target:
            continue
        if path.exists() or path.is_symlink():
            retired = backup/'managed-retained'/uuid.uuid4().hex
            retired.parent.mkdir(parents=True, exist_ok=True)
            os.replace(path, retired)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.symlink_to(target)
else:
    raise ValueError('Unknown managed-link operation')
