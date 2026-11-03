# System management

ArchEasy targets an **installed Arch Linux desktop**. These pages do not turn it into a partitioning installer. A live USB can be used for guided recovery, but ArchEasy does not automatically discover or operate on an offline installation.

## Backup & Restore

Use **View Snapshots / Storage Usage** to inspect the root filesystem, Btrfs subvolumes and existing Snapper root or Timeshift configuration. Snapper is eligible only for a configured Btrfs `/` subvolume. Timeshift requires an existing configuration with a backup device UUID; Btrfs mode additionally requires the conventional `/@` root layout. Having both configurations blocks creation. Utility/permission failures are reported; configuration detection is conservative and is not a guarantee of recovery.

**Create Snapshot** opens a terminal and requires `YES`. Individual snapshot commands use sudo. No configurations, boot entries or schedules are created. Pre-update snapshots use the same provider detection and failures block the update. There is no new automatic scheduler or retention engine. **Configure Automatic Snapshots / Delete Snapshot** provides guidance for the native provider; ArchEasy never deletes snapshots, including the last recovery point. Existing provider timers can independently delete snapshots: inspect those settings yourself.

Snapper list includes dates/descriptions and provider metadata. Btrfs usage is filesystem-wide shared storage, not a reliable per-snapshot reclaimable size. Unknown sizes remain unknown. **Compare Snapshots** accepts two positive Snapper IDs and runs its read-only status comparison; privileges may prevent listing/comparison. Timeshift comparison is not implemented.

Btrfs local snapshots do **not** protect against disk failure. ext4, XFS and other filesystems use file backups, not native snapshots. The existing **Snapshots & backups…** picker creates portable ZIP exports and verifies/unpacks them to a new folder; see [backups](backups.md). ZIP backups are not encrypted.

For encrypted external backups, install Restic, mount an external disk, and initialize a repository separately (`restic -r /absolute/mounted/disk/repository init`). Enter its absolute path in the page. ArchEasy currently supports existing **local mounted repositories**, not remote URLs or repository initialization. It does not verify that a path is physically on a different disk: confirm this with `lsblk` before relying on it. Enter a dedicated source folder inside your home, then **Export Backup with Restic**. Restic capture differs from portable ZIP capture: it does not apply ArchEasy's credential exclusions, and selected files may contain secrets. Close applications for consistency. Passwords are prompted interactively by Restic, never saved by ArchEasy. Password-file/password-command/plaintext password environment variables are removed for these operations.

**Check Backup Health** runs `restic check --read-data` and can take a long time. **List External Snapshots** provides IDs. Restore requires a hexadecimal snapshot ID and a new absolute folder; it exclusively creates that folder and restores with verification. Failure may leave a partial folder for inspection. Live paths are never restored over. Linked destination ancestors and overlapping source/repository folders are refused. Keep the external disk mounted and keep the terminal open.

Root restoration is **guided only**, even with Snapper or Timeshift installed. The recovery guide explains offline validation, changed data, excluded subvolumes, boot/EFI consistency, actual bootloader detection and reboot requirements. There is no live root rollback, automatic bootloader edit or integrity guarantee based solely on a provider listing.

## System Updates

**Check for Updates / Review Updates** performs package lock/process, repository, `pacman -Dk` and disk-space checks. A 2 GiB free-space floor on root/cache is a preliminary guard, not a transaction size estimate. Pacman performs the final dependency, signature and space checks. Missing tools fail clearly. `checkupdates` refreshes a separate database; cached `pacman -Qu` is labelled as a fallback when that check fails. Installed official, foreign and Flatpak packages are distinguished. Critical/kernel candidates and reboot guidance are shown. Exact reboot detection for every package is not implemented.

**Update System** opens the existing review dialog, then the shared terminal backend. It repeats the checks, presents fresh `checkupdates` candidates when available (or a clearly labelled cached `pacman -Qu` preview when the executable is missing), links Arch news and proceeds to sudo authentication without a typed approval phrase. Read news yourself; automatic news fetching/manual-intervention matching is not implemented. Pacman's refreshed transaction may differ from the earlier plan and still requires its normal confirmation. Only `sudo pacman -Syu` is run; no signature bypass, forced overwrite, partial upgrade or automatic dependency removal is added. Existing locks are never deleted. Pacman owns the authoritative cross-process lock; preflight alone cannot eliminate a race.

