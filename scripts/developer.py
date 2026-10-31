"""Developer setup policy and planning; shared interactive runner is provided by ArchEasy.

MANIFEST is injected from data/developer-tools.json by the Rust process adapter.
No shell commands are assembled by the GUI. All subprocess arguments are arrays.
"""
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import urllib.request

from archeasy_setup import Session, emit, installed, availability, supported

REGISTRY = 'https://registry.npmjs.org'
PREFIX = Path.home() / '.local/share/archeasy/developer/npm'
NAME = re.compile(r'^[a-z0-9][a-z0-9+_.-]*$')


def query(*args, timeout=45):
    try:
        p = subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True, text=True,
                           timeout=timeout, env={**os.environ, 'LC_ALL': 'C', 'RUSTUP_AUTO_INSTALL': '0', 'RUSTUP_NO_UPDATE_CHECK': '1'})
        return p.returncode, p.stdout, p.stderr
    except (OSError, subprocess.TimeoutExpired) as e:
        return 127, '', str(e)


def tools():
    return {item['id']: item for item in MANIFEST['tools']}


def sources(item):
    choices = ['official'] if item['packages'] else []
    choices += [key for key in ('flatpak', 'aur', 'upstream') if item.get(key)]
    return choices


def validate_manifest():
    items = tools()
    if len(items) != len(MANIFEST['tools']):
        raise ValueError('Duplicate developer tool ID')
    for item in items.values():
        if not NAME.fullmatch(item['id']) or not sources(item):
            raise ValueError('Invalid developer tool manifest')
        for name in item['packages'] + ([item['aur']] if item.get('aur') else []):
            if not NAME.fullmatch(name):
                raise ValueError('Invalid package name in manifest')
        if not set(item.get('requires', [])) <= items.keys():
            raise ValueError('Unknown manifest dependency')
    for values in MANIFEST['presets'].values():
        if not set(values) <= items.keys():
            raise ValueError('Unknown preset item')


def expand(selected):
    items, result, visiting = tools(), [], set()
    def visit(key):
        if key in visiting:
            raise ValueError('Cyclic developer dependency')
        if key in result:
            return
        if key not in items:
            raise ValueError('Unknown selected tool: ' + key)
        visiting.add(key)
        for dep in items[key].get('requires', []):
            visit(dep)
        visiting.remove(key)
        result.append(key)
    for key in selected:
        visit(key)
    return result


def is_installed(item, source, native, flat):
    if source == 'flatpak':
        return item.get('flatpak') in flat
    if source == 'aur':
        return item.get('aur') in native
    if source == 'upstream':
        return (PREFIX / 'node_modules/.bin/codex').is_file() and query(str(PREFIX / 'node_modules/.bin/codex'), '--version')[0] == 0
    present = bool(item['packages']) and all(p in native for p in item['packages'])
    if item['id'] == 'rust' and present:
        return query('rustup', 'run', 'stable', 'rustc', '--version')[0] == 0
    return present


def selected_source(item, choices):
    source = choices.get(item['id'], item.get('default_source', 'official'))
    if source not in sources(item):
        raise ValueError('Unsupported source for ' + item['title'])
    return source


