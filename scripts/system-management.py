"""ArchEasy management backend. JSON requests; no shell evaluation or live rollback.
All mutating operations are interactive terminal jobs, never GUI privilege escalation.
"""
import datetime
import errno
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import termios

TERMINAL_LOG = None
import tempfile
import time


class OperationError(Exception):
    pass


def run(args, cancel=None, timeout=45, accepted=(0,), interactive=False):
    """Bounded diagnostics; atomic terminal transactions deliberately cannot be cancelled."""
    if cancel is not None and cancel.is_set():
        raise OperationError('Cancelled before command started')
    try:
        if interactive:
            return run_terminal(args, accepted)
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                     env={**os.environ, 'LC_ALL': 'C'})
            started = time.monotonic()
            while child.poll() is None:
                if os.fstat(stdout.fileno()).st_size + os.fstat(stderr.fileno()).st_size > 2 * 1024 * 1024:
                    child.kill()
                    child.wait()
                    raise OperationError('Diagnostic output exceeded 2 MiB; inspect with the native tool')
                if (cancel and cancel.is_set()) or time.monotonic() - started > timeout:
                    child.terminate()
                    try:
                        child.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait()
                    raise OperationError('Cancelled or timed out; diagnostic did not complete')
                time.sleep(.05)
            stdout.seek(0)
            stderr.seek(0)
            text = stdout.read(2 * 1024 * 1024).decode('utf-8', 'replace')
            error = stderr.read(65536).decode('utf-8', 'replace')
            if child.returncode not in accepted:
                raise OperationError(f'{args[0]} failed (exit {child.returncode}): {error or text}')
            return text.strip()
    except OSError as error:
        raise OperationError(f'{args[0]} unavailable: {error}') from error


def run_terminal(args, accepted):
    # A terminal output descriptor lets pacman render and flush progress bars.
    # Keep stdin and the controlling terminal inherited: never relay or log
    # keyboard input, including sudo passwords and transaction responses.
    master, slave = os.openpty()
    with os.fdopen(master, 'rb', buffering=0) as output, os.fdopen(slave, 'wb', buffering=0) as terminal:
        attributes = termios.tcgetattr(terminal)
        attributes[1] &= ~termios.OPOST  # The outer terminal handles newlines.
        termios.tcsetattr(terminal, termios.TCSANOW, attributes)
        try:
            size = fcntl.ioctl(sys.stdout.fileno(), termios.TIOCGWINSZ, bytes(8))
            fcntl.ioctl(terminal.fileno(), termios.TIOCSWINSZ, size)
        except (OSError, ValueError):
            fcntl.ioctl(terminal.fileno(), termios.TIOCSWINSZ,
                        struct.pack('HHHH', 24, 80, 0, 0))
        child = subprocess.Popen(args, stdin=None, stdout=terminal, stderr=subprocess.STDOUT,
                                 env={**os.environ, 'LC_ALL': 'C'})
        terminal.close()  # The reader must see EOF when the child closes its tty.
        with open(TERMINAL_LOG, 'ab') if TERMINAL_LOG else tempfile.TemporaryFile() as log:
            while True:
                try:
                    block = output.read(4096)
                except OSError as error:
                    if error.errno != errno.EIO:  # Linux PTY end of stream.
                        raise
                    break
                if not block:
                    break
                sys.stdout.buffer.write(block)
                sys.stdout.buffer.flush()
                log.write(block)
                log.flush()
        child.wait()
        if child.returncode not in accepted:
            raise OperationError(f'{args[0]} failed (exit {child.returncode}); review terminal output and View Logs')
        return ''


def optional(args):
    try:
        return run(args)
    except OperationError as error:
        return f'Unavailable: {error}'


def filesystem(runner=run):
    data = json.loads(runner(['findmnt', '--json', '--target', '/', '--output', 'TARGET,SOURCE,FSTYPE,OPTIONS']))
    mounts = data.get('filesystems', [])
    if len(mounts) != 1 or mounts[0].get('target') != '/':
        raise OperationError('Root mount could not be identified')
    return mounts[0]


