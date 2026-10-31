"""User-local GitHub release installer. Downloaded code is never executed here."""
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import struct
import sys
import tarfile
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import zipfile

MAX_DOWNLOAD = 2 * 1024**3
MAX_EXPANDED = 4 * 1024**3
MAX_FILES = 20000
HOSTS = {'github.com', 'api.github.com', 'objects.githubusercontent.com',
         'release-assets.githubusercontent.com', 'github-releases.githubusercontent.com'}
PART = re.compile(r'^[A-Za-z0-9_.][A-Za-z0-9_.-]{0,99}$')


def emit(**event):
    print(json.dumps(event), flush=True)


def repo_url(value):
    if not isinstance(value, str):
        raise ValueError('Paste an HTTPS GitHub repository URL.')
    value = value.strip()
    if any(ord(c) < 33 or ord(c) == 127 for c in value):
        raise ValueError('Repository URLs cannot contain whitespace or control characters.')
    url = urllib.parse.urlsplit(value)
    parts = url.path.rstrip('/').split('/')
    if (url.scheme != 'https' or url.netloc.lower() != 'github.com' or url.query or url.fragment
            or len(parts) != 3 or parts[0] or any(not PART.fullmatch(p) or p in ('.', '..') for p in parts[1:])):
        raise ValueError('Use https://github.com/owner/repository (no credentials, query or extra path).')
    owner, name = parts[1:]
    name = name.removesuffix('.git')
    if not PART.fullmatch(name) or name in ('.', '..'):
        raise ValueError('Invalid GitHub repository name.')
    return owner + '/' + name


def architecture(machine=None):
    machine = (machine or platform.machine()).lower()
    if machine in ('x86_64', 'amd64'):
        return 'x86_64'
    if machine in ('aarch64', 'arm64'):
        return 'aarch64'
    raise ValueError('Automatic release installation currently supports x86_64 and aarch64 only.')


def secure_url(url):
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != 'https' or parsed.hostname not in HOSTS or parsed.username or parsed.password or parsed.port not in (None, 443):
        raise ValueError('Refusing a download outside the supported HTTPS GitHub release hosts.')
    return url


class Redirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        secure_url(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def open_url(url):
    secure_url(url)
    request = urllib.request.Request(url, headers={'User-Agent': 'ArchEasy', 'Accept': 'application/vnd.github+json' if url.startswith('https://api.github.com/') else 'application/octet-stream'})
    try:
        return urllib.request.build_opener(Redirects()).open(request, timeout=30)
    except urllib.error.HTTPError as error:
        error.close()
        if error.code == 404:
            raise RuntimeError('Repository or stable release not found. Private repositories are not supported by this installer.') from error
        if error.code in (403, 429):
            raise RuntimeError('GitHub denied or rate-limited the request. Wait and try again.') from error
        raise RuntimeError('GitHub returned HTTP ' + str(error.code)) from error
    except (urllib.error.URLError, TimeoutError) as error:
        raise RuntimeError('GitHub connection failed. Check your network and retry: ' + str(error)) from error


def api(path):
    with open_url('https://api.github.com/' + path) as response:
        body = response.read(4 * 1024**2 + 1)
    if len(body) > 4 * 1024**2:
        raise RuntimeError('GitHub metadata response is too large.')
    return json.loads(body)


def asset_method(name, arch):
    name = name.lower()
    if re.search(r'(^|[._-])(windows|win32|win64|darwin|apple|macos|osx|freebsd|android)([._-]|$)', name):
        return None
    aliases = {'x86_64': ('x86_64', 'amd64', 'x64'), 'aarch64': ('aarch64', 'arm64')}
    found = {kind for kind, names in aliases.items() if any(re.search(r'(^|[._-])' + re.escape(alias) + r'([._-]|$)', name) for alias in names)}
    if found != {arch}:
        return None
    if name.endswith('.appimage'):
        return 'AppImage'
    if 'linux' not in re.split(r'[._-]', name):
        return None
    if name.endswith(('.tar.gz', '.tgz', '.tar.xz', '.tar.bz2', '.zip')):
        return 'Linux binary archive'
    if name.endswith('.bin') or '.' not in name.rsplit('-', 1)[-1]:
        return 'Linux binary'
    return None


def asset_record(asset, repo):
    name = asset.get('name', '')
    url = asset.get('browser_download_url', '')
    if not isinstance(name, str) or not name or '/' in name or '\\' in name or any(ord(c) < 32 for c in name):
        raise ValueError('Invalid release asset name.')
    secure_url(url)
    if not url.startswith('https://github.com/' + repo + '/releases/download/'):
        raise ValueError('Asset does not belong to this GitHub repository release.')
    size = asset.get('size')
    if size is not None and (not isinstance(size, int) or size < 0 or size > MAX_DOWNLOAD):
        raise ValueError('Release asset exceeds the 2 GiB download limit or has an invalid size.')
    return dict(id=asset['id'], name=name, url=url, size=size, digest=asset.get('digest'))


def checksum_asset(name, target):
    lower = name.lower()
    if lower in {target.lower() + suffix for suffix in ('.sha256', '.sha256sum', '.sha256.txt', '.sha512', '.sha512sum', '.sha512.txt')}:
        return True
    return bool(re.fullmatch(r'(?:.*[._-])?(?:checksums(?:\.txt|\.sha256|\.sha512)?|sha(?:256|512)sums?(?:\.txt)?)', lower))


def inspect(url, arch=None):
    repo = repo_url(url)
    arch = architecture(arch)
    metadata = api('repos/' + repo)
    canonical = metadata.get('full_name', repo)
    repo = repo_url('https://github.com/' + canonical)
    try:
        release = api('repos/' + repo + '/releases/latest')
    except RuntimeError as error:
        if str(error).startswith('Repository or stable release not found'):
            raise RuntimeError('No stable release is available. Automatic installation from source code is not yet supported.') from error
        raise
    if release.get('draft') or release.get('prerelease') or not release.get('tag_name'):
        raise RuntimeError('No stable published release is available.')
    assets = release.get('assets', [])
    candidates = []
    for asset in assets:
        method = asset_method(asset.get('name', ''), arch)
        if not method:
            continue
        item = asset_record(asset, repo)
        item.update(method=method, arch=arch, repo=repo, version=release['tag_name'], release_id=release['id'], title=metadata.get('name', repo.split('/')[1]))
        item['checksums'] = [asset_record(a, repo) for a in assets if checksum_asset(a.get('name', ''), item['name'])]
        candidates.append(item)
    return dict(repo=repo, title=metadata.get('name', repo.split('/')[1]), version=release['tag_name'], arch=arch, candidates=candidates,
                explanation='' if candidates else 'No supported Linux release download matches this architecture. Source-only repositories, installers, builds and assets without an explicit architecture are not automatically installed yet.')


def download(asset, path, limit=MAX_DOWNLOAD):
    total = 0
    last = -1
    emit(log='Downloading ' + asset['name'] + '\n')
    with open_url(asset['url']) as response, path.open('xb') as output:
        while True:
            chunk = response.read(1024 * 256)
            if not chunk:
                break
            total += len(chunk)
            if total > limit:
                raise RuntimeError('Download exceeds the allowed size.')
            output.write(chunk)
            size = asset.get('size')
            percent = int(total * 100 / size) if size else 0
            if percent != last:
                emit(progress=min(0.7, percent / 100 * 0.7), log=f'Downloaded {total} bytes' + (f' / {size}' if size else '') + '\n')
                last = percent
    if asset.get('size') is not None and total != asset['size']:
        raise RuntimeError('Download size differs from the published size. Retry inspection.')


def checksum_text(text, target):
    values = []
    for line in text.splitlines():
        line = line.strip()
        match = re.fullmatch(r'([0-9a-fA-F]{64}|[0-9a-fA-F]{128})\s+[*]?(.+)', line)
        bsd = re.fullmatch(r'SHA(256|512)\s*\((.+)\)\s*=\s*([0-9a-fA-F]+)', line, re.I)
        if match and match[2].removeprefix('./') == target:
            values.append(match[1].lower())
        elif bsd and bsd[2] == target and len(bsd[3]) == int(bsd[1]) // 4:
            values.append(bsd[3].lower())
        elif re.fullmatch(r'[0-9a-fA-F]{64}|[0-9a-fA-F]{128}', line):
            values.append(line.lower())
    return values


def verify(asset, path, workspace):
    expected = []
    digest = asset.get('digest')
    if digest:
        match = re.fullmatch(r'sha(256|512):([a-fA-F0-9]+)', digest)
        if not match or len(match[2]) != int(match[1]) // 4:
            raise RuntimeError('Unsupported or malformed published asset digest.')
        expected.append(match[2].lower())
    for index, checksum in enumerate(asset.get('checksums', [])):
        target = workspace / ('checksum-' + str(index))
        download(checksum, target, 4 * 1024**2)
        text = target.read_text(encoding='utf-8')
        matches = checksum_text(text, asset['name'])
        # Bare digests are valid only in the asset-specific sidecar.
        if checksum['name'].lower() not in {asset['name'].lower() + suffix for suffix in ('.sha256', '.sha256sum', '.sha256.txt', '.sha512', '.sha512sum', '.sha512.txt')}:
            matches = checksum_text('\n'.join(line for line in text.splitlines() if ' ' in line or '\t' in line), asset['name'])
        if not matches:
            raise RuntimeError('Published checksum file has no supported checksum for this download.')
        expected.extend(matches)
    hashes = {64: hashlib.sha256(), 128: hashlib.sha512()}
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            for value in hashes.values():
                value.update(chunk)
    if any(hashes[len(value)].hexdigest() != value for value in expected):
        raise RuntimeError('Checksum mismatch. The download was not installed.')
    emit(log='Published checksum verified.\n' if expected else 'No published checksum available; HTTPS download only.\n')
    return hashes[64].hexdigest()


def elf(path, arch):
    with path.open('rb') as stream:
        header = stream.read(20)
    return (len(header) == 20 and header[:4] == b'\x7fELF' and header[4:6] == b'\x02\x01'
            and struct.unpack('<H', header[16:18])[0] in (2, 3)
            and struct.unpack('<H', header[18:20])[0] == {'x86_64': 62, 'aarch64': 183}[arch])


def unpack(archive, payload):
    """Copy only regular files/directories; never trust archive extraction paths/modes."""
    size, count, seen = 0, 0, set()
    def destination(name, length):
        nonlocal size, count
        if not isinstance(length, int) or length < 0:
            raise RuntimeError('Invalid archive member size.')
        parts = PurePosixPath(name)
        if parts.is_absolute() or '..' in parts.parts or '\\' in name or any(ord(c) < 32 for c in name):
            raise RuntimeError('Unsafe path in release archive.')
        if not parts.parts:
            return None
        relative = str(parts)
        if relative in seen:
            raise RuntimeError('Duplicate path in release archive.')
        seen.add(relative)
        size += length
        count += 1
        if size > MAX_EXPANDED or count > MAX_FILES:
            raise RuntimeError('Release archive exceeds extraction limits.')
        return payload / relative
    def copy(source, target, length, executable):
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as output:
            remaining = length
            while remaining:
                chunk = source.read(min(remaining, 1024 * 1024))
                if not chunk:
                    raise RuntimeError('Truncated archive member.')
                output.write(chunk)
                remaining -= len(chunk)
        target.chmod(0o755 if executable else 0o644)
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as source:
            for item in source.infolist():
                mode = item.external_attr >> 16
                if mode & 0o170000 not in (0, 0o100000, 0o040000):
                    raise RuntimeError('Archive links and special files are not supported.')
                target = destination(item.filename, item.file_size)
                if target is None:
                    continue
                if item.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    with source.open(item) as stream:
                        copy(stream, target, item.file_size, bool(mode & 0o111))
    else:
        with tarfile.open(archive, 'r:*') as source:
            for item in source:
                if not (item.isfile() or item.isdir()):
                    raise RuntimeError('Archive links and special files are not supported.')
                target = destination(item.name, item.size)
                if target is None:
                    continue
                if item.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    with source.extractfile(item) as stream:
                        copy(stream, target, item.size, bool(item.mode & 0o111))


def entrypoint(payload, asset):
    if asset['method'] != 'Linux binary archive':
        result = payload / asset['name']
    else:
        binaries = [p for p in payload.rglob('*') if p.is_file() and '.so' not in p.name and elf(p, asset['arch']) and (p.stat().st_mode & 0o111 or p.suffix.lower() == '.appimage')]
        preferred = [p for p in binaries if p.name.lower() == asset['repo'].split('/')[1].lower()]
        if len(preferred) == 1:
            result = preferred[0]
        elif len(binaries) == 1:
            result = binaries[0]
        else:
            raise RuntimeError('Cannot identify one application executable in this archive. Install this release manually.')
    if not elf(result, asset['arch']):
        raise RuntimeError('Executable is not a supported Linux ELF binary for this computer. Nothing was launched.')
    result.chmod(0o755)
    return result.relative_to(payload)


def locations(home=None):
    home = Path(home) if home is not None else Path.home()
    if any(ord(c) < 32 for c in str(home)):
        raise RuntimeError('Unsupported control characters in home directory path.')
    return home / '.local/share/archeasy/apps', home / '.local/bin', home / '.local/share/applications'


def app_id(repo):
    repo = repo_url('https://github.com/' + repo)
    # A hash makes owner/repository separator ambiguities impossible.
    return 'github-' + hashlib.sha256(repo.lower().encode()).hexdigest()[:24]


def real_directory(path):
    if path.is_symlink():
        raise RuntimeError('Refusing a symlink directory: ' + str(path))
    if path != path.parent:
        real_directory(path.parent)
    path.mkdir(mode=0o700, exist_ok=True)


@contextlib.contextmanager
def lock(root):
    real_directory(root)
    fd = os.open(root / '.lock', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'a') as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('Another GitHub app install or removal is active.')
        yield


def installed(home=None):
    root, _, _ = locations(home)
    if not root.exists():
        return []
    if root.is_symlink():
        raise RuntimeError('Refusing a symlink app directory.')
    result = []
    for path in sorted(root.glob('github-*')):
        if path.is_symlink() or not path.is_dir():
            continue
        try:
            record = json.loads((path / '.archeasy.json').read_text())
            if app_id(record['repo']) == path.name and record['id'] == path.name:
                result.append(record)
        except (OSError, ValueError, KeyError, TypeError):
            emit(log='Ignored invalid app record: ' + path.name + '\n')
    return result


def desktop_text(title, executable, terminal):
    title = ''.join(c for c in title if ord(c) >= 32).replace('\\', '\\\\')
    command = str(executable).replace('\\', '\\\\\\\\').replace('"', '\\\\"').replace('`', '\\\\`').replace('$', '\\\\$').replace('%', '%%')
    return f'[Desktop Entry]\nType=Application\nName={title}\nExec="{command}"\nTerminal={str(terminal).lower()}\nCategories=Utility;\n'


def install(asset, home=None):
    if os.geteuid() == 0:
        raise RuntimeError('Run ArchEasy as your normal desktop user. Root installation is refused.')
    fresh = inspect('https://github.com/' + asset['repo'])
    if asset not in fresh['candidates']:
        raise RuntimeError('The release changed since inspection. Inspect again before installing.')
    root, bin_dir, desktop_dir = locations(home)
    identity = app_id(asset['repo'])
    destination = root / identity
    launcher = bin_dir / ('archeasy-' + asset['repo'].replace('/', '-').lower())
    desktop = desktop_dir / (identity + '.desktop')
    with lock(root):
        real_directory(bin_dir)
        real_directory(desktop_dir)
        if destination.exists() or destination.is_symlink():
            raise RuntimeError('This repository is already installed. Remove it before installing another version.')
        if os.path.lexists(launcher) or os.path.lexists(desktop):
            raise RuntimeError('A launcher or desktop entry already exists. No existing file was overwritten.')
        with tempfile.TemporaryDirectory(prefix='.install-', dir=root) as workspace:
            workspace = Path(workspace)
            archive = workspace / 'download'
            download(asset, archive)
            digest = verify(asset, archive, workspace)
            emit(progress=0.75, log='Preparing application files; no downloaded code will be executed.\n')
            staged = workspace / 'app'
            payload = staged / 'payload'
            payload.mkdir(parents=True)
            if asset['method'] == 'Linux binary archive':
                unpack(archive, payload)
            else:
                shutil.move(archive, payload / asset['name'])
            relative = entrypoint(payload, asset)
            executable = destination / 'payload' / relative
            graphical = asset['method'] == 'AppImage' or relative.suffix.lower() == '.appimage' or any(payload.rglob('*.desktop'))
            menu = desktop_text(asset['title'], executable, False) if graphical else None
            record = dict(id=identity, repo=asset['repo'], title=asset['title'], version=asset['version'], arch=asset['arch'], method=asset['method'], asset=asset['name'], sha256=digest,
                          executable=str(Path('payload') / relative), launcher=launcher.name, desktop_sha256=hashlib.sha256(menu.encode()).hexdigest() if menu else None)
            (staged / '.archeasy.json').write_text(json.dumps(record, indent=2))
            published = linked = menu_written = False
            try:
                staged.rename(destination)
                published = True
                launcher.symlink_to(executable)
                linked = True
                if menu:
                    with desktop.open('x') as handle:
                        menu_written = True
                        handle.write(menu)
                emit(log='Installed ' + asset['title'] + ' ' + asset['version'] + '\nLauncher: ' + str(launcher) + '\nNo application or installer was executed. AppImages may require FUSE.\n', progress=1.0)
            except Exception:
                if menu_written:
                    desktop.unlink()
                if linked:
                    launcher.unlink()
                if published:
                    shutil.rmtree(destination)
                raise
    return record


def remove(identity, home=None):
    if os.geteuid() == 0:
        raise RuntimeError('Remove user apps as your normal desktop user.')
    if not re.fullmatch(r'github-[0-9a-f]{24}', identity):
        raise ValueError('Invalid tracked application ID.')
    root, bin_dir, desktop_dir = locations(home)
    with lock(root):
        records = {item['id']: item for item in installed(home)}
        if identity not in records:
            raise RuntimeError('Application is not tracked by ArchEasy.')
        record = records[identity]
        destination = root / identity
        # Derive external paths from the repository, never trust arbitrary registry paths.
        launcher = bin_dir / ('archeasy-' + record['repo'].replace('/', '-').lower())
        desktop = desktop_dir / (identity + '.desktop')
        executable = PurePosixPath(record['executable'])
        if executable.is_absolute() or '..' in executable.parts or not executable.parts or executable.parts[0] != 'payload':
            raise RuntimeError('Invalid tracked executable path.')
        real_directory(bin_dir)
        real_directory(desktop_dir)
        expected = destination / str(executable)
        if os.path.lexists(launcher) and (not launcher.is_symlink() or os.readlink(launcher) != str(expected)):
            raise RuntimeError('Launcher has been replaced. Resolve it manually before removal.')
        if os.path.lexists(desktop) and (desktop.is_symlink() or not record.get('desktop_sha256') or hashlib.sha256(desktop.read_bytes()).hexdigest() != record['desktop_sha256']):
            raise RuntimeError('Desktop entry has been changed. Resolve it manually before removal.')
        if launcher.is_symlink():
            launcher.unlink()
        if desktop.exists():
            desktop.unlink()
        shutil.rmtree(destination)
        emit(log='Removed tracked app files, launcher and desktop entry. User settings were preserved.\n', progress=1.0)


def main(mode, request):
    if mode == 'inspect':
        emit(inspection=inspect(request['url']))
    elif mode == 'list':
        emit(installed_apps=installed())
    elif mode == 'install':
        install(request['asset'])
        emit(installed_apps=installed())
    elif mode == 'remove':
        remove(request['id'])
        emit(installed_apps=installed())
    else:
        raise ValueError('Unknown GitHub release operation.')


if __name__ == '__main__':
    try:
        main(sys.argv[1], json.loads(sys.argv[2]))
        emit(done=True)
    except Exception as error:
        emit(error=str(error))
        sys.exit(1)
