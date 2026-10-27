"""Portable snapshots: create a ZIP or verify/unpack into a new folder."""
import hashlib
import errno
import json
import os
import shutil
from pathlib import Path
import stat
import sys
import tempfile
import time
import zipfile

LIMIT = 20 * 1024**3
COUNT = 200_000


def safe(name):
    if not isinstance(name, str) or '\\' in name or any(p in ('', '.', '..') for p in name.split('/')):
        raise ValueError('Unsafe archive path: ' + str(name))
    return name


def create(req):
    home = Path(req['home']).resolve(strict=True)
    target = Path(req['destination'])
    target = target.parent.resolve(strict=True) / target.name
    paths = [Path(p) for p in req['paths']]
    for path in paths:
        relative = path.relative_to(home)
        if not relative.parts or '..' in relative.parts:
            raise ValueError('Select paths inside home, not the whole home directory.')
        for parent in [path, *path.parents]:
            if parent == home:
                break
            if parent.is_symlink():
                raise ValueError('Linked selection: ' + str(path))
        if not path.exists():
            raise ValueError('Missing selection: ' + str(path))
        if path == target or path in target.parents:
            raise ValueError('Save the ZIP outside selected folders.')
    entries, seen = [], set()
    total = skipped = 0
    with tempfile.TemporaryDirectory(prefix='.archeasy-snapshot-', dir=target.parent) as temporary:
        stage = Path(temporary)/'snapshot.zip'
        with zipfile.ZipFile(stage, 'w', zipfile.ZIP_DEFLATED) as archive:
            def add(path):
                nonlocal total, skipped
                name = safe('home/' + path.relative_to(home).as_posix())
                if name in seen:
                    return
                seen.add(name)
                if len(seen) > COUNT:
                    raise ValueError('Snapshot exceeds 200,000 entries')
                before = path.lstat()
                mode = stat.S_IMODE(before.st_mode) & 0o777
                if stat.S_ISLNK(before.st_mode) or not (stat.S_ISDIR(before.st_mode) or stat.S_ISREG(before.st_mode)):
                    skipped += 1
                elif stat.S_ISDIR(before.st_mode):
                    entries.append({'path': name, 'directory': True, 'mode': mode})
                    for child in sorted(path.iterdir()):
                        add(child)
                else:
                    total += before.st_size
                    if total > LIMIT:
                        raise ValueError('Snapshot exceeds 20 GiB')
                    digest, size = hashlib.sha256(), 0
                    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                    with os.fdopen(fd, 'rb') as source:
                        opened = os.fstat(source.fileno())
                        if not stat.S_ISREG(opened.st_mode) or (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
                            raise ValueError('File changed: ' + str(path))
                        with archive.open(name, 'w', force_zip64=True) as output:
                            while block := source.read(1024*1024):
                                size += len(block)
                                if size > before.st_size:
                                    raise ValueError('File grew: ' + str(path))
                                digest.update(block)
                                output.write(block)
                        after = os.fstat(source.fileno())
                    if size != before.st_size or (before.st_mtime_ns, before.st_ctime_ns) != (after.st_mtime_ns, after.st_ctime_ns):
                        raise ValueError('File changed during backup: ' + str(path))
                    entries.append({'path': name, 'size': size, 'mode': mode, 'sha256': digest.hexdigest()})
            for path in paths:
                add(path)
            if req.get('script'):
                archive.writestr('my-system.sh', req['script'])
            archive.writestr('restore-snapshot.py', req['helper'])
            archive.writestr('README.txt', 'ArchEasy snapshot\n\nExtract restore-snapshot.py and run:\npython3 restore-snapshot.py restore snapshot.zip NEW_FOLDER\n\nFiles are verified and unpacked into NEW_FOLDER. Review home/ then copy desired files into your home. Existing live files are never changed by unpacking.\nOn Arch Linux, review and run: bash NEW_FOLDER/my-system.sh (if included).\nThis reinstalls current repository packages and restores bundled managed configs with rollback backups. Vendor/unavailable packages need their original sources.\nCustom apps need their source/binaries: include Projects, .local/bin, .local/share/applications and relevant app data when taking snapshots.\nLinks/special files are skipped. This ZIP may contain passwords/tokens; keep it private.\n')
            archive.writestr('manifest.json', json.dumps({'format': 'archeasy-snapshot-v1', 'created': time.strftime('%Y-%m-%dT%H:%M:%S%z'), 'selections': [str(p.relative_to(home)) for p in paths], 'entries': entries, 'bytes': total, 'skipped': skipped, 'apps': bool(req.get('script'))}, indent=2))
        stage.chmod(0o600)
        try:
            os.link(stage, target)
        except OSError as error:
            if error.errno not in (errno.EPERM, errno.EOPNOTSUPP, errno.ENOSYS, errno.EXDEV):
                raise
            # Some destination filesystems cannot hard-link. O_EXCL keeps the
            # same no-overwrite guarantee, including existing symbolic links.
            fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            try:
                with os.fdopen(fd, 'wb') as output, stage.open('rb') as source:
                    shutil.copyfileobj(source, output)
                    output.flush()
                    os.fsync(output.fileno())
            except BaseException:
                target.unlink()
                raise
    return f'Saved {target}: {len(entries)} entries, {total:,} bytes; skipped {skipped} links/special files.'


def restore(source, destination):
    target = Path(destination)
    if target.exists() or target.is_symlink():
        raise ValueError('Destination must be a new folder; existing folders are never overwritten.')
    with zipfile.ZipFile(source) as archive:
        infos = archive.infolist()
        if len(infos) > COUNT+4 or sum(i.file_size for i in infos) > LIMIT + 64*1024**2:
            raise ValueError('Archive exceeds snapshot limits')
        names = [safe(i.filename) for i in infos]
        if len(names) != len(set(names)):
            raise ValueError('Duplicate archive paths')
        if archive.getinfo('manifest.json').file_size > 64*1024**2:
            raise ValueError('Manifest exceeds limits')
        manifest = json.loads(archive.read('manifest.json'))
        if manifest.get('format') != 'archeasy-snapshot-v1':
            raise ValueError('Unsupported snapshot format')
        records = manifest['entries']
        if len(records) > COUNT:
            raise ValueError('Too many entries')
        expected, directories, seen = {}, {}, set()
        for entry in records:
            name = safe(entry['path'])
            if not name.startswith('home/') or name in seen:
                raise ValueError('Invalid manifest entry')
            seen.add(name)
            if entry.get('directory'):
                directories[name] = entry
            else:
                expected[name] = entry
        extras = {'manifest.json', 'README.txt', 'restore-snapshot.py', 'my-system.sh'}
        if set(names) - set(expected) - extras or set(expected) - set(names):
            raise ValueError('Archive and manifest disagree')
        target.mkdir(mode=0o700)
        try:
            for name in directories:
                (target/name).mkdir(parents=True, exist_ok=True, mode=0o700)
            for info in infos:
                if stat.S_ISLNK(info.external_attr >> 16) or info.is_dir():
                    raise ValueError('Unsupported archive entry')
                path = target/info.filename
                path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                digest = hashlib.sha256()
                with archive.open(info) as stream, path.open('xb') as output:
                    while block := stream.read(1024*1024):
                        output.write(block)
                        digest.update(block)
                path.chmod(0o600)
                if info.filename in expected:
                    entry = expected[info.filename]
                    if path.stat().st_size != entry['size'] or digest.hexdigest() != entry['sha256']:
                        raise ValueError('Checksum mismatch: ' + info.filename)
                    path.chmod(int(entry['mode']) & 0o777)
            for name, entry in sorted(directories.items(), key=lambda pair: len(pair[0]), reverse=True):
                (target/name).chmod(int(entry['mode']) & 0o777)
        except BaseException as error:
            raise ValueError(f'{error}; incomplete output retained at {target}') from error
    return f'Verified and unpacked to {target}. Review home/ and README.txt. Run my-system.sh on Arch to reinstall apps, if included.'


if __name__ == '__main__':
    os.umask(0o077)
    try:
        if len(sys.argv) == 4 and sys.argv[1] == 'restore':
            print(restore(sys.argv[2], sys.argv[3]))
        else:
            req = json.load(sys.stdin)
            print(restore(req['source'], req['destination']) if req.get('action') == 'restore' else create(req))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
