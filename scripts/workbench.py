"""Integrated workbench services. Loaded with ArchEasy's management runtime.
No bootloader, disk, firewall, SSH or firmware mutation is implemented here.
"""
from pathlib import Path
import json
import os
import re
import shutil
import sys
import time
import contextlib
import fcntl
import subprocess
import base64
import hashlib
import platform
import stat
import management as core

BASE_RUN = core.run
CANCEL_PATH = None
class Cancellation:
    def is_set(self):
        return CANCEL_PATH is not None and CANCEL_PATH.exists()

def run(args, **kwargs):
    if not kwargs.get('interactive'):
        kwargs.setdefault('cancel', Cancellation())
    return BASE_RUN(args, **kwargs)

core.run = run
optional = core.optional
Error = core.OperationError
ORIGINAL_DISPATCH = core.dispatch
CREATOR = {
    'Video Studio': ['obs-studio', 'kdenlive', 'shotcut'],
    'Graphic Design Suite': ['gimp', 'krita', 'inkscape'],
    'Audio Studio': ['audacity', 'ardour'],
    '3D Studio': ['blender'],
    'Photography': ['darktable', 'rawtherapee'],
    'Creator Essentials': ['obs-studio', 'kdenlive', 'blender', 'gimp', 'audacity'],
}
ALL_CREATOR = sorted({package for group in CREATOR.values() for package in group})


def read(path, default=''):
    try:
        return Path(path).read_text(errors='replace')[:131072].strip()
    except OSError:
        return default


def number(path):
    try:
        return float(read(path))
    except ValueError:
        return None


def memory(text):
    values = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 2:
            try:
                values[parts[0].rstrip(':')] = int(parts[1]) * 1024
            except ValueError:
                pass
    total, available = values.get('MemTotal'), values.get('MemAvailable')
    swap = values.get('SwapTotal')
    return {'total': total, 'available': available,
            'used_percent': (100 * (total - available) / total) if total and available is not None else None,
            'swap_total': swap, 'swap_used': swap - values.get('SwapFree', swap) if swap is not None else None}


def cpu_ticks(text):
    line = next((line for line in text.splitlines() if line.startswith('cpu ')), '')
    try:
        fields = [int(value) for value in line.split()[1:9]]
        if len(fields) < 4 or any(value < 0 for value in fields):
            return None
        return {'total': sum(fields), 'idle': fields[3] + (fields[4] if len(fields) > 4 else 0)}
    except ValueError:
        return None


def net_counters(text):
    counters = {}
    for line in text.splitlines():
        if ':' not in line:
            continue
        name, values = line.split(':', 1)
        fields = values.split()
        try:
            if len(fields) >= 16 and name.strip() != 'lo':
                counters[name.strip()] = {'rx': int(fields[0]), 'tx': int(fields[8])}
        except ValueError:
            pass
    return counters


def sensor_values(root=Path('/sys/class/hwmon')):
    values = []
    for device in sorted(root.glob('hwmon*'))[:32]:
        for sensor in sorted(device.glob('temp*_input'))[:16]:
            value = number(sensor)
            if value is not None:
                values.append({'sensor': read(device / 'name', device.name) + '/' + read(sensor.with_name(sensor.name.replace('_input', '_label')), sensor.stem), 'celsius': value / 1000})
    return values


def health_sample():
    graphics = []
    for device in sorted(Path('/sys/class/drm').glob('card[0-9]*/device'))[:8]:
        graphics.append({'device': str(device.parent.name), 'vendor': read(device / 'vendor', 'unavailable'),
                         'model': read(device / 'product_name', read(device / 'label', 'PCI device ' + read(device / 'device', 'unavailable'))),
                         'utilisation': number(device / 'gpu_busy_percent'),
                         'vram_used': number(device / 'mem_info_vram_used'),
                         'vram_total': number(device / 'mem_info_vram_total')})
    space = shutil.disk_usage('/')
    return {'time': time.time(), 'cpu_ticks': cpu_ticks(read('/proc/stat')),
            'cpu_model': next((line.split(':', 1)[1].strip() for line in read('/proc/cpuinfo').splitlines() if line.startswith('model name')), platform.machine()),
            'frequency_khz': number('/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq'),
            'memory': memory(read('/proc/meminfo')), 'network': net_counters(read('/proc/net/dev')),
            'uptime_seconds': read('/proc/uptime').split(' ')[0], 'kernel': platform.release(),
            'storage': {'total': space.total, 'free': space.free, 'used_percent': space.used / space.total * 100},
            'sensors': sensor_values(), 'gpu': graphics}


def service_status(names):
    return {name: optional(['systemctl', 'show', '--property=ActiveState', '--value', name]) for name in names}