def snapshot_backend(fs, snapper_config, timeshift_config):
    # Do not silently select a provider when both own configurations.
    if snapper_config and timeshift_config:
        return 'conflict'
    if fs.get('fstype') == 'btrfs' and snapper_config:
        return 'snapper'
    if timeshift_config:
        return 'timeshift'
    return 'files'


def snapshot_context():
    fs = filesystem()
    snapper = False
    try:
        config = Path('/etc/snapper/configs/root').read_text()
        snapper = bool(re.search(r'^SUBVOLUME="/"\s*$', config, re.M)) and bool(re.search(r'^FSTYPE="btrfs"\s*$', config, re.M))
    except OSError:
        pass
    timeshift_path = Path('/etc/timeshift/timeshift.json')
    timeshift = timeshift_path.exists()
    backend = snapshot_backend(fs, snapper, timeshift)
    if Path('/etc/snapper/configs/root').exists() and timeshift:
        backend = 'conflict'
    elif backend == 'timeshift':
        try:
            config = json.loads(timeshift_path.read_text())
            if not config.get('backup_device_uuid'):
                backend = 'files'
            elif str(config.get('btrfs_mode')).lower() == 'true' and (
                fs.get('fstype') != 'btrfs' or not re.search(r'(?:^|,)subvol=/@(?:,|$)', fs.get('options', ''))
            ):
                backend = 'files'
        except (ValueError, OSError):
            backend = 'files'
    return fs, backend


def backups():
    fs, backend = snapshot_context()
    details = [f'Root filesystem: {fs}', f'Snapshot backend: {backend}',
               'Local snapshots do not protect against disk failure. Size may be shared, not reclaimable space.',
               'Portable ZIP backups work on ext4, XFS and other readable filesystems; they are file backups, not native snapshots.']
    if fs.get('fstype') == 'btrfs':
        details += [optional(['btrfs', 'subvolume', 'show', '/']), optional(['btrfs', 'subvolume', 'list', '/']), optional(['btrfs', 'filesystem', 'usage', '/'])]
    if backend == 'snapper':
        details.append(optional(['snapper', '-c', 'root', 'list']))
    elif backend == 'timeshift':
        details.append(optional(['timeshift', '--list']))
    elif backend == 'conflict':
        details.append('Both configurations exist. Resolve ownership in their tools before creating snapshots. No configuration is changed by ArchEasy.')
    else:
        details.append('No verified root snapshot configuration. Use Export Backup or configure one supported provider first.')
    return '\n\n'.join(details)


def package_id(value):
    if not isinstance(value, str) or not re.fullmatch(r'[a-zA-Z0-9@_+][a-zA-Z0-9@._+:-]*', value):
        raise OperationError('Invalid package identifier')
    return value


def parse_updates(text):
    rows = []
    for line in text.splitlines():
        fields = line.split()
        if len(fields) != 4 or fields[2] != '->':
            raise OperationError('Malformed update output; refresh before proceeding')
        rows.append({'package': package_id(fields[0]), 'installed': fields[1], 'available': fields[3]})
    return rows


def preflight(lock=Path('/var/lib/pacman/db.lck'), free=None, runner=run):
    if lock.exists():
        raise OperationError('Pacman lock exists. Wait for the owning operation; never delete its lock automatically.')
    processes = runner(['ps', '-eo', 'comm=']).splitlines()
    if set(p.strip() for p in processes) & {'pacman', 'paru', 'yay', 'pamac', 'packagekitd'}:
        raise OperationError('Another package manager is running; close it first.')
    if free is None:
        free = min(shutil.disk_usage('/').free, shutil.disk_usage('/var/cache/pacman/pkg').free)
    if free < 2 * 1024**3:
        raise OperationError('Less than 2 GiB available on root or package cache. Free space first; actual transaction may need more.')
    repositories = runner(['pacman-conf', '--repo-list'])
    if not repositories.strip():
        raise OperationError('No configured repositories')
    runner(['pacman', '-Dk'])
    return repositories