def plan(request, native, flat, available):
    items = tools()
    requested = request.get('selected', [])
    if not isinstance(requested, list) or any(not isinstance(key, str) for key in requested):
        raise ValueError('Invalid tool selection')
    selected = expand(requested)
    if not isinstance(request.get('allow_aur', False), bool):
        raise ValueError('AUR permission must be an explicit boolean choice')
    choices = request.get('sources', {})
    if not isinstance(choices, dict) or not set(choices) <= items.keys():
        raise ValueError('Invalid developer source choices')
    if not selected:
        raise ValueError('Select at least one developer tool')
    # Explicit prerequisites are part of the review, never silently installed.
    selected_sources = {key: selected_source(items[key], choices) for key in selected}
    extra = []
    if 'distrobox' in selected and not native.intersection({'docker', 'podman'}) and not set(selected).intersection({'docker', 'podman'}):
        extra.append('podman')
    if any(s == 'aur' for s in selected_sources.values()):
        if not request.get('allow_aur'):
            raise ValueError('AUR is disabled. Enable reviewed AUR builds explicitly or choose another source.')
        extra += ['git', 'build']
    if 'upstream' in selected_sources.values():
        extra += ['node']
    selected = expand(extra + selected)
    for key in selected:
        selected_sources.setdefault(key, selected_source(items[key], choices))
    for key in selected:
        source = selected_sources[key]
        for other in items[key].get('conflicts', []):
            if source in ('official', 'aur') and other in selected and selected_sources[other] in ('official', 'aur'):
                raise ValueError(f'Conflicting native editors: {items[key]["title"]} and {items[other]["title"]}. Choose one or use a separate Flatpak build.')
        if key == 'rust' and 'rustup' not in native and native.intersection({'rust', 'rust-git', 'rust-nightly', 'rust-bin'}):
            raise ValueError('Distribution Rust conflicts with rustup. Existing Rust will not be replaced. Deselect Rust via rustup or resolve the toolchain choice manually.')
    targets, steps = [], []
    for key in selected:
        item, source = items[key], selected_sources[key]
        if is_installed(item, source, native, flat):
            continue
        if source == 'official':
            missing = [p for p in item['packages'] if p not in native]
            unavailable = [p for p in missing if p not in available]
            if unavailable:
                raise ValueError('Unavailable official packages for ' + item['title'] + ': ' + ', '.join(unavailable) + '. Select an offered alternative source explicitly; no substitution is automatic.')
            targets += [available[p] for p in missing]
        elif source == 'flatpak':
            if 'flatpak' not in native:
                if 'flatpak' not in available:
                    raise ValueError('Official Flatpak package is unavailable')
                targets.append(available['flatpak'])
            steps.append(dict(id=key, kind='flatpak', app=item['flatpak']))
        elif source == 'aur':
            steps.append(dict(id=key, kind='aur', package=item['aur']))
        else:
            steps.append(dict(id=key, kind='upstream'))
    targets = list(dict.fromkeys(targets))
    operations = []
    if targets:
        operations.append(['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-S', '--needed', '--'] + targets)
    for step in steps:
        if step['kind'] == 'flatpak':
            operations.append(['flatpak', 'install', '--user', 'flathub', step['app']])
        elif step['kind'] == 'aur':
            operations += [['git', '-c', 'core.hooksPath=/dev/null', '-c', 'init.templateDir=', 'clone', '--depth', '1', '--', 'https://aur.archlinux.org/' + step['package'] + '.git', '<private build directory>'], ['makepkg', '--nodeps', '--cleanbuild'], ['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-U', '--needed', '--', '<reviewed built package archives>']]
        else:
            operations.append(['npm', 'install', '--prefix', str(PREFIX), '--ignore-scripts', '--no-audit', '--no-fund', '--registry=' + REGISTRY, '--@openai:registry=' + REGISTRY, '@openai/codex@<verified version>'])
    if 'rust' in selected:
        operations.append(['rustup', 'toolchain', 'install', 'stable', '--profile', 'minimal'])
    return dict(selected=selected, sources=selected_sources, targets=targets, steps=steps, operations=operations)


def fields(text):
    result, current = {}, None
    for line in text.splitlines():
        if ':' in line and not line[:1].isspace():
            current, value = line.split(':', 1)
            current = current.strip()
            result[current] = value.strip()
        elif current and line[:1].isspace():
            result[current] += ' ' + line.strip()
    return result


def conflict_present(expression):
    if not re.fullmatch(r'[a-zA-Z0-9@._+<>=:-]+', expression):
        raise RuntimeError('Invalid conflict metadata: ' + expression)
    code, missing, error = query('pacman', '-T', '--', expression)
    if code == 0:
        return True
    if code == 127 and missing.strip() == expression and not error.strip():
        return False
    raise RuntimeError('Cannot verify installed conflict providers for ' + expression + ': ' + error)


def native_preflight(targets, native):
    """Reject installed-package conflicts and show Pacman's dependency resolution."""
    for target in targets:
        code, text, error = query('pacman', '-Si', target)
        if code:
            raise RuntimeError('Cannot validate package metadata: ' + target + ': ' + error)
        info = fields(text)
        if info.get('Name') != target.split('/')[-1]:
            raise RuntimeError('Unexpected package metadata for ' + target)
        for conflict in info.get('Conflicts With', '').split():
            if conflict == 'None':
                continue
            if conflict_present(conflict):
                raise RuntimeError(target + ' conflicts with installed provider ' + conflict + '. No replacement will be attempted.')
    if not targets:
        return ''
    code, text, error = query('pacman', '-Sp', '--print-format', '%r/%n %v', '--', *targets)
    if code:
        raise RuntimeError('Dependency resolution failed: ' + error + text)
    if not text.strip():
        raise RuntimeError('Pacman returned an empty dependency plan')
    return text