def health_report():
    sample = health_sample()
    warnings = []
    if sample['storage']['free'] < 2 * 1024**3:
        warnings.append('Low root storage: less than 2 GiB free. Inspect usage before installing packages.')
    pressure = sample['memory']['used_percent']
    if pressure is not None and pressure > 90:
        warnings.append('High memory usage; inspect memory pressure and processes. Cache use alone does not indicate a fault.')
    if not sample['sensors']:
        warnings.append('No readable hwmon temperature sensors. Missing metrics are unavailable, not zero.')
    return 'GPU identity / active drivers:\n' + optional(['lspci', '-nnk']) + '\n\n' + json.dumps(sample, indent=2) + '\n\n' + '\n'.join(warnings) + '\n\nFailed services:\n' + optional(['systemctl', '--failed', '--no-pager', '--plain']) + '\n\nMemory pressure:\n' + read('/proc/pressure/memory', 'Unavailable') + '\n\nRecent warning/error history (review repeated service failures):\n' + optional(['journalctl', '-b', '-p', 'warning', '-n', '60', '--no-pager']) + '\n\nTemperature limits are device-specific. No universal safety threshold is assumed. SMART/filesystem checks are in Disk Manager.'


def desktop_detect(env=None, sessions=Path('/usr/share/xsessions')):
    env = os.environ if env is None else env
    active = env.get('XDG_CURRENT_DESKTOP', '') or env.get('XDG_SESSION_DESKTOP', '')
    installed = sorted({path.stem for folder in [sessions, sessions.parent / 'wayland-sessions'] for path in folder.glob('*.desktop')})
    lowered = active.lower()
    known = [name for name in ['kde', 'gnome', 'xfce', 'cinnamon', 'hyprland'] if name in lowered]
    return {'active': active or 'Unavailable', 'detected': known, 'installed_sessions': installed,
            'session_type': env.get('XDG_SESSION_TYPE', 'Unavailable')}


DESKTOP_COMMANDS = {'kde': ['systemsettings'], 'gnome': ['gnome-control-center'], 'xfce': ['xfce4-settings-manager'], 'cinnamon': ['cinnamon-settings']}
APPEARANCE_PRESETS = {'Minimal': 'default', 'Modern Dark': 'prefer-dark', 'Classic Desktop': 'prefer-light', 'Developer Workstation': 'prefer-dark', 'Gaming Desktop': 'prefer-dark'}


def desktop_report():
    result = desktop_detect()
    result['theme_locations'] = {str(path): sorted(child.name for child in path.iterdir())[:100] if path.is_dir() else [] for path in [Path.home() / '.themes', Path('/usr/share/themes'), Path.home() / '.icons', Path('/usr/share/icons')]}
    return json.dumps(result, indent=2) + '\n\nGNOME-only presets change the supported color-scheme preference, not your desktop layout. Other desktops use their native settings for themes, icons, cursors, fonts, wallpaper and Qt/GTK compatibility. Hyprland presets remain in Configs with preview and rollback. Installed sessions are not proof they are active. Never replace a desktop or display manager for appearance.'


def gnome_key():
    if desktop_detect()['detected'] != ['gnome']:
        raise Error('Appearance preset requires an unambiguous active GNOME desktop; use native settings otherwise')
    keys = run(['gsettings', 'list-keys', 'org.gnome.desktop.interface']).splitlines()
    if 'color-scheme' not in keys:
        raise Error('This GNOME schema does not support color-scheme')
    return ['gsettings', 'org.gnome.desktop.interface', 'color-scheme']


def appearance_change(preset=None, restore=False):
    _, schema, key = gnome_key()
    directory = core.state_dir() / 'appearance'
    directory.mkdir(exist_ok=True, mode=0o700)
    if restore:
        records = sorted(directory.glob('*.json'))
        if not records:
            raise Error('No appearance change to restore')
        path = records[-1]
        record = json.loads(path.read_text())
        if record.get('restored'):
            raise Error('The latest appearance change has already been restored')
        if run(['gsettings', 'get', schema, key]) != record['after']:
            raise Error('Appearance was changed outside ArchEasy. Refusing to overwrite the newer setting.')
        value = record['before']
        if value not in {"'default'", "'prefer-dark'", "'prefer-light'"}:
            raise Error('Unsupported saved preference')
        run(['gsettings', 'set', schema, key, value])
        record['restored'] = True
    else:
        if preset not in APPEARANCE_PRESETS:
            raise Error('Unknown preset')
        value = "'" + APPEARANCE_PRESETS[preset] + "'"
        record = {'before': run(['gsettings', 'get', schema, key]), 'after': value, 'preset': preset}
        path = directory / f'{time.time_ns()}.json'
        core.record_write(path, record)  # Before-state survives a failed apply.
        run(['gsettings', 'set', schema, key, value])
    core.record_write(path, record)
    return 'Appearance preference changed. Applications may need reopening. No dotfiles were replaced.'


