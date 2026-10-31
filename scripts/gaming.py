"""Gaming backend. JSON-lines protocol; no GTK, shell execution, or AUR helpers.

Requests on stdin: {"answer": "..."}, {"cancel": true}.
Pacman remains interactive: every answer comes from the user, never defaults here.
"""
import fcntl
import glob
import json
import os
import errno
import pty
import re
import termios
from pathlib import Path
import queue
import selectors
import shutil
import subprocess
import sys
import threading

CATALOG = [
    ('steam', 'Steam', ['steam'], 'com.valvesoftware.Steam', True),
    ('heroic', 'Heroic Games Launcher', ['heroic-games-launcher'], 'com.heroicgameslauncher.hgl', True),
    ('lutris', 'Lutris', ['lutris'], 'net.lutris.Lutris', True),
    ('faugus', 'Faugus Launcher (optional)', ['faugus-launcher'], 'io.github.Faugus.faugus-launcher', False),
    ('mangohud', 'MangoHud', ['mangohud', 'lib32-mangohud'], None, True),
    ('goverlay', 'GOverlay (advanced)', ['goverlay'], None, False),
    ('gamescope', 'Gamescope (advanced)', ['gamescope'], None, False),
    ('gamemode', 'GameMode', ['gamemode', 'lib32-gamemode'], None, True),
    ('wine', 'Wine and Winetricks (advanced)', ['wine', 'winetricks'], None, False),
    ('protontricks', 'Protontricks (advanced)', ['protontricks'], 'com.github.Matoking.protontricks', False),
    ('umu', 'UMU Launcher (advanced)', ['umu-launcher'], None, False),
    ('vulkan', 'Vulkan tools and dependencies', ['vulkan-tools', 'vulkan-icd-loader', 'lib32-vulkan-icd-loader'], None, True),
]
OFFICIAL = ('core', 'extra', 'multilib')


def emit(**event):
    print(json.dumps(event), flush=True)


def query(*args):
    try:
        p = subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=45, env={**os.environ, 'LC_ALL': 'C'})
        return p.returncode, p.stdout, p.stderr
    except (OSError, subprocess.TimeoutExpired) as e:
        return 127, '', str(e)


def supported(text):
    fields = dict(line.split('=', 1) for line in text.splitlines() if '=' in line)
    return fields.get('ID', '').strip('"') == 'arch' or 'arch' in fields.get('ID_LIKE', '').strip('"').split()


def graphics(devices):
    """Require an existing bound driver; never select or replace kernel drivers."""
    packages = ['pciutils', 'vulkan-tools', 'vulkan-icd-loader', 'lib32-vulkan-icd-loader']
    for vendor, driver in devices:
        if vendor == '0x1002' and driver == 'amdgpu':
            packages += ['mesa', 'lib32-mesa', 'vulkan-radeon', 'lib32-vulkan-radeon']
        elif vendor == '0x8086' and driver in ('i915', 'xe'):
            packages += ['mesa', 'lib32-mesa', 'vulkan-intel', 'lib32-vulkan-intel']
        elif vendor == '0x10de' and driver == 'nvidia':
            packages += ['nvidia-utils', 'lib32-nvidia-utils']
        elif vendor == '0x10de' and driver == 'nouveau':
            packages += ['mesa', 'lib32-mesa', 'vulkan-nouveau', 'lib32-vulkan-nouveau']
        else:
            raise RuntimeError(f'Unsupported GPU/driver {vendor}/{driver or "unbound"}. Configure a working Vulkan driver first; ArchEasy will not replace it.')
    if not devices:
        raise RuntimeError('No supported display GPU detected. Check the graphics driver before installing.')
    return list(dict.fromkeys(packages))


def devices():
    found = []
    for name in glob.glob('/sys/bus/pci/devices/*'):
        p = Path(name)
        if (p / 'class').read_text().strip().startswith('0x03'):
            driver = (p / 'driver').resolve().name if (p / 'driver').exists() else ''
            found.append(((p / 'vendor').read_text().strip(), driver))
    return found


