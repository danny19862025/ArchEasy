# Workbench delivery and verification — 1 November 2026

This extends the **existing uncommitted management expansion**. Its files and functionality were retained. No reset, commit, push, merge, pull request, release or remote-repository modification was performed. The local checkout's origin is the requested ArchEasy repository. No destructive host tests or live package, boot/disk, firewall, SSH, power or restore changes were run.

## Inspection and implementation sequence

Repository inspection covered the file inventory including CI/release workflows, working-tree status, Rust dependencies, navigation, existing store/removal/queue, Gaming Setup, Developer Tools and their locks, config privacy/history/rollback, backup/update/hardware helper and tests. There is no partitioning/live-ISO installer architecture. This is an installed-system GTK4/Libadwaita app using embedded Python and terminal sudo or existing setup polkit boundaries.

The implementation proceeded through shared bounded diagnostics and session monitoring; rescue/disk/network/laptop checks; reviewed appearance/Creator/migration operations; grouped navigation and category reuse; mocked tests/regressions/security review/documentation. No functioning page was replaced with a separate application. The old horizontally scrolling switcher became a keyboard-accessible grouped native menu, and the existing software page became Software Centre. Stable page IDs remain intact for existing setup links.

## Capability status

The complete [capability matrix](workbench.md) distinguishes implemented actions from partial/guided/manual work for all nine modules. This is **not a claim that every requested automation is complete**. In particular:

- Rescue/disk repair, bootloader/EFI changes, firewall/SSH/VPN configuration, hibernation/lid/threshold setup and automatic live dotfile restore are not implemented as mutating actions.
- GNOME presets change only the supported color preference; broader appearance uses native settings/existing Configs. No full multi-desktop preset or actual desktop-preview engine is claimed.
- Migration imports stage selected files in a new directory. They do not reinstall packages, execute imported scripts or overwrite existing configs.
- Monitoring GPU capability is driver-dependent. History is session samples plus system journal history, not durable metric history. Temperature thresholds are not treated as universally safe.
- SMART/EFI/rule reads may fail on permissions; no auto-sudo diagnostic bypass or false healthy result is substituted.
- The existing third-party package/build paths retain their own explicit review and some locking/logging limitations. New modules never fetch/execute arbitrary AUR/upstream install scripts.

## Reused architecture

`src/management.rs` embeds the management and workbench helpers together; requests and tool arguments remain structured. Shared services supply filesystem and package checks, bounded command execution, persistent private state, errors and terminal logs. Workbench cancellation adds a private diagnostic flag observed by the shared runner, while atomic commands ignore scan cancellation. Compatible runtime locks coordinate new package operations with Gaming Setup, Developer Tools and the install queue; pacman retains the authoritative system lock.

`src/workbench.rs` adds GTK pages/Cairo charts and category/grouped navigation. Existing software search/cards/install/Installed/metadata, Configs previews and rollback, System Utilities, portable ZIP/export, backups, updates and hardware pages remain. No additional Cargo dependency or resident service was introduced. Optional native utility dependencies are listed in the guide and PKGBUILD.

## Files added by this expansion

- `src/workbench.rs`
- `scripts/workbench.py`
- `tests/workbench_test.py`
- `docs/workbench.md`
- `docs/workbench-verification.md`

## Files modified by this expansion

- `src/main.rs`: page integration, Software Centre title, default dashboard and navigation, official/Flatpak install routing.
- `src/management.rs` (already uncommitted from the prior expansion): compose trusted embedded helpers; diagnostic cancellation controls and shared adapters.
- `src/store/installed.rs`: reviewed broker removal requests with preserved optional clean-removal choice and Flatpak installation scope; regression assertions updated for the broker boundary.
- `scripts/install-queue.sh`: compatible application locks, pacman lock refusal, full upgrades for every official queue stage, and interactive checkpoints between transactions.
- `tests/setup_store_smoke.py`: isolated runtime locks and updated full-upgrade expectation; original failure-stop assertions retained.
- `tests/native_ui_smoke.py`: include isolated workbench construction test.
- `.github/workflows/ci.yml`: normal-user workbench test suite.
- `scripts/PKGBUILD`: workbench checks and optional diagnostic/power utilities.
- `README.md`, `docs/development.md`, `docs/system-management.md`: feature guide, test commands, current cancellation/locking and queue behavior.

Other pre-existing dirty/untracked management files are preserved, not newly claimed as workbench files.

## Test results and exact commands

All commands were run from `/home/danny/Projects/ArchEasy`:

| Command | Result |
| --- | --- |
| `python3 -W error::ResourceWarning tests/workbench_test.py` | 29 passed |
| `python3 -W error::ResourceWarning tests/system_management_test.py` | 15 passed |
| `python3 tests/setup_store_smoke.py` | 6 passed |
| `python3 tests/snapshot_smoke.py` | 7 passed |
| `python3 tests/maintenance_smoke.py` | 3 passed |
| `python3 tests/gaming_smoke.py` | 17 passed |
| `python3 tests/developer_smoke.py` | 32 passed |
| `python3 tests/github_build_smoke.py` | 4 passed |
| `python3 tests/github_release_smoke.py` | 27 passed |
| `cargo test --offline` | 70 passed, 5 failed, 12 ignored |
| `cargo test --offline workbench::tests` | 2 passed, 1 graphical test ignored |
| `cargo test --offline store::installed::tests::package_sources` | Passed |
| `cargo clippy --offline -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `cargo build --offline` | Passed |
| `cargo build --release --offline` | Passed |
| `python3 -m py_compile scripts/workbench.py scripts/system-management.py` | Passed |
| `bash -n scripts/install-queue.sh scripts/PKGBUILD` | Passed |
| `luac -p presets/hypr/hyprland.lua` | Passed |
| `lua tests/validate-hyprland.lua presets/hypr/hyprland.lua` | Passed, 44 shortcuts/settings |
| `desktop-file-validate assets/archeasy.desktop` | Passed |
| `git diff --check` | Passed |

The seven existing Python suites were also run together with this exact command:

```sh
bash -c 'for t in setup_store snapshot maintenance gaming developer github_build github_release; do python3 "tests/${t}_smoke.py" || exit; done'
```

Total Python tests: **140 passed**, including both management suites and all existing non-graphical Python regression suites.

The five Rust failures are unchanged GitHub mock-server tests calling `TcpListener::bind("127.0.0.1:0")` at `src/github.rs:373`, which receives `PermissionDenied: Operation not permitted` in this restricted environment:

- `github::tests::checks_recipes_and_preserves_card_metadata`
- `github::tests::one_failed_recipe_does_not_discard_verified_results`
- `github::tests::repeated_searches_reuse_recipe_checks_including_missing_files`
- `github::tests::reports_api_errors_and_rejects_bad_tokens`
- `github::tests::respects_candidate_limit`

No tests were disabled to avoid those failures. The graphical suite was **not run** here because Xvfb/xvfb-run is unavailable. An ignored widget-construction test was added to the existing isolated GUI smoke runner/CI, but that does not substitute for actually running it.

## Security review and residual risks

- New command paths use fixed programs/argument arrays, identifier/enum/path validation and least privilege. Output/imported metadata is never executed or rendered as markup. Creator packages are an allowlist checked against configured repositories. No signature bypass/overwrite flag/lock deletion is introduced.
- Configuration writes are limited to explicit GNOME preference changes with persisted before-state/conflict-aware restore, native supported profile/backlight choices, and private migration artifacts. No existing dotfile is overwritten by migration.
- Imports enforce format/checksum/count/size/path/symlink/secret checks and fixed non-executable permissions. Exports open individual regular user-owned files without following a final symlink and reject changes during capture. Existing ZIP traversal/restore regression coverage remains intact.
- Migration secret screening is heuristic; manifests are private, **not encrypted**. Diagnostics/logs can contain identifying network/path/hardware information. No report is uploaded automatically; review before sharing.
- Application locks are per user and do not eliminate races with other users/tools; native package transactions must enforce their own integrity. The legacy queue/third-party/cleanup paths are not all unified under one complete preflight/logging implementation. No atomic cancellation guarantee is made.
- Path checks assume a trusted local user/session; they are not a defense against every same-account concurrent path-replacement attack. A completed command or diagnostic read does not prove recoverability, bootability or complete security.

## Required manual/VM verification and next steps

Before calling this release-ready, run the complete suite in the existing Arch CI/container with loopback sockets and Xvfb. Verify grouped menu/keyboard/narrow-window layout, chart sampling/cancellation/page visibility, report export errors and all nine page flows.

Use disposable Arch VMs for sudo rejection, full pacman/Flatpak transactions and logs, lock conflicts with Gaming/Developer/queue, interruption/retry, GNOME apply/external-conflict/revert, multiple desktop sessions, unsupported power profiles and brightness permissions. Test migration export/preview/staging across desktop/kernel/GPU differences and corrupt/malicious inputs; confirm no original configs change. Check BIOS/UEFI, GRUB/systemd-boot, Btrfs/ext4/XFS, encryption/RAID/LVM and multi-boot **diagnostics** against known fixtures. Physical sensor/battery/SMART/Wi-Fi availability needs real hardware validation.

Do not format/resize/repair partitions, flash firmware, replace a bootloader, lock out a firewall or restore a live root to test this on the host. Those actions remain outside the automatic implementation.

Future improvements: fully unify legacy package queues/third-party/cleanup transactions under the broker; add durable bounded metric storage and more vendor GPU adapters; implement capability-gated multi-desktop appearance previews; add an audited selective live-dotfile plan/backup/apply/revert workflow; extend read-only rescue fixtures and machine-wide broker coordination.

## Local installation

The release build was installed atomically to `/home/danny/.local/bin/archeasy`, which both existing desktop launchers already target. The previous executable is preserved at `/home/danny/.local/state/archeasy/install-backups/20261101-084922-workbench/archeasy`. Installed/build SHA-256 equality was verified (`abc23e3cb7b1956c6cca90fb46464ff920e9c7f92e7ac514151d5dc31bccfd69`). Reopen ArchEasy to see System Health and the grouped navigation. No running process was terminated and no user desktop/system configuration was changed by installation.