def bootloader_evidence(efi, grub, loader, bootctl=''):
    evidence = []
    if grub:
        evidence.append('GRUB files present; this alone does not prove GRUB is the active bootloader')
    if loader:
        evidence.append('systemd-boot loader files present; this alone does not prove it is active')
    if not efi:
        evidence.append('EFI runtime unavailable: legacy boot or an environment without EFI variables')
    if not evidence:
        evidence.append('Bootloader unknown; do not assume GRUB')
    return '\n'.join(evidence) + '\nRuntime boot evidence:\n' + bootctl


RESCUE_GUIDE = '''Recovery preview — no commands below are executed by ArchEasy.
Record target devices, UUIDs, encryption, RAID/LVM and Btrfs subvolumes with lsblk/findmnt before making changes. Confirm the actual bootloader from runtime evidence; files alone are not proof. Preserve EFI and boot files and verify multi-boot ownership.
Boot failure: use a compatible Arch live ISO, unlock encryption and activate the correct volumes using their own documentation, mount the intended root plus its boot/EFI partitions, and inspect journal/fstab/package evidence. Never format an EFI partition to troubleshoot it.
Initramfs: confirm installed kernel packages, matching modules, actual mounted boot location, available space and initramfs generator (mkinitcpio or dracut) before considering a rebuild in the correct root/chroot. No generic rebuild is safe for every layout.
fstab: compare UUID references with blkid/lsblk. Back up fstab and validate proposed edits before reboot; network and removable mounts may be intentionally absent. Do not automatically remove an unresolved entry.
Package recovery: verify no package process owns the lock, read pacman.log, check database consistency and Arch news. Recheck and approve a full upgrade; never delete another process's lock or blindly downgrade.
Snapshot recovery: use Backup & Restore's offline guide. Boot/EFI and nested subvolumes may require separate recovery. A successful command is not proof the system will boot.
Prepare recovery media and an external verified backup before any repair. Actual bootloader installation, EFI edits, disk repair and root rollback remain manual, with exact target and consequences reviewed in the recovery environment.'''


def rescue():
    evidence = bootloader_evidence(Path('/sys/firmware/efi').exists(), Path('/boot/grub/grub.cfg').exists(), Path('/boot/loader/loader.conf').exists(), optional(['bootctl', 'status', '--no-pager']))
    return '\n\n'.join([evidence, 'EFI entries:\n' + optional(['efibootmgr', '-v']),
        'Mount layout:\n' + optional(['findmnt', '--real']), 'Device layers / UUIDs:\n' + optional(['lsblk', '-o', 'NAME,TYPE,FSTYPE,UUID,MOUNTPOINTS']),
        'fstab verification (read-only):\n' + optional(['findmnt', '--verify', '--verbose']),
        'fstab (inline secrets redacted):\n' + re.sub(r'(?i)(password|passwd|token|secret)=([^,\s]+)', r'\1=[redacted]', read('/etc/fstab', 'Unavailable')), 'Kernel packages:\n' + optional(['pacman', '-Q', 'linux', 'linux-lts', 'linux-zen', 'linux-hardened']),
        'Boot files (not proof of validity):\n' + '\n'.join(str(path) for pattern in ['vmlinuz*', 'initramfs*', 'EFI/Linux/*.efi'] for path in Path('/boot').glob(pattern)),
        'Database consistency:\n' + optional(['pacman', '-Dk']), 'Failed units:\n' + optional(['systemctl', '--failed', '--no-pager']),
        'Boot errors:\n' + optional(['journalctl', '-b', '-p', 'err', '-n', '100', '--no-pager']), RESCUE_GUIDE])


def disk_rows(text):
    data = json.loads(text)
    result = []
    def visit(items, depth=0):
        if depth > 16 or not isinstance(items, list):
            raise Error('Malformed block device hierarchy')
        for item in items:
            if not isinstance(item, dict) or not isinstance(item.get('name'), str):
                raise Error('Malformed block device entry')
            result.append({key: item.get(key) for key in ['name', 'type', 'size', 'model', 'rota', 'fstype', 'mountpoints', 'fsavail', 'fsuse%'] } | {'depth': depth})
            visit(item.get('children', []), depth + 1)
    visit(data.get('blockdevices'))
    return result