def installed():
    code, text, error = query('pacman', '-Qq')
    if code:
        raise RuntimeError(f'Cannot read installed packages: {error}')
    native = set(text.splitlines())
    flat = set()
    if shutil.which('flatpak'):
        for scope in ('--user', '--system'):
            code, text, error = query('flatpak', 'list', scope, '--app', '--columns=application')
            if code:
                raise RuntimeError(f'Cannot inspect {scope} Flatpaks: {error}')
            flat.update(text.splitlines())
    return native, flat


def repositories():
    code, text, error = query('pacman-conf', '--repo-list')
    if code:
        raise RuntimeError(f'Cannot inspect active repositories: {error}')
    return text.splitlines()


def availability():
    result = {}
    for repo in repositories():
        if repo not in OFFICIAL:
            continue
        code, text, error = query('pacman', '-Sl', repo)
        if code:
            raise RuntimeError(f'Cannot validate {repo}: {error}. Refresh the system databases and retry.')
        for line in text.splitlines():
            fields = line.split()
            if len(fields) >= 3 and fields[0] == repo:
                result.setdefault(fields[1], repo + '/' + fields[1])
    return result


def plan(selected, native, flat, available, dependencies):
    targets, fallbacks, missing = [], [], []
    for key, title, packages, app, _ in CATALOG:
        if key not in selected:
            continue
        if all(p in native for p in packages) or (app and app in flat):
            continue
        needed = [p for p in packages if p not in native]
        if all(p in available for p in needed):
            targets += needed
        elif app:
            fallbacks.append((title, app))
        else:
            missing += [p for p in needed if p not in available]
    targets += [p for p in dependencies if p not in native]
    missing += [p for p in targets if p not in available]
    if missing:
        raise RuntimeError('Unavailable official packages: ' + ', '.join(sorted(set(missing))) + '. No untrusted repository or AUR fallback is used.')
    if fallbacks and 'flatpak' not in native:
        if 'flatpak' not in available:
            raise RuntimeError('Flatpak is unavailable in official repositories.')
        targets.append('flatpak')
    return [available[p] for p in dict.fromkeys(targets)], fallbacks


def needs_multilib(selected, native, flat, dependencies):
    needed = [p for p in dependencies if p not in native]
    for key, _, packages, app, _ in CATALOG:
        if key in selected and not (app and app in flat):
            needed += [p for p in packages if p not in native]
    return any(p in ('steam', 'umu-launcher') or p.startswith('lib32-') for p in needed)


def validate_nvidia_pair(native, available, dependencies):
    if 'nvidia-utils' not in dependencies or 'lib32-nvidia-utils' in native:
        return
    target = available.get('lib32-nvidia-utils')
    if not target:
        return  # planner reports the unavailable library
    code, current, error = query('pacman', '-Q', 'nvidia-utils')
    code2, info, error2 = query('pacman', '-Si', target)
    version = next((line.split(':', 1)[1].strip() for line in info.splitlines() if line.startswith('Version') and ':' in line), '')
    current = current.split()
    def upstream(value):
        return value.split(':')[-1].rsplit('-', 1)[0]
    if code or code2 or len(current) != 2 or not version or upstream(current[1]) != upstream(version):
        raise RuntimeError('The official 32-bit NVIDIA library does not match the installed NVIDIA userspace. Update your existing driver stack first, restart if required, then retry. No graphics driver was replaced. ' + error + error2)