def update_plan(runner=run):
    # checkupdates uses a separate database, avoiding a partial-upgrade state.
    source = 'Fresh repository preview from checkupdates'
    warning = None
    try:
        text = runner(['checkupdates'], accepted=(0, 2))
    except OperationError as error:
        # Only a missing executable permits the cached preview. Do not conceal
        # refresh, database, timeout or cancellation failures.
        if not isinstance(error.__cause__, FileNotFoundError):
            raise
        text = runner(['pacman', '-Qu'], accepted=(0, 1))
        source = 'Cached preview (may be stale)'
        warning = ('checkupdates is not installed (provided by pacman-contrib). '
                   'This cached list may omit updates; an empty list does not mean the system is current. '
                   'pacman -Syu refreshes repositories and asks you to approve '
                   'the final full transaction. Review that transaction before confirming.')
    rows = parse_updates(text)
    critical = [r['package'] for r in rows if r['package'].startswith(('linux', 'nvidia')) or r['package'] in {'systemd', 'glibc', 'mesa', 'pacman', 'openssl'}]
    return {'official': rows, 'preview_source': source, 'warning': warning,
            'critical_or_reboot_review': critical,
            'transaction': ['sudo', 'pacman', '-Syu'],
            'reboot': 'Review after kernel, microcode, graphics or core library updates; running applications may retain old libraries.'}


def updates():
    sections = []
    try:
        sections.append('Configured repositories:\n' + preflight())
    except (OperationError, OSError) as error:
        sections.append('BLOCKED: ' + str(error))
    try:
        sections.append('Official update plan (refresh again at transaction time):\n' + json.dumps(update_plan(), indent=2))
    except OperationError as error:
        sections.append(str(error) + '\nCached fallback (may be stale):\n' + optional(['pacman', '-Qu']))
    sections += ['Installed official packages:\n' + optional(['pacman', '-Qn']),
                 'Foreign packages (AUR or locally installed, not official):\n' + optional(['pacman', '-Qm']),
                 'Installed Flatpaks:\n' + optional(['flatpak', 'list', '--columns=application,version,installation'])]
    for scope in ['--user', '--system']:
        sections.append(f'Flatpak {scope} updates:\n' + optional(['flatpak', 'remote-ls', '--updates', scope, '--columns=application,version']))
    sections.append('AUR updates are opt-in: review foreign packages and PKGBUILDs with your separately verified helper. ArchEasy does not run helpers automatically.')
    sections.append('Arch news/manual intervention: https://archlinux.org/news/ — read before approval. Network notices are not evidence that an update is safe.')
    try:
        sections.append('Recent pacman history (transaction started/completed markers matter):\n' + '\n'.join(Path('/var/log/pacman.log').read_text(errors='replace').splitlines()[-120:]))
    except OSError as error:
        sections.append(f'History unavailable: {error}')
    return '\n\n'.join(sections)


def driver_advice(pci, packages, available):
    """Recommendations are evidence-based and never install a driver."""
    packages, available = set(packages), set(available)
    text = []
    lower = pci.lower()
    candidates = []
    if 'amd' in lower or 'advanced micro devices' in lower:
        candidates += ['mesa', 'vulkan-radeon']
        text.append('AMD: inspect Kernel driver in use for amdgpu/radeon. Older generations need individual Vulkan compatibility review.')
    if 'intel' in lower:
        candidates += ['mesa', 'vulkan-intel']
        text.append('Intel: verify GPU generation before selecting Vulkan/media drivers. Do not install legacy xf86 drivers by default.')
    if 'nvidia' in lower:
        text.append('NVIDIA: PCI ID and active module identify hardware; generation/driver variant compatibility requires the current Arch NVIDIA guide. Nouveau may be intentional. No variant is selected automatically.')
        if any('nvidia' in p and 'dkms' in p for p in packages):
            for kernel in sorted(packages & {'linux', 'linux-lts', 'linux-zen', 'linux-hardened'}):
                if kernel + '-headers' not in packages:
                    text.append(f'Missing {kernel}-headers for installed NVIDIA DKMS; review every installed kernel.')
        text.append('Secure Boot may require signing and enrolling modules. Verify boot state before changing drivers; graphical boot changes require separate confirmation.')
    for name in sorted(set(candidates) - packages):
        if name in available:
            text.append(f'Review {name}: available in configured repositories, absent locally. Verify PCI generation compatibility before installation.')
    text.append('32-bit gaming: review matching lib32-mesa and vendor lib32 Vulkan packages only if multilib is enabled and 32-bit applications are needed.')
    return '\n'.join(text)