def disk_report():
    listing = run(['lsblk', '--json', '--bytes', '-o', 'NAME,TYPE,SIZE,MODEL,ROTA,FSTYPE,MOUNTPOINTS,FSAVAIL,FSUSE%'])
    try:
        btrfs = optional(['btrfs', 'subvolume', 'list', '/']) if core.filesystem().get('fstype') == 'btrfs' else 'Not a Btrfs root'
    except Exception as error:
        btrfs = 'Root filesystem detection unavailable: ' + str(error)
    return json.dumps(disk_rows(listing), indent=2) + '\n\nMounted usage:\n' + optional(['df', '-hT']) + '\n\nTRIM status:\n' + optional(['systemctl', 'status', 'fstrim.timer', '--no-pager']) + '\n\nBtrfs subvolumes (only if root is Btrfs):\n' + btrfs + '\n\nLUKS/crypt, LVM and RAID layers are reported by lsblk; encrypted unmounted devices cannot be inspected. No fsck/repair is run on mounted filesystems. No partition or encryption changes are offered.'


def disk_maintenance():
    return '\n\n'.join(['Package cache:\n' + optional(['du', '-sh', '/var/cache/pacman/pkg']),
        'Journal storage:\n' + optional(['journalctl', '--disk-usage']), 'Orphan review:\n' + optional(['pacman', '-Qdt']),
        'Temporary file policy (inspection only):\n' + optional(['systemd-tmpfiles', '--cat-config']),
        'Use System utilities for selected orphan/cache cleanup with confirmation. No user files or old logs are deleted here. Use a filesystem-specific offline tool for repair; a mounted usage scan is not an integrity check.'])


def smart(device):
    # A strict block-device path, not an arbitrary smartctl option or shell fragment.
    if not isinstance(device, str) or not re.fullmatch(r'/dev/(?:sd[a-z]+|nvme[0-9]+n[0-9]+|mmcblk[0-9]+)', device):
        raise Error('Choose a physical /dev/sdX, /dev/nvmeNnN or /dev/mmcblkN device from the disk list')
    if not stat.S_ISBLK(os.stat(device).st_mode):
        raise Error('Not a block device')
    return optional(['smartctl', '-a', device]) + '\nSMART availability/status depends on device/controller and permissions. No self-test or repair is started.'


def network_report():
    return '\n\n'.join(['Interfaces and addresses:\n' + optional(['ip', '-json', 'address']), 'Default routes:\n' + optional(['ip', '-json', 'route']),
        'DNS status:\n' + optional(['resolvectl', 'status']), 'Resolver file:\n' + read('/etc/resolv.conf', 'Unavailable'),
        'NetworkManager (optional):\n' + optional(['nmcli', '-t', '-f', 'DEVICE,TYPE,STATE,CONNECTION', 'device']),
        'Wi-Fi link evidence:\n' + optional(['iw', 'dev']),
        'Network services:\n' + json.dumps(service_status(['NetworkManager', 'systemd-networkd', 'systemd-resolved', 'iwd']), indent=2),
        'Troubleshooting: compare link/address/default route/DNS evidence first. NetworkManager is optional. Do not restart networking over an active remote administration connection.'])


def firewall_state(firewalld, ufw):
    if firewalld and ufw:
        return 'Both firewalld and UFW appear active: inspect ownership before any change; do not stack managers.'
    return 'Active manager: ' + ('firewalld' if firewalld else 'UFW' if ufw else 'unknown/none detected; inspect nftables too')


def security_report():
    firewalld = optional(['firewall-cmd', '--state'])
    ufw = optional(['ufw', 'status'])
    info = [firewall_state(firewalld == 'running', bool(re.search(r'^Status: active$', ufw, re.M))),
        'firewalld:\n' + firewalld, 'UFW:\n' + ufw,
        'Firewall zones:\n' + optional(['firewall-cmd', '--list-all-zones']),
        'nftables rules (may require privileges):\n' + optional(['nft', 'list', 'ruleset']),
        'SSH service:\n' + optional(['systemctl', 'status', 'sshd', '--no-pager']),
        'Listening sockets (exposure depends on firewall/router):\n' + optional(['ss', '-lntu']),
        'SSH effective configuration (validation only):\n' + optional(['sshd', '-t']),
        'Public SSH key fingerprints (no private keys opened):']
    directory = Path.home() / '.ssh'
    if directory.is_symlink():
        info.append('Linked .ssh directory skipped')
    else:
        for path in sorted(directory.glob('*.pub'))[:32]:
            if path.is_file() and not path.is_symlink():
                info.append(optional(['ssh-keygen', '-lf', str(path)]))
        if directory.exists() and directory.stat().st_mode & 0o077:
            info.append('Permission warning: ~/.ssh is accessible to other users; review ownership and access before changing modes.')
    info += ['WireGuard interfaces (no configuration/private keys):\n' + optional(['wg', 'show', 'interfaces']),
        'No firewall rules, SSH settings/services or VPN credentials are changed. Never share private keys or wg showconf output. Security updates are reviewed in System Updates; these checks do not prove complete security. Firewall edits can disconnect SSH; plan console access before using the native manager.']
    return '\n\n'.join(info)