VERSIONS = [('Git', ['git', '--version']), ('GCC', ['gcc', '--version']), ('Clang', ['clang', '--version']), ('LLVM', ['llvm-config', '--version']), ('Python', ['python', '--version']), ('pip', ['python', '-m', 'pip', '--version']), ('uv', ['uv', '--version']), ('Rust', ['rustc', '--version']), ('Cargo', ['cargo', '--version']), ('Go', ['go', 'version']), ('Node.js', ['node', '--version']), ('npm', ['npm', '--version']), ('OpenJDK', ['java', '-version']), ('Ruby', ['ruby', '--version']), ('PHP', ['php', '--version']), ('Docker', ['docker', '--version']), ('Podman', ['podman', '--version']), ('Codex', ['codex', '--version']), ('Neovim', ['nvim', '--version']), ('Emacs', ['emacs', '--version']), ('VS Code executable (see build identity below)', ['code', '--version']), ('VSCodium', ['codium', '--version'])]


def report(native, flat, choices):
    lines = []
    def record(text):
        lines.append(text)
        emit(log=text)
    record('Development readiness report\n')
    for title, command in VERSIONS:
        if title == 'Codex' and choices.get('codex') == 'upstream':
            command = [str(PREFIX / 'node_modules/.bin/codex'), '--version']
        code, text, error = query(*command, timeout=8)
        result = (text or error).strip().splitlines()
        record(title + ': ' + (result[0] if code == 0 and result else 'not available / not configured') + '\n')
    for item in MANIFEST['tools']:
        source = selected_source(item, choices)
        status = 'Installed' if is_installed(item, source, native, flat) else 'Not Installed'
        emit(tool_status={'id': item['id'], 'state': status})
        record(f'{item["title"]} [{source}]: {status}\n')
    missing = [p for item in MANIFEST['tools'] if item.get('recommended') for p in item['packages'] if p not in native]
    record('Missing essential development dependencies: ' + (', '.join(missing) or 'none') + '\n')
    for name, packages in [('GTK4/libadwaita', ['gtk4', 'libadwaita']), ('Qt6', ['qt6-base', 'qt6-tools', 'qt6-declarative'])]:
        if all(p in native for p in packages):
            for module in (['gtk4', 'libadwaita-1'] if name == 'GTK4/libadwaita' else ['Qt6Core']):
                code, version, _ = query('pkg-config', '--modversion', module, timeout=8)
                if not code:
                    record(module + ' version: ' + version.strip() + '\n')
        record(name + ': ' + ('installed' if all(p in native for p in packages) else 'missing ' + ', '.join(p for p in packages if p not in native)) + '\n')
    if 'rustup' in native:
        code, text, error = query('rustup', 'toolchain', 'list')
        record('Rustup toolchains: ' + (text.strip() if not code else error.strip()) + '\n')
    record('Readiness describes installed tools, not running services. No shell, service, database, account or group configuration was changed.\n')

    emit(readiness_report=''.join(lines), missing_dependencies=missing)