def read_file(path):
    try:
        return Path(path).read_text(errors='replace')[:65536].strip()
    except OSError as error:
        return f'Unavailable: {error}'


def graphics_devices(pci):
    """lspci -nnk has one unindented device line followed by indented details."""
    blocks = []
    current = []
    for line in pci.splitlines():
        if line and not line[0].isspace():
            if current:
                blocks.append(current)
            current = [line]
        elif current:
            current.append(line)
    if current:
        blocks.append(current)
    return '\n\n'.join('\n'.join(block) for block in blocks
                        if any(key in block[0] for key in ['VGA', '3D controller', 'Display controller']))


def hardware():
    pci = optional(['lspci', '-nnk'])
    graphics = graphics_devices(pci)
    packages = optional(['pacman', '-Qq']).splitlines()
    available = optional(['pacman', '-Slq']).splitlines()
    sections = ['CPU:\n' + optional(['lscpu']), 'RAM:\n' + read_file('/proc/meminfo'),
                'Motherboard: ' + read_file('/sys/class/dmi/id/board_name'),
                'Firmware: ' + read_file('/sys/class/dmi/id/bios_vendor') + ' ' + read_file('/sys/class/dmi/id/bios_version'),
                'Kernel: ' + optional(['uname', '-r']), 'PCI hardware / active drivers (graphics, network, audio):\n' + pci,
                'USB / Bluetooth:\n' + optional(['lsusb']), 'Network interfaces:\n' + optional(['ip', '-details', 'link']),
                'Bluetooth:\n' + optional(['bluetoothctl', 'list']),
                'Storage:\n' + optional(['lsblk', '--json', '-o', 'NAME,TYPE,SIZE,FSTYPE,MOUNTPOINTS,MODEL']),
                'Driver review:\n' + driver_advice(graphics, packages, available),
                'Firmware errors (permission may be required):\n' + optional(['journalctl', '-k', '-b', '--no-pager', '--grep=firmware', '-n', '80']),
                'Secure Boot:\n' + optional(['mokutil', '--sb-state'])]
    return '\n\n'.join(sections)


def destination(value, must_exist=False):
    if not isinstance(value, str) or not value or '\x00' in value or '\n' in value:
        raise OperationError('Choose an absolute local path')
    path = Path(value)
    if not path.is_absolute() or '..' in path.parts:
        raise OperationError('Choose an absolute local path without parent traversal')
    for part in [path, *path.parents]:
        if part.is_symlink():
            raise OperationError('Symlinked destinations are not supported')
    if path == Path('/') or path == Path.home() or not path.parent.is_dir():
        raise OperationError('Use a dedicated backup or new restore folder')
    if must_exist and not path.is_dir():
        raise OperationError('Repository folder must already exist')
    if not must_exist and path.exists():
        raise OperationError('Restore destination must not exist')
    return str(path)


def state_dir():
    path = Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'archeasy/management'
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    return path


def history():
    rows = []
    for path in sorted(state_dir().glob('*.json'))[-30:]:
        try:
            record = json.loads(path.read_text())
            rows.append(json.dumps(record, indent=2))
            log = path.with_suffix('.log')
            if log.exists():
                with open(log, 'rb') as stream:
                    stream.seek(max(0, log.stat().st_size - 65536))
                    rows.append(stream.read().decode('utf-8', 'replace'))
        except (ValueError, OSError):
            rows.append(f'Unreadable record: {path.name}')
    return '\n\n'.join(rows) or 'No recorded management operations.'


def record_write(path, record):
    temporary = path.with_suffix('.tmp')
    with open(temporary, 'w', encoding='utf-8') as stream:
        os.chmod(temporary, 0o600)
        json.dump(record, stream, indent=2)
    temporary.replace(path)


RECOVERY = '''Root snapshot restoration is an offline guided operation.
1. Export important current files to a different disk. A snapshot is not a full-disk backup.
2. Record findmnt /, lsblk -f, /etc/fstab, subvolume IDs and the bootloader actually in use. Boot/EFI and nested subvolumes may not be captured.
3. Boot a compatible Arch recovery USB. Identify the correct device and mount the snapshot read-only for inspection. Verify its files and required boot/kernel data before proceeding.
4. Follow the configured provider's recovery guide for this exact layout. Root data/configuration captured in that snapshot will replace newer versions; excluded home/boot data needs separate recovery. Do not run a generic live rollback or assume changing the default subvolume changes boot entries.
5. Confirm the selected snapshot, scope, integrity, bootloader and target devices with an experienced administrator before applying recovery. A reboot is required to use the recovered root.
ArchEasy never changes bootloader configuration or performs a live root rollback.'''