def battery_info(directory):
    values = {name: read(directory / name, 'Unavailable') for name in ['status', 'capacity', 'cycle_count', 'energy_now', 'energy_full', 'energy_full_design', 'charge_now', 'charge_full', 'charge_full_design', 'charge_control_start_threshold', 'charge_control_end_threshold']}
    for unit in ['energy', 'charge']:
        full, design = number(directory / f'{unit}_full'), number(directory / f'{unit}_full_design')
        if full is not None and design and design > 0:
            values['health_percent'] = round(full / design * 100, 1)
            break
    return values


def laptop_detect(root=Path('/sys/class/power_supply')):
    return [path for path in sorted(root.glob('*')) if read(path / 'type') == 'Battery']


def laptop_report():
    batteries = laptop_detect()
    services = service_status(['power-profiles-daemon', 'tlp', 'tuned', 'auto-cpufreq'])
    return json.dumps({'laptop_detected': bool(batteries), 'batteries': {path.name: battery_info(path) for path in batteries}, 'services': services,
        'energy_preferences': {str(path): read(path) for path in list(Path('/sys/devices/system/cpu').glob('cpu*/cpufreq/energy_performance_preference'))[:16]}}, indent=2) + '\n\nPower profiles:\n' + optional(['powerprofilesctl', 'list']) + '\n\nBacklights:\n' + optional(['brightnessctl', '-l']) + '\n\nSuspend states:\n' + read('/sys/power/state', 'Unavailable') + '\nSwap:\n' + read('/proc/swaps', 'Unavailable') + '\nResume device:\n' + read('/sys/power/resume', 'Unavailable') + '\n\nlogind settings:\n' + optional(['systemd-analyze', 'cat-config', 'systemd/logind.conf']) + '\n\nLid/suspend guide: configure the active desktop power settings first, as desktops may inhibit logind handling. Review AC/battery and docked behaviour there. Do not edit both desktop and logind blindly. Hibernate is not validated merely by the presence of swap: kernel support, resume device/offset, encryption and initramfs must agree. Lid actions, touchpad, Bluetooth power and charge thresholds use supported native tools; no hardware/firmware writes are made by this page.'


@contextlib.contextmanager
def package_lock():
    directory = Path(os.environ.get('XDG_RUNTIME_DIR', str(Path(os.environ.get('XDG_CACHE_HOME', str(Path.home() / '.cache'))) / 'archeasy')))
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    with contextlib.ExitStack() as stack:
        for name in ['archeasy-packages.lock', 'archeasy-update.lock']:
            stream = stack.enter_context(open(directory / name, 'a'))
            try:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise Error('Another ArchEasy package operation is active; wait for it to finish') from error
        yield


def native_settings():
    desktops = desktop_detect()['detected']
    if len(desktops) != 1 or desktops[0] not in DESKTOP_COMMANDS:
        raise Error('No supported unambiguous native settings panel; use Configs or the desktop settings manually')
    command = DESKTOP_COMMANDS[desktops[0]]
    if not shutil.which(command[0]):
        raise Error('Native settings utility is not installed')
    subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    return 'Opened native settings. Review changes there; ArchEasy has changed no settings.'


def creator_selection(request):
    selected = request.get('packages', [])
    if not isinstance(selected, list) or not selected or len(selected) > len(ALL_CREATOR) or any(package not in ALL_CREATOR for package in selected):
        raise Error('Select known Creator applications only')
    return sorted(set(selected))


def creator_plan(request):
    selected = creator_selection(request)
    info = []
    for package in selected:
        # -Si checks configured repository availability; never executes third-party metadata.
        metadata = run(['pacman', '-Si', '--', package])
        info.append({'package': package, 'source': 'Official configured repositories',
                     'installed': optional(['pacman', '-Q', '--', package]), 'metadata': metadata})
    return info


EXCLUDED_NAMES = {'.ssh', '.gnupg', '.aws', '.kube', '.mozilla', '.netrc', '.env', '.npmrc', '.git-credentials', 'keyrings', 'kwalletd', 'Cookies', 'Login Data', 'rclone', 'chromium', 'google-chrome', 'BraveSoftware', 'firefox'}
HARDWARE_CONFIGS = {'hyprland.conf', 'hyprland.lua', 'monitors.conf', 'xorg.conf', 'fstab', 'mkinitcpio.conf'}


def safe_relative(name):
    if not isinstance(name, str) or not name or len(name) > 4096 or '\\' in name or '\x00' in name or any(part in {'', '.', '..'} for part in name.split('/')):
        raise Error('Unsafe migration path')
    path = Path(name)
    if path.is_absolute() or not (path.parts[0] == '.config' or path.parts[0] in {'.bashrc', '.zshrc', '.profile', '.gitconfig', '.tmux.conf'}):
        raise Error('Only selected user configuration paths are supported')
    if any(part in EXCLUDED_NAMES or part.lower().endswith(('.kdbx', '.pem', '.key')) for part in path.parts):
        raise Error('Credential/browser/key-store paths are excluded')
    if path.name in HARDWARE_CONFIGS:
        raise Error('Hardware/system-specific configuration requires separate manual migration')
    return path