# Runs only after a dedicated GUI permission prompt and polkit authentication.
# Compare the exact inspected content, preserve permissions, and retain a backup.
MULTILIB_HELPER = r'''
import os, pathlib, re, stat, sys, tempfile
p = pathlib.Path('/etc/pacman.conf')
if p.is_symlink(): raise SystemExit('Refusing symlink pacman.conf')
old = p.read_text()
metadata = p.stat()
if old != sys.argv[1]: raise SystemExit('pacman.conf changed; run setup again')
pattern = r'(?m)^#[ \t]*\[multilib\][ \t]*\n#[ \t]*(Include[ \t]*=[ \t]*/etc/pacman.d/mirrorlist)[ \t]*$'
new, count = re.subn(pattern, r'[multilib]\n\1', old)
if count != 1: raise SystemExit('Enable multilib manually: standard commented section not found')
backup = p.with_name('pacman.conf.archeasy-gaming.bak')
with backup.open('x') as f:
    os.fchmod(f.fileno(), stat.S_IMODE(metadata.st_mode))
    os.fchown(f.fileno(), metadata.st_uid, metadata.st_gid)
    f.write(old); f.flush(); os.fsync(f.fileno())
fd, name = tempfile.mkstemp(prefix='.pacman.conf.archeasy-', dir='/etc')
try:
    with os.fdopen(fd, 'w') as f:
        os.fchmod(f.fileno(), stat.S_IMODE(metadata.st_mode))
        os.fchown(f.fileno(), metadata.st_uid, metadata.st_gid)
        f.write(new); f.flush(); os.fsync(f.fileno())
    if p.is_symlink() or p.read_text() != old: raise SystemExit('pacman.conf changed during setup; refusing overwrite')
    os.replace(name, p)
finally:
    if os.path.exists(name): os.unlink(name)
'''