@contextlib.contextmanager
def operation_lock():
    directory = Path(os.environ.get('XDG_RUNTIME_DIR', str(Path(os.environ.get('XDG_CACHE_HOME', str(Path.home() / '.cache'))) / 'archeasy')))
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    with (directory / 'archeasy-packages.lock').open('a') as lock, (directory / 'archeasy-update.lock').open('a') as update:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(update, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('Another ArchEasy package operation is active. Wait for it to finish and retry.')
        yield


def flathub(session, app):
    code, text, error = query('flatpak', 'remotes', '--user', '--columns=name,url')
    if code:
        raise RuntimeError('Cannot inspect Flatpak remotes: ' + error)
    remotes = dict(line.split('\t', 1) for line in text.splitlines() if '\t' in line)
    if 'flathub' not in remotes:
        session.consent('Add official Flathub for your account?\nExact operation: flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo')
        session.run(['flatpak', 'remote-add', '--user', '--if-not-exists', 'flathub', 'https://dl.flathub.org/repo/flathub.flatpakrepo'])
        code, text, error = query('flatpak', 'remotes', '--user', '--columns=name,url')
        remotes = dict(line.split('\t', 1) for line in text.splitlines() if '\t' in line)
    if code or remotes.get('flathub', '').rstrip('/') != 'https://dl.flathub.org/repo':
        raise RuntimeError('Refusing an unverified Flathub remote URL')
    code, info, error = query('flatpak', 'remote-info', '--user', 'flathub', app)
    if code:
        raise RuntimeError('Flatpak app unavailable: ' + app + ': ' + error)
    command = ['flatpak', 'install', '--user', 'flathub', app]
    session.consent(info + '\nInstall this sandboxed app?\n' + shlex.join(command))
    session.run(command)


def aur_preflight(package):
    if not shutil.which('git'):
        # Fresh systems may not have Git until the approved native prerequisite stage.
        url = 'https://aur.archlinux.org/rpc/v5/info?arg[]=' + package
        with urllib.request.urlopen(url, timeout=30) as response:
            data = json.load(response)
        matches = [p for p in data.get('results', []) if p.get('Name') == package and p.get('PackageBase') == package]
        if not matches:
            raise RuntimeError('AUR package unavailable: ' + package)
        return None
    code, text, error = query('git', '-c', 'core.hooksPath=/dev/null', 'ls-remote', '--exit-code', 'https://aur.archlinux.org/' + package + '.git', 'HEAD')
    if code or not re.match(r'^[0-9a-f]{40,64}\s+HEAD', text):
        raise RuntimeError('Cannot verify AUR package repository: ' + package + ': ' + error)
    return text.split()[0]


def srcinfo(text):
    result = {}
    for line in text.splitlines():
        if ' = ' in line:
            key, value = line.strip().split(' = ', 1)
            result.setdefault(key, []).append(value)
    return result


def aur_install(session, package, commit):
    if os.geteuid() == 0:
        raise RuntimeError('AUR builds must run as a normal desktop user')
    with tempfile.TemporaryDirectory(prefix='archeasy-developer-aur-') as directory:
        root = Path(directory) / 'source'
        session.run(['git', '-c', 'core.hooksPath=/dev/null', '-c', 'init.templateDir=', 'clone', '--depth', '1', '--', 'https://aur.archlinux.org/' + package + '.git', str(root)])
        code, revision, error = query('git', '-C', str(root), 'rev-parse', 'HEAD')
        if code or not re.fullmatch(r'[0-9a-f]{40,64}', revision.strip()) or (commit is not None and revision.strip() != commit):
            raise RuntimeError('AUR recipe changed since planning. Retry to review the new revision.')
        commit = revision.strip()
        code, files, error = query('git', '-C', str(root), 'ls-files')
        if code or not files.strip():
            raise RuntimeError('Cannot inspect AUR build files: ' + error)
        review, hashes = [], {}
        for name in files.splitlines():
            path = root / name
            if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
                raise RuntimeError('Unsafe AUR build file: ' + name)
            data = path.read_bytes()
            if len(data) > 262144 or sum(len(t) for t in review) + len(data) > 524288:
                raise RuntimeError('AUR review is too large; review and install this package manually.')
            try:
                text = data.decode('utf-8')
            except UnicodeError:
                raise RuntimeError('AUR contains binary build files; manual review required.')
            hashes[name] = hashlib.sha256(data).hexdigest()
            review.append('\n=== ' + name + ' ===\n' + text)
        if 'PKGBUILD' not in hashes or '.SRCINFO' not in hashes:
            raise RuntimeError('AUR recipe lacks PKGBUILD or .SRCINFO')
        info = srcinfo((root / '.SRCINFO').read_text())
        if package not in info.get('pkgname', []) or any(not NAME.fullmatch(p) for p in info.get('pkgname', [])):
            raise RuntimeError('Unexpected AUR package identity')
        for conflict in info.get('conflicts', []) + info.get('conflicts_' + os.uname().machine, []):
            if conflict_present(conflict):
                raise RuntimeError('AUR package conflicts with installed provider ' + conflict)
        session.consent('Review ALL files below before trusting this AUR recipe. Building executes its code as YOUR USER, with access to your files. ArchEasy has not audited it. No code has run yet. Approve only if you trust the recipe and its sources.\nRevision: ' + commit + '\n' + ''.join(review))
        # Resolve declared build/runtime/test dependencies without running PKGBUILD.
        deps = []
        for key in ('depends', 'makedepends', 'checkdepends'):
            deps += info.get(key, []) + info.get(key + '_' + os.uname().machine, [])
        code, missing, error = query('pacman', '-T', '--', *deps) if deps else (0, '', '')
        if code not in (0, 127) or error.strip() or (code == 127 and not missing.strip()) or (code == 0 and missing.strip()):
            raise RuntimeError('Cannot inspect AUR dependencies: ' + error + missing)
        available = availability()
        targets = []
        for dependency in missing.splitlines():
            name = re.split(r'[<>=]', dependency)[0]
            if not NAME.fullmatch(name) or name not in available:
                raise RuntimeError('AUR dependency unavailable officially: ' + dependency + '. Recursive unreviewed AUR builds are not allowed.')
            targets.append(available[name])
        if targets:
            native, _ = installed()
            resolved = native_preflight(targets, native)
            command = ['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-S', '--needed', '--'] + targets
            session.consent('Install these declared AUR prerequisites?\n' + resolved + '\n' + shlex.join(command))
            session.run(command)
        if deps and query('pacman', '-T', '--', *deps)[0]:
            raise RuntimeError('AUR dependency versions are still unsatisfied')
        # --nodeps prevents makepkg from starting its own package-manager operation.
        command = ['makepkg', '--nodeps', '--cleanbuild']
        session.consent('Run the reviewed build as your user?\nWorking directory: ' + str(root) + '\n' + shlex.join(command))
        # Recheck after the last permission dialog, immediately before execution.
        for name, digest in hashes.items():
            path = root / name
            if path.is_symlink() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
                raise RuntimeError('AUR files changed after review. Refusing to build.')
        with working_directory(root):
            session.run(command)
            code, packages, error = query('makepkg', '--packagelist')
        if code or not packages.strip():
            raise RuntimeError('Cannot locate built AUR packages: ' + error)
        archives = []
        for value in packages.splitlines():
            path = Path(value)
            if not path.is_absolute():
                path = root / path
            # User makepkg.conf may redirect outputs. Do not install unrelated files.
            if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
                raise RuntimeError('Built archive is outside the reviewed build directory')
            code, identity, error = query('pacman', '-Qp', '--', str(path))
            if code or not identity.split() or identity.split()[0] not in info['pkgname']:
                raise RuntimeError('Built archive identity differs from reviewed .SRCINFO')
            if identity.split()[0] == package:
                archives.append(str(path))
        if not archives:
            raise RuntimeError('The selected AUR package was not produced by the build')
        command = ['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-U', '--needed', '--'] + archives
        session.consent('Install the locally built, reviewed package archives?\n' + shlex.join(command))
        session.run(command)


@contextlib.contextmanager
def working_directory(path):
    old = Path.cwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(old)


def npm_install(session):
    code, text, error = query('npm', 'view', '@openai/codex', 'name', 'version', 'dist.integrity', '--json', '--registry=' + REGISTRY, '--@openai:registry=' + REGISTRY, '--ignore-scripts')
    if code:
        raise RuntimeError('Cannot validate official Codex npm package: ' + error)
    metadata = json.loads(text)
    version = metadata.get('version', '')
    if metadata.get('name') != '@openai/codex' or not re.fullmatch(r'\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?', version) or not metadata.get('dist.integrity', '').startswith('sha512-'):
        raise RuntimeError('Unexpected Codex npm metadata')
    command = ['npm', 'install', '--prefix', str(PREFIX), '--ignore-scripts', '--no-audit', '--no-fund', '--registry=' + REGISTRY, '--@openai:registry=' + REGISTRY, '@openai/codex@' + version]
    session.consent('Install the official Codex npm package at the pinned version below? Lifecycle scripts are disabled. No shell profile or global npm configuration changes are made.\nIntegrity: ' + metadata['dist.integrity'] + '\n' + shlex.join(command))
    session.run(command)
    code, text, error = query(str(PREFIX / 'node_modules/.bin/codex'), '--version')
    if code:
        raise RuntimeError('Codex installation did not pass its version check. No downloaded lifecycle script will be run automatically. ' + error)
    emit(log='Codex installed at ' + str(PREFIX / 'node_modules/.bin/codex') + '\n' + text)


def rust_toolchain(session):
    code, text, error = query('rustup', 'toolchain', 'list')
    if code:
        raise RuntimeError('Cannot inspect rustup toolchains: ' + error)
    if any(line.startswith('stable-') for line in text.splitlines()) and query('rustup', 'run', 'stable', 'rustc', '--version')[0] == 0:
        emit(log='Rustup stable toolchain already installed; preserved existing default.\n')
        return
    command = ['rustup', 'toolchain', 'install', 'stable', '--profile', 'minimal']
    session.consent('Install the stable Rust toolchain into your user account? Existing default and shell files remain unchanged.\n' + shlex.join(command))
    session.run(command)
    if query('rustup', 'run', 'stable', 'rustc', '--version')[0]:
        raise RuntimeError('Rust stable toolchain verification failed')
    emit(log='Rust installed. Use rustup run stable cargo for this toolchain, or choose your default separately. No existing default was overwritten.\n')


def main(mode, request):
    validate_manifest()
    if mode == 'catalog':
        emit(catalog=MANIFEST['tools'], presets=MANIFEST['presets'])
        return
    if mode not in ('check', 'install') or not isinstance(request, dict):
        raise ValueError('Invalid developer request')
    if not supported(Path('/etc/os-release').read_text()):
        raise RuntimeError('Developer setup requires Arch Linux or a distribution declaring ID_LIKE=arch')
    native, flat = installed()
    if mode == 'check':
        report(native, flat, request.get('sources', {}))
        return
    session = Session()
    active = []
    with operation_lock():
        try:
            session.checkpoint()
            native, flat = installed()
            result = plan(request, native, flat, availability())
            for key in result['selected']:
                emit(tool_status={'id': key, 'state': 'Installed' if is_installed(tools()[key], result['sources'][key], native, flat) else 'Not Installed'})
            active = result['selected']
            if any(step['kind'] == 'aur' for step in result['steps']) and os.geteuid() == 0:
                raise RuntimeError('AUR builds must run as a normal desktop user')
            resolution = native_preflight(result['targets'], native)
            revisions = {}
            for step in result['steps']:
                session.checkpoint()
                if step['kind'] == 'aur':
                    revisions[step['id']] = aur_preflight(step['package'])
            review = 'Selected tools and required prerequisites:\n' + '\n'.join(tools()[key]['title'] + ' [' + result['sources'][key] + ']' for key in active)
            review += '\n\nExact operations (archive paths / npm version are confirmed again when known):\n' + '\n'.join(shlex.join(args) for args in result['operations'])
            review += '\n\nPacman dependency plan:\n' + (resolution or 'No native packages needed.')
            review += '\n\nNo repository refresh or whole-system upgrade is performed. Keep Arch up to date before installing. Services, shell profiles, databases, privileged groups and existing user configurations are not configured automatically. Review every package-manager prompt.'
            emit(log=review + '\n')
            session.consent(review)
            stages = len(result['steps']) + bool(result['targets']) + ('rust' in active)
            finished = 0
            if result['targets']:
                session.run(result['operations'][0])
                finished += 1
                emit(progress=finished / max(1, stages))
            for step in result['steps']:
                session.checkpoint()
                if step['kind'] == 'flatpak':
                    flathub(session, step['app'])
                elif step['kind'] == 'aur':
                    aur_install(session, step['package'], revisions[step['id']])
                else:
                    npm_install(session)
                finished += 1
                emit(progress=finished / max(1, stages))
            if 'rust' in active:
                rust_toolchain(session)
            native, flat = installed()
            missing = [key for key in active if not is_installed(tools()[key], result['sources'][key], native, flat)]
            if missing:
                raise RuntimeError('Still missing after installation: ' + ', '.join(missing))
            for key in active:
                emit(tool_status={'id': key, 'state': 'Installed'})
            emit(completed=[tools()[key]['title'] for key in active], progress=1.0)
            emit(log='Developer setup complete. Services remain disabled unless you had already enabled them. Existing shell settings and user groups are preserved.\n')
        except Exception:
            # Recover from partial success by querying actual state, never assuming rollback.
            try:
                native, flat = installed()
                pending = []
                for key in active or expand(request.get('selected', [])):
                    source = selected_source(tools()[key], request.get('sources', {}))
                    complete = is_installed(tools()[key], source, native, flat)
                    # A failed rustup stage must remain retryable even if the package installed.
                    if key == 'rust':
                        complete = complete and query('rustup', 'run', 'stable', 'rustc', '--version')[0] == 0
                    emit(tool_status={'id': key, 'state': 'Installed' if complete else 'Failed'})
                    if not complete:
                        pending.append(key)
                emit(failed=pending)
            except Exception:
                emit(failed=active or request.get('selected', []))
            raise


if __name__ == '__main__':
    import sys
    try:
        main(sys.argv[1], json.loads(sys.argv[2]) if len(sys.argv) > 2 else {})
        emit(done=True)
    except Exception as error:
        emit(error=str(error))
        sys.exit(1)