def has_secret(data):
    text = data.decode('utf-8', 'strict')
    return bool(re.search(r'-----BEGIN .*PRIVATE KEY|(?:ghp_|github_pat_|xox[bp]-|AKIA)[A-Za-z0-9_-]{12,}|(?:password|passwd|token|api[_-]?key|secret|private[_-]?key)\s*[=:]\s*\S+|https?://[^\s/:]+:[^\s/@]+@', text, re.I))


def home_path(relative, home=None):
    home = Path.home() if home is None else home
    path = home / safe_relative(relative)
    for parent in [path, *path.parents]:
        if parent == home:
            break
        if parent.is_symlink():
            raise Error('Symlinked migration paths are refused')
    if path.exists() and (not path.is_file() or path.stat().st_uid != os.getuid()):
        raise Error('Only regular files owned by the current user are supported')
    return path


def compatibility():
    return {'distribution': read('/etc/os-release'), 'kernel': platform.release(), 'architecture': platform.machine(),
            'desktop': desktop_detect(), 'gpu': optional(['lspci', '-nn'])}


def manifest_packages(args):
    text = optional(args)
    return [] if text.startswith('Unavailable:') else text.splitlines()


def manifest_export(request):
    paths = request.get('paths', [])
    if not isinstance(paths, list) or not paths or len(paths) > 100:
        raise Error('Explicitly select 1–100 individual configuration files, not folders')
    entries = []
    total = 0
    for name in sorted(set(paths)):
        path = home_path(name)
        if path.stat().st_size > 1024**2:
            raise Error('A selected config exceeds 1 MiB')
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, 'rb') as stream:
            opened = os.fstat(stream.fileno())
            if not stat.S_ISREG(opened.st_mode) or opened.st_uid != os.getuid():
                raise Error('Selected configuration changed or is not owned by this user')
            data = stream.read(1024**2 + 1)
            after = os.fstat(stream.fileno())
            if (opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
                raise Error('Selected configuration changed during export')
        total += len(data)
        if len(data) > 1024**2 or total > 8 * 1024**2:
            raise Error('Migration content exceeds size limit')
        if has_secret(data):
            raise Error('Possible secret in a selected file; review locally. Export refused.')
        entries.append({'path': name, 'sha256': hashlib.sha256(data).hexdigest(), 'content': base64.b64encode(data).decode(), 'mode': 0o600})
    manifest = {'format': 'archeasy-migration-v1', 'source': compatibility(), 'created': time.time(), 'files': entries,
                'packages': {'official': manifest_packages(['pacman', '-Qen']), 'foreign_optional': manifest_packages(['pacman', '-Qem']) if request.get('foreign') is True else [], 'flatpak': manifest_packages(['flatpak', 'list', '--app', '--columns=application,version,origin,installation'])}}
    target = core.destination(request.get('destination'))
    fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'w') as stream:
        json.dump(manifest, stream, indent=2)
    return f'Saved private manifest: {target}. Review before sharing. Secret detection is heuristic, not a guarantee.'


def load_manifest(value):
    path = Path(value)
    if not path.is_file() or path.stat().st_size > 16 * 1024**2:
        raise Error('Choose a migration JSON manifest no larger than 16 MiB')
    manifest = json.loads(path.read_text())
    if manifest.get('format') != 'archeasy-migration-v1' or not isinstance(manifest.get('files'), list) or len(manifest['files']) > 100:
        raise Error('Unsupported migration manifest')
    if not isinstance(manifest.get('source', {}), dict) or not isinstance(manifest.get('packages', {}), dict):
        raise Error('Malformed migration metadata')
    for kind, packages in manifest.get('packages', {}).items():
        if kind not in {'official', 'foreign_optional', 'flatpak'} or not isinstance(packages, list) or len(packages) > 20000:
            raise Error('Malformed package manifest')
        for package in packages:
            if not isinstance(package, str) or len(package) > 512 or not re.fullmatch(r'[a-zA-Z0-9@._+:/~() \t-]+', package):
                raise Error('Invalid package manifest entry')
    seen = set()
    for entry in manifest['files']:
        name = entry.get('path')
        safe_relative(name)
        if name in seen:
            raise Error('Duplicate migration path')
        seen.add(name)
        data = base64.b64decode(entry.get('content', ''), validate=True)
        if len(data) > 1024**2 or hashlib.sha256(data).hexdigest() != entry.get('sha256') or has_secret(data):
            raise Error('Invalid checksum, size or possible secret in manifest')
    return manifest