class Session:
    def __init__(self):
        self.messages = queue.Queue()
        self.cancelled = False
        def read():
            # Raw reads avoid buffered stdin locks during interpreter shutdown.
            pending = b''
            while True:
                chunk = os.read(sys.stdin.fileno(), 4096)
                if not chunk:
                    break
                pending += chunk
                while b'\n' in pending:
                    line, pending = pending.split(b'\n', 1)
                    try:
                        message = json.loads(line)
                        if isinstance(message, dict):
                            self.messages.put(message)
                    except ValueError:
                        pass
            self.messages.put({'cancel': True, 'disconnect': True})
        threading.Thread(target=read, daemon=True).start()

    def checkpoint(self):
        while not self.messages.empty():
            self.cancelled |= bool(self.messages.get().get('cancel'))
        if self.cancelled:
            raise RuntimeError('Cancelled between stages. Completed transactions are preserved; retry to install the remaining packages.')
        if Path('/var/lib/pacman/db.lck').exists():
            raise RuntimeError('Another package operation is active. Wait for it to finish and retry; never remove db.lck.')

    def consent(self, message):
        self.checkpoint()
        emit(prompt=message)
        while True:
            msg = self.messages.get()
            if msg.get('cancel') or msg.get('answer', '').lower() not in ('y', 'yes'):
                raise RuntimeError('Stage declined. No further installation stages started.')
            return

    def run(self, args):
        self.checkpoint()
        emit(log='\nStage: ' + ('Enable multilib (with configuration backup)' if args[0:3] == ['pkexec', '--disable-internal-agent', '/usr/bin/python3'] else ' '.join(args)) + '\n')
        # A private PTY keeps native package-manager prompts interactive without
        # opening a terminal window. Polkit's terminal password fallback is disabled.
        master, slave = pty.openpty()
        settings = termios.tcgetattr(slave)
        settings[3] &= ~termios.ECHO
        termios.tcsetattr(slave, termios.TCSANOW, settings)
        selector = selectors.DefaultSelector()
        try:
            with subprocess.Popen(args, stdin=slave, stdout=slave, stderr=slave,
                                  env={**os.environ, 'LC_ALL': 'C', 'TERM': 'dumb', 'NO_COLOR': '1'}) as child:
                os.close(slave)
                slave = -1
                selector.register(master, selectors.EVENT_READ)
                while selector.get_map():
                    for key, _ in selector.select(.1):
                        try:
                            data = os.read(master, 4096)
                        except OSError as error:
                            if error.errno != errno.EIO:
                                raise
                            data = b''  # Linux PTY slave closed after process exit.
                        if data:
                            text = data.decode(errors='replace').replace('\r\n', '\n').replace('\r', '\n')
                            emit(log=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', text))
                        else:
                            selector.unregister(master)
                    while not self.messages.empty():
                        msg = self.messages.get()
                        self.cancelled |= bool(msg.get('cancel'))
                        if msg.get('disconnect') and master != -1:
                            if selector.get_map():
                                selector.unregister(master)
                            os.close(master)
                            master = -1
                        if 'answer' in msg and master != -1:
                            try:
                                os.write(master, (msg['answer'] + '\n').encode())
                            except OSError as error:
                                if error.errno != errno.EIO:
                                    raise
                    # Never kill an active transaction for a stage cancellation.
                if child.wait():
                    raise RuntimeError('Installation command failed or authentication was cancelled. Ensure a desktop polkit authentication agent is running for administrator requests. Review the log, resolve the reported issue, and retry. Installed packages will be skipped.')
        finally:
            selector.close()
            if master != -1:
                os.close(master)
            if slave != -1:
                os.close(slave)
        self.checkpoint()


def check():
    if not supported(Path('/etc/os-release').read_text()):
        raise RuntimeError('Gaming setup supports Arch Linux and distributions declaring ID_LIKE=arch only.')
    if os.uname().machine != 'x86_64':
        raise RuntimeError('Gaming setup currently supports x86_64 Arch systems with multilib only.')
    native, flat = installed()
    gpu = devices()
    code, model, _ = query('lspci', '-nnk')
    display_lines = []
    in_gpu = False
    for line in model.splitlines():
        if line and not line[0].isspace():
            in_gpu = any(s in line for s in ('VGA ', '3D controller', 'Display controller'))
        if in_gpu:
            display_lines.append(line)
    display = '\n'.join(display_lines)
    emit(log='GPU model / graphics driver:\n' + (display or 'Model unavailable (installing pciutils will enable model lookup); detected vendor/driver: ' + str(gpu)) + '\n')
    code, text, error = query('vulkaninfo', '--summary')
    emit(log='Vulkan: ' + ('available\n' + text if code == 0 else 'unavailable or unverified: ' + error) + '\n')
    for _, title, packages, app, _ in CATALOG:
        state = 'installed' if all(p in native for p in packages) else 'Flatpak installed' if app in flat else 'missing: ' + ', '.join(p for p in packages if p not in native)
        emit(log=f'{title}: {state}\n')
    deps = graphics(gpu)
    # Existing alternative Vulkan stacks require a manual review, not replacement.
    if any('amdvlk' in p or 'amdgpu-pro' in p for p in native) and any(v == '0x1002' for v, _ in gpu):
        raise RuntimeError('Existing AMDVLK/proprietary AMD stack detected. Review matching 32-bit Vulkan libraries manually; ArchEasy will not switch your graphics stack.')
    if any(d == 'nvidia' for _, d in gpu) and 'nvidia-utils' not in native:
        raise RuntimeError('Custom/legacy NVIDIA userspace detected. Install matching 32-bit libraries manually; no driver replacement attempted.')
    emit(log='Missing graphics dependencies: ' + (', '.join(p for p in deps if p not in native) or 'none') + '\n')
    return native, flat, deps


def main(mode, selected):
    if mode == 'catalog':
        emit(catalog=[{'id': k, 'title': t, 'recommended': r} for k, t, _, _, r in CATALOG])
        return
    if mode not in ('check', 'install') or not set(selected) <= {x[0] for x in CATALOG}:
        raise RuntimeError('Invalid gaming request')
    native, flat, deps = check()
    if mode == 'check':
        return
    if not selected:
        raise RuntimeError('Select at least one gaming application.')
    session = Session()
    runtime = Path(os.environ.get('XDG_RUNTIME_DIR', str(Path(os.environ.get('XDG_CACHE_HOME', str(Path.home() / '.cache'))) / 'archeasy')))
    runtime.mkdir(mode=0o700, parents=True, exist_ok=True)
    # Same lock as ArchEasy system updates; held throughout native and Flatpak stages.
    with (runtime / 'archeasy-packages.lock').open('a') as lock, (runtime / 'archeasy-update.lock').open('a') as update_lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(update_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('An ArchEasy package operation is already active.')
        session.checkpoint()
        if needs_multilib(selected, native, flat, deps) and 'multilib' not in repositories():
            old = Path('/etc/pacman.conf').read_text()
            session.consent('Enable multilib in /etc/pacman.conf? A backup will be retained. Approve to allow this specific configuration change, or Stop to leave the file unchanged.')
            session.run(['pkexec', '--disable-internal-agent', '/usr/bin/python3', '-I', '-c', MULTILIB_HELPER, old])
            session.consent('Multilib enabled. Refresh databases AND upgrade the whole system now? Review and answer pacman prompts in the response field. Approve to continue.')
            session.run(['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-Syu'])
        available = availability()
        # A preceding full upgrade may have changed installed packages.
        native, flat = installed()
        validate_nvidia_pair(native, available, deps)
        targets, fallbacks = plan(selected, native, flat, available, deps)
        emit(log='Official targets: ' + ', '.join(targets) + '\nFlathub fallbacks: ' + ', '.join(t for t, _ in fallbacks) + '\n')
        if targets:
            session.consent('Install the listed official packages and perform a full system upgrade? Approve to continue. Pacman will then ask for transaction approval; enter each response below. Review any replacement or removal request carefully.')
            session.run(['pkexec', '--disable-internal-agent', '/usr/bin/pacman', '-Syu', '--needed', '--'] + targets)
        if fallbacks:
            # Never trust a user-defined remote just because it is named flathub.
            for title, app in fallbacks:
                session.checkpoint()
                code, info, err = query('flatpak', 'remotes', '--user', '--columns=name,url')
                if code:
                    raise RuntimeError('Cannot inspect Flatpak remotes: ' + err)
                remotes = dict(line.split('\t', 1) for line in info.splitlines() if '\t' in line)
                if 'flathub' not in remotes:
                    session.consent('Add the official Flathub remote to your user account for ' + title + '? Approve to allow.')
                    session.run(['flatpak', 'remote-add', '--user', '--if-not-exists', 'flathub', 'https://dl.flathub.org/repo/flathub.flatpakrepo'])
                elif remotes['flathub'].rstrip('/') != 'https://dl.flathub.org/repo':
                    raise RuntimeError('The flathub remote has an unexpected URL; verify it manually before retrying.')
                code, _, err = query('flatpak', 'remote-info', '--user', 'flathub', app)
                if code:
                    raise RuntimeError(f'{title} is unavailable from Flathub: {err}')
                session.consent(f'Install {title} from official Flathub for your user account? Approve to continue, then answer Flatpak prompts below.')
                session.run(['flatpak', 'install', '--user', 'flathub', app])
        native, flat = installed()
        incomplete = [t for k, t, ps, app, _ in CATALOG if k in selected and not (all(p in native for p in ps) or app in flat)]
        incomplete += [p for p in deps if p not in native]
        if incomplete:
            raise RuntimeError('Still missing after installation: ' + ', '.join(incomplete))
        emit(completed=[t for k, t, _, _, _ in CATALOG if k in selected])
        emit(log='\nCompleted applications:\n' + '\n'.join(t for k, t, _, _, _ in CATALOG if k in selected) + '\n')
        code, text, error = query('vulkaninfo', '--summary')
        emit(log='Final Vulkan check: ' + ('available' if code == 0 else 'not verified; restart your session and check again: ' + error) + '\n')
        if code:
            emit(warning='Applications installed, but Vulkan is not verified. Restart your session and run Gaming System Check; review graphics errors in the log.')
        emit(log='Steam Play / Proton: Open Steam → Settings → Compatibility. Enable Steam Play for other titles if offered and select Proton. Per game: Properties → Compatibility → Force a specific Steam Play compatibility tool. Steam downloads Proton when needed. No Steam configuration was overwritten.\n')


if __name__ == '__main__':
    try:
        main(sys.argv[1], sys.argv[2:])
        emit(done=True)
    except Exception as e:
        emit(error=str(e))
        sys.exit(1)