Flatpak updates are a separate terminal action with the normal tool prompts for user and system installations. Foreign packages are listed, but AUR update execution is deliberately manual: independently verify the installed helper, its compatibility and each PKGBUILD. No helper is installed or run automatically by the application. The older `scripts/system-update.sh` and its regression tests remain in the repository for compatibility, but the application update entry points now use the shared backend instead of its combined transaction.

Recent pacman history and persistent operation logs help diagnose failures. The update plan is a candidate list; inspect `/var/log/pacman.log` transaction markers and terminal output to determine the actual affected packages and completion. Retry means inspect the failure, fix its cause, recheck, and approve a new transaction; no failed destructive command is replayed blindly. No automatic downgrade/recovery is attempted.

## Hardware & Drivers

Read-only scans use `/proc`, `/sys`, PCI/USB listings, installed and repository package metadata. They include CPU, RAM, board/BIOS, storage, active PCI drivers, networking, Bluetooth and firmware journal evidence. Missing utilities and access errors appear as unavailable data. OpenGL/Vulkan checks run separately and can fail in headless sessions. Firmware checks use fwupd's current metadata; ArchEasy neither refreshes device firmware nor flashes it.

AMD/Intel recommendations are restricted to absent packages found in configured repository metadata and require generation compatibility review. 32-bit libraries are optional guidance, not an installation list. NVIDIA reports installed DKMS/header mismatches across standard kernels and explains variant, Nouveau and Secure Boot considerations; automatic PCI-generation mapping, custom-kernel compatibility and driver installation are not implemented. No working graphics setup is replaced. Hardware output is treated as display data, never executed or interpreted as markup/commands.

## Dependencies and permissions

Build/install as described in the [README](../README.md). No new Rust dependencies. Python 3 (standard library), util-linux (`findmnt`, `flock` for legacy helpers), procps (`ps`), pacman are needed for full management diagnostics. Optional pacman-contrib (`checkupdates`) provides fresh update previews; without it the preview is cached and may be stale, while the approved full `pacman -Syu` still refreshes repositories and presents its final transaction. Refresh failures from an installed `checkupdates` remain blocking errors. Optional: configured Snapper/Timeshift and btrfs-progs, Restic, Flatpak, pciutils, usbutils, iproute2, bluez-utils, mesa-utils, vulkan-tools, mokutil and fwupd. Install missing packages through a reviewed full-system upgrade, not a separate database refresh followed by selective installs.

The GTK application stays unprivileged. Only approved terminal snapshot/pacman commands use sudo. Read-only diagnostics have bounded time/output; the report shows progress while running. Read-only scan controls now expose cooperative Cancel scan; the integrated workbench shares that cancellation path. Once a package/backup transaction starts, no cancellation guarantee is made: do not close its terminal. Tool output is streamed there and retained in private log files. Keyboard/password input is never logged. Do not type passwords into the GUI fields.

Private JSON records and terminal output are stored in `$XDG_STATE_HOME/archeasy/management` (default `~/.local/state/archeasy/management`). A process lock serializes these management jobs for one user. Interrupted records retain a running/unknown state. The workbench additionally coordinates package operations with Gaming Setup, Developer Tools and the install queue through compatible runtime locks. Unrelated tools, other users and some legacy cleanup/third-party workflows remain outside that application lock; pacman owns the machine-wide package transaction lock. View Logs shows the latest 30 records and up to 64 KiB per terminal log; older files remain on disk until manually removed. Logs can include personal paths, hardware identifiers and package data: review before sharing. Filesystem path checks reduce accidental overwrites but cannot defend against a malicious process concurrently replacing path ancestors; run in your own trusted desktop account.

## Validation and recovery drills

```sh
python3 tests/system_management_test.py
cargo test --offline
cargo clippy --offline -- -D warnings
cargo fmt --all -- --check
```

The Python suite mocks package/snapshot commands and covers malformed PCI/filesystem/update output, missing utilities, network errors, lock handling, low storage, provider conflicts, destination validation, compatibility advice, sudo failure, cancellation, interrupted state and terminal protocol. Existing smoke tests and GUI instructions are in [development](development.md). No destructive host operation is needed.

In a **disposable Arch VM**, manually verify navigation at narrow widths, sudo denial, pacman interactive prompts/logging, pre-update snapshot failures, both provider conflict detection and supported layouts, external-disk disconnects, Restic password prompts and restore verification, physical GPU diagnostics, Secure Boot reporting and fwupd availability. Root rollback, partition edits, firmware flashing and driver replacement are never automated tests. Practice offline recovery from an independently verified external backup before relying on snapshots.