def migration_preview(request):
    manifest = load_manifest(request.get('source', ''))
    changes = []
    for entry in manifest['files']:
        path = home_path(entry['path'])
        changes.append({'path': entry['path'], 'state': 'conflict: existing file (restore stages separately)' if path.exists() else 'new file (restore stages separately)'})
    # Do not display config contents, credentials or imported command text.
    return json.dumps({'source_architecture': manifest.get('source', {}).get('architecture'), 'target_architecture': platform.machine(),
        'source_kernel': manifest.get('source', {}).get('kernel'), 'source_gpu_evidence': manifest.get('source', {}).get('gpu'),
        'target_kernel': platform.release(), 'source_desktop': manifest.get('source', {}).get('desktop'), 'target_desktop': desktop_detect(), 'changes': changes,
        'packages': manifest.get('packages', {}), 'guidance': 'Different kernels, GPU, desktop or package availability require manual review. No packages or scripts from the manifest are executed. Restore stages selected files in a new directory; current dotfiles stay untouched.'}, indent=2)


def migration_stage(request):
    manifest = load_manifest(request.get('source', ''))
    selected = request.get('paths')
    if not isinstance(selected, list) or not selected or not set(selected).issubset({entry['path'] for entry in manifest['files']}):
        raise Error('Select exact file paths from Preview Changes')
    target = Path(core.destination(request.get('destination')))
    target.mkdir(mode=0o700)
    for entry in manifest['files']:
        if entry['path'] not in selected:
            continue
        path = target / safe_relative(entry['path'])
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, 'wb') as stream:
            stream.write(base64.b64decode(entry['content'], validate=True))
    return f'Selected files staged in {target}. Review them as untrusted code before copying. Existing files were not overwritten; no imported scripts were executed.'


def creator_install(request):
    core.preflight()
    plan = creator_plan(request)
    print(json.dumps(plan, indent=2), flush=True)
    if core.TERMINAL_LOG:
        with open(core.TERMINAL_LOG, 'a') as log:
            log.write('Creator plan (final transaction can differ):\n' + json.dumps(plan, indent=2) + '\n')
    if input('Full system upgrade and selected Creator packages. Type INSTALL to approve: ') != 'INSTALL':
        raise Error('Cancelled before transaction')
    core.preflight()
    run(['sudo', 'pacman', '-Syu', '--needed', '--', *creator_selection(request)], interactive=True)
    return 'Creator package transaction completed. Review logs; a system upgrade can require a reboot.'