POLICY = '''Automatic snapshots and retention are managed by the existing provider, not a second ArchEasy scheduler.
Review Snapper root configuration and snapper-timeline.timer / snapper-cleanup.timer, or Timeshift scheduling and exclusions in its own interface. Do not enable two providers for the same root.
No retention policy is installed or executed by ArchEasy. Explicitly configure limits in your chosen provider only after verifying an external recovery copy. Preserve at least one validated recovery snapshot; provider cleanup may not enforce this for you.
Snapshot deletion: inspect the provider list and validate another recovery point first. Use its native interface for selected deletion; ArchEasy does not bulk-delete or enforce retention.
Pre-update backup is selected explicitly in Review Updates; failure stops the update.'''


def dispatch(request, interactive=False):
    action = request.get('action')
    if action == 'backups':
        return backups()
    if action == 'updates':
        return updates()
    if action == 'hardware':
        return hardware()
    if action == 'history':
        return history()
    if action == 'recovery':
        return RECOVERY
    if action == 'policy':
        return POLICY
    if action == 'compare':
        first, second = request.get('first', ''), request.get('second', '')
        if not all(re.fullmatch(r'[1-9][0-9]{0,9}', value) for value in [first, second]):
            raise OperationError('Enter two positive Snapper IDs from View Snapshots')
        if snapshot_context()[1] != 'snapper':
            raise OperationError('Comparison requires a verified, unambiguous Snapper root configuration')
        return run(['snapper', '-c', 'root', 'status', first + '..' + second])
    if action == 'acceleration':
        return optional(['glxinfo', '-B']) + '\n\n' + optional(['vulkaninfo', '--summary']) + '\nSoftware renderers such as llvmpipe/lavapipe indicate software rendering. A headless session may be unable to test graphics.'
    if action == 'firmware':
        return optional(['fwupdmgr', 'get-devices']) + '\n\n' + optional(['fwupdmgr', 'get-updates']) + '\nNo firmware is installed. Availability may use cached metadata; review fwupd and vendor instructions separately.'
    if not interactive:
        raise OperationError('This operation requires an explicitly approved interactive terminal')
    if os.geteuid() == 0:
        raise OperationError('Run ArchEasy as your normal user; elevate only individual commands')
    if action == 'official':
        print('Checking package locks, databases and free space…', flush=True)
        preflight()
        print('Checking available updates…', flush=True)
        plan = update_plan()
        if TERMINAL_LOG:
            with open(TERMINAL_LOG, 'a') as log:
                log.write('Reviewed plan candidates (final pacman transaction may differ):\n' + json.dumps(plan, indent=2) + '\n')
        print(json.dumps(plan, indent=2), flush=True)
        print('Read https://archlinux.org/news/ before approval. Pacman presents its final refreshed transaction. Reboot after kernel/core updates. No AUR or Flatpak changes are chained.', flush=True)
        provider = request.get('snapshot', 'none')
        if provider not in {'none', 'snapper', 'timeshift'}:
            raise OperationError('Invalid snapshot provider')
        if provider != 'none':
            if snapshot_context()[1] != provider:
                raise OperationError('Selected snapshot provider is not eligible or conflicts with another configuration')
            dispatch({'action': 'snapshot'}, interactive=True)
        preflight()  # Recheck after review/snapshot; pacman itself owns the final lock.
        print('Starting full system update. Enter your sudo password if requested…', flush=True)
        run(['sudo', 'pacman', '-Syu'], interactive=True)
        return 'Official full upgrade completed. Reboot may be required; review pacman.log.'
    if action == 'snapshot':
        _, backend = snapshot_context()
        if backend == 'snapper':
            return run(['sudo', 'snapper', '-c', 'root', 'create', '--type', 'single', '--description', 'ArchEasy manual snapshot'], interactive=True)
        if backend == 'timeshift':
            return run(['sudo', 'timeshift', '--create', '--comments', 'ArchEasy manual snapshot', '--tags', 'O'], interactive=True)
        raise OperationError('No unambiguous compatible snapshot provider; inspect Backup & Restore first')
    if action == 'flatpak':
        for scope in ['--user', '--system']:
            run(['flatpak', 'update', scope], interactive=True)
        return 'Flatpak operations completed'
    if action in {'restic-check', 'restic-export', 'restic-restore', 'restic-list'}:
        repository = destination(request.get('repository'), must_exist=True)
        # Do not inherit plaintext password or arbitrary shell password commands.
        for name in ['RESTIC_PASSWORD', 'RESTIC_PASSWORD_FILE', 'RESTIC_PASSWORD_COMMAND', 'RESTIC_REPOSITORY', 'RESTIC_REPOSITORY_FILE']:
            os.environ.pop(name, None)
        args = ['restic', '--repo', repository]
        if action == 'restic-check':
            args += ['check', '--read-data']
        elif action == 'restic-list':
            args += ['snapshots']
        elif action == 'restic-export':
            source = Path(request.get('source', '')).resolve(strict=True)
            home = Path.home().resolve()
            if source == home or not source.is_relative_to(home) or not source.is_dir():
                raise OperationError('Select a dedicated source folder inside your home; it may contain secrets')
            repo = Path(repository).resolve()
            if source == repo or repo.is_relative_to(source) or source.is_relative_to(repo):
                raise OperationError('Backup source and repository overlap')
            args += ['backup', '--', str(source)]
        else:
            snapshot = request.get('snapshot', '')
            if not re.fullmatch(r'[a-f0-9]{8,64}', snapshot):
                raise OperationError('Select a hexadecimal Restic snapshot ID')
            target = destination(request.get('destination'))
            # Create exclusively before restic so a pre-existing target cannot be overwritten.
            os.mkdir(target, 0o700)
            args += ['restore', snapshot, '--target', target, '--verify']
        return run(args, interactive=True)
    raise OperationError('Unknown management operation')