def dispatch(request, interactive=False):
    action = request.get('action')
    diagnostics = {'health': health_report, 'sample': lambda: json.dumps(health_sample()),
        'processes': lambda: optional(['ps', '-eo', 'pid,comm,%cpu,%mem', '--sort=-%mem']),
        'services': lambda: optional(['systemctl', '--failed', '--no-pager', '--plain']),
        'desktop': desktop_report, 'native-settings': native_settings, 'rescue': rescue, 'rescue-guide': lambda: RESCUE_GUIDE,
        'disks': disk_report, 'disk-layout': lambda: json.dumps(disk_rows(run(['lsblk', '--json', '--bytes', '-o', 'NAME,TYPE,SIZE,MODEL,ROTA,FSTYPE,MOUNTPOINTS,FSAVAIL,FSUSE%']))), 'disk-maintenance': disk_maintenance, 'network': network_report, 'security': security_report,
        'connectivity': lambda: 'DNS lookup:\n' + optional(['getent', 'ahosts', 'archlinux.org']) + '\nHTTPS:\n' + optional(['curl', '--head', '--fail', '--max-time', '10', '--proto', '=https', 'https://archlinux.org/']),
        'laptop': laptop_report, 'creator-plan': lambda: json.dumps(creator_plan(request), indent=2),
        'migration-preview': lambda: migration_preview(request), 'smart': lambda: smart(request.get('device'))}
    if action in diagnostics:
        result = diagnostics[action]()
        if Cancellation().is_set():
            raise Error('Diagnostic cancelled; no system settings were changed')
        return result
    if action in {'official', 'flatpak'} and interactive:
        with package_lock():
            return ORIGINAL_DISPATCH(request, interactive)
    if action not in {'software-flatpak-install', 'software-flatpak-remove', 'software-install', 'software-remove', 'creator-install', 'appearance', 'appearance-restore', 'power-profile', 'brightness', 'migration-export', 'migration-stage'}:
        result = ORIGINAL_DISPATCH(request, interactive)
        if not interactive and Cancellation().is_set():
            raise Error('Diagnostic cancelled')
        return result
    if not interactive or os.geteuid() == 0:
        raise Error('Requires approved terminal operation as your normal user')
    if action in {'software-flatpak-install', 'software-flatpak-remove'}:
        package = request.get('package', '')
        if not isinstance(package, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]+', package):
            raise Error('Invalid Flatpak application ID')
        with package_lock():
            if action == 'software-flatpak-install':
                remote = request.get('remote', '')
                if not isinstance(remote, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', remote):
                    raise Error('Invalid Flatpak remote')
                if shutil.disk_usage(Path.home()).free < 2 * 1024**3:
                    raise Error('Less than 2 GiB free in home; inspect storage before installing')
                run(['flatpak', 'install', '--user', '--', remote, package], interactive=True)
            else:
                scope = request.get('scope')
                if scope not in {'--user', '--system'}:
                    raise Error('Invalid Flatpak installation scope')
                run(['flatpak', 'uninstall', scope, '--', package], interactive=True)
        return 'Flatpak operation completed. Review the recorded tool output.'
    if action in {'software-install', 'software-remove'}:
        package = core.package_id(request.get('package'))
        with package_lock():
            core.preflight()
            if action == 'software-install':
                print(run(['pacman', '-Si', '--', package]), flush=True)
                if input('Full system upgrade plus selected package. Type INSTALL to approve: ') != 'INSTALL':
                    raise Error('Cancelled before transaction')
                core.preflight()
                run(['sudo', 'pacman', '-Syu', '--needed', '--', package], interactive=True)
            else:
                print(run(['pacman', '-Qi', '--', package]), flush=True)
                if input('Removal will be reviewed by pacman. Type REMOVE to approve: ') != 'REMOVE':
                    raise Error('Cancelled before removal')
                core.preflight()
                # Recursive removal is an explicit checkbox choice in Installed.
                mode = '-Rns' if request.get('clean') is True else '-R'
                run(['sudo', 'pacman', mode, '--', package], interactive=True)
        return 'Package operation completed; review pacman.log for the actual transaction.'
    if action == 'creator-install':
        with package_lock():
            return creator_install(request)
    if action in {'appearance', 'appearance-restore'}:
        return appearance_change(request.get('preset'), action == 'appearance-restore')
    if action == 'power-profile':
        profile = request.get('profile')
        if profile not in {'power-saver', 'balanced', 'performance'}:
            raise Error('Invalid power profile')
        if any(run(['systemctl', 'is-active', name], accepted=(0, 3, 4)) == 'active' for name in ['tlp', 'tuned', 'auto-cpufreq']):
            raise Error('Conflicting power service detected; no profile changed')
        if run(['systemctl', 'is-active', 'power-profiles-daemon'], accepted=(0, 3, 4)) != 'active':
            raise Error('power-profiles-daemon is not active; no service is enabled automatically')
        available = run(['powerprofilesctl', 'list'])
        if not re.search(r'(?:^|\n)\s*\*?\s*' + re.escape(profile) + r':', available):
            raise Error('Requested profile is not advertised by this hardware/backend')
        run(['powerprofilesctl', 'set', profile])
        return 'Power profile selected; no power-management service was installed or enabled.'
    if action == 'brightness':
        value = request.get('percent', '')
        if not isinstance(value, str) or not value.isdigit() or not 10 <= int(value) <= 100:
            raise Error('Brightness must be 10–100 percent')
        run(['brightnessctl', '--class=backlight', 'set', value + '%'])
        return 'Backlight setting requested. Permissions and hardware may limit support.'
    if action == 'migration-export':
        return manifest_export(request)
    return migration_stage(request)


core.dispatch = dispatch

if __name__ == '__main__':
    terminal = False
    code = 0
    try:
        terminal = len(sys.argv) == 3 and sys.argv[1] == '--terminal'
        request = json.loads(sys.argv[2]) if terminal else json.load(sys.stdin)
        if terminal:
            # Never echo imported contents or secret values. Paths are reviewable.
            print('Review operation:', json.dumps(request, indent=2), flush=True)
            if request.get('action') not in {'creator-install', 'software-install', 'software-remove', 'official', 'restic-check', 'restic-list'} and input('Type YES to approve: ') != 'YES':
                raise Error('Cancelled before changes')
        if isinstance(request.get('_cancel_path'), str):
            CANCEL_PATH = Path(request['_cancel_path'])
        # High-frequency samples do not create a persistent file per poll.
        print(dispatch(request) if request.get('action') == 'sample' else core.execute(request, interactive=terminal))
    except Exception as error:
        print('Operation failed: ' + str(error), file=sys.stderr)
        code = 1
    finally:
        if terminal and sys.stdin.isatty():
            try:
                input('Press Enter to close; persistent results are in View Logs…')
            except (EOFError, KeyboardInterrupt):
                pass
    sys.exit(code)