def execute(request, interactive=False):
    global TERMINAL_LOG
    if request.get('action') == 'history':
        return history()
    directory = state_dir()
    with open(directory / 'operation.lock', 'a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise OperationError('Another management operation is running') from error
        path = directory / f'{time.time_ns()}.json'
        record = {'action': request.get('action'), 'started': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  'status': 'running; if left in this state, completion is unknown after interruption'}
        if interactive:
            TERMINAL_LOG = path.with_suffix('.log')
            fd = os.open(TERMINAL_LOG, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            os.close(fd)
            record['terminal_log'] = str(TERMINAL_LOG)
            print('Persistent tool output:', TERMINAL_LOG, flush=True)
        record_write(path, record)
        try:
            result = dispatch(request, interactive)
            record.update(status='completed', output=result[:131072])
            return result or 'Completed; review terminal output.'
        except Exception as error:
            record.update(status='failed; transaction completion must be inspected', error=str(error),
                          recovery='Review this operation and pacman.log. Resolve locks/network/storage errors, recheck updates and approve a new full transaction. Do not blindly downgrade or delete locks.')
            raise
        finally:
            record_write(path, record)


if __name__ == '__main__':
    terminal = False
    exit_code = 0
    try:
        terminal = len(sys.argv) == 3 and sys.argv[1] == '--terminal'
        request = json.loads(sys.argv[2]) if terminal else json.load(sys.stdin)
        if terminal:
            print('Review operation and paths:', json.dumps(request, indent=2), flush=True)
            print('Atomic operations cannot be safely cancelled. Passwords are prompted by the tool and are never saved by ArchEasy.', flush=True)
            if request['action'] not in {'restic-check', 'restic-list', 'official'} and input('Type YES to proceed: ') != 'YES':
                raise OperationError('Cancelled before operation')
        print(execute(request, interactive=terminal))
    except Exception as error:
        print(f'Operation failed: {error}', file=sys.stderr)
        exit_code = 1
    finally:
        if terminal and sys.stdin.isatty():
            try:
                input('Press Enter to close this session. Persistent results are available in View Logs…')
            except (EOFError, KeyboardInterrupt):
                pass
    sys.exit(exit_code)
