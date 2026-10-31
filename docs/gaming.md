# One-Click Gaming Setup

On the Software home screen, click **Gaming** below Hyprland and Install from GitHub, review the selected apps, and click **🎮 Install Gaming Essentials**. All setup and transaction responses happen in ArchEasy. Permission dialogs explain changes before they begin. When Pacman or Flatpak asks a question, read the live log and use **Send Response** (for example, `y`, `n`, or a provider number). These are answers, not terminal commands. Administrator requests use your desktop's polkit authentication dialog; a running graphical polkit agent is required. Internal terminal authentication is disabled.

**Gaming System Check** reports PCI GPU models and bound drivers, a real `vulkaninfo --summary` probe, native/user/system Flatpak installation status, and missing graphics dependencies. If `pciutils` is missing, the initial report includes detected vendor IDs and drivers; setup installs it for subsequent model lookup. Failed or headless Vulkan probes are reported as unverified, never guessed from package names.

**Install Missing Packages** uses your current checkbox selection and skips installed native packages or equivalent Flatpak apps. **Retry Failed Installation** preserves the failed selection and rechecks installed state. A completion card lists verified applications and explains Steam Play/Proton. A successful package installation with a failed final Vulkan probe still requires graphics verification after restarting the session.

## Applications and fallbacks

The backend validates the enabled `core`, `extra`, and `multilib` package databases at runtime. Repository-qualified native targets are preferred. Package availability changes over time; no AUR helper, downloaded install script, or CachyOS repository is added.

| Checkbox | Default | Native packages | Approved Flathub fallback |
| --- | --- | --- | --- |
| Steam | Selected | `steam` | `com.valvesoftware.Steam` |
| Heroic Games Launcher | Selected | `heroic-games-launcher` | `com.heroicgameslauncher.hgl` |
| Lutris | Selected | `lutris` | `net.lutris.Lutris` |
| Faugus Launcher | Optional | `faugus-launcher` | `io.github.Faugus.faugus-launcher` |
| MangoHud | Selected | `mangohud`, `lib32-mangohud` | None |
| GOverlay | Advanced | `goverlay` | None |
| Gamescope | Advanced | `gamescope` | None |
| GameMode | Selected | `gamemode`, `lib32-gamemode` | None |
| Wine and Winetricks | Advanced | `wine`, `winetricks` | None |
| Protontricks | Advanced | `protontricks` | `com.github.Matoking.protontricks` |
| UMU Launcher | Advanced | `umu-launcher` | None |
| Vulkan tools and dependencies | Selected | `vulkan-tools`, `vulkan-icd-loader`, `lib32-vulkan-icd-loader` | None |

Graphics prerequisites are always checked and installed even if the Vulkan checkbox is cleared. A missing tool with no approved fallback stops planning with its package names; it is never silently omitted. Flatpak itself is installed from an official repository if needed. Adding the official Flathub user remote and installing each fallback require approval. Existing remotes named `flathub` with unexpected URLs are rejected. App availability is checked using `flatpak remote-info` before installation. Existing system Flatpak apps count as installed, and new fallbacks use the user scope.

Flatpak launchers use sandboxed runtimes. Native MangoHud, GameMode, Wine, and Protontricks do not automatically configure the equivalent Flatpak runtimes or grant additional sandbox permissions. Those integrations require app-specific verification.

## Graphics and system safety

Setup supports **x86_64 Arch Linux and distributions declaring `ID_LIKE=arch`** with standard Arch repositories. Other architectures remain supported by ArchEasy's existing features. Supported graphics library plans:

| Bound graphics driver | Libraries |
| --- | --- |
| AMD `amdgpu` | `mesa`, `lib32-mesa`, `vulkan-radeon`, `lib32-vulkan-radeon` |
| Intel `i915` or `xe` | `mesa`, `lib32-mesa`, `vulkan-intel`, `lib32-vulkan-intel` |
| NVIDIA `nvidia` with standard userspace | `nvidia-utils`, `lib32-nvidia-utils` |
| NVIDIA `nouveau` | `mesa`, `lib32-mesa`, `vulkan-nouveau`, `lib32-vulkan-nouveau` |

Hybrid systems receive libraries for both detected GPUs. Unknown/unbound drivers, legacy NVIDIA userspace, and existing AMDVLK installations require manual review. Before adding a missing NVIDIA 32-bit library, its upstream version must match the installed userspace. Kernel drivers, kernel parameters, and desktop settings are never selected or rewritten by setup.

If needed, enabling multilib requires a separate permission dialog and polkit authentication. Only the standard commented `[multilib]` plus mirrorlist section is uncommented. A symlink, changed file, nonstandard layout, or existing `pacman.conf.archeasy-gaming.bak` stops the edit. The helper preserves mode and ownership, creates that backup exclusively, then atomically replaces the configuration. It never adds a repository section or overwrites the backup. After approval it runs a full `pacman -Syu`, so newly enabled databases are usable without a partial upgrade. Configuration changes already approved and completed are retained if a later stage is declined.

Native installation uses interactive `pacman -Syu --needed` with explicit targets, so users must approve a full system upgrade. There is no `--noconfirm`, forced replacement, conflict suppression, or automatic prompt response. Pacman may propose changes during the approved full upgrade; users review and explicitly answer every prompt in the GUI. Existing third-party repository configuration remains in Pacman's normal dependency/upgrade resolution, so review its transaction carefully.

Gaming holds the same update lock as system updates plus a shared package-operation lock. Existing ArchEasy terminal operations also acquire the shared lock, preventing overlap with Gaming and each other. Known active Pacman database locks stop stages without removing lock files. External package managers do not honor ArchEasy's advisory lock; avoid starting unrelated external operations during setup. Pacman's own database lock remains authoritative for native transactions.

**Cancel After Current Stage** stops before the next stage. An active transaction is allowed to finish safely; decline a pending package-manager question using the response field if you want that transaction to stop. Closing ArchEasy is blocked until its gaming session finishes. Completed packages are retained, and retry skips them. Setup does not modify Steam settings, Wine prefixes, games, save files, or user configuration files.

## Development and verification

- `src/gaming/mod.rs`: GTK view, selection state, dialogs, response field, progress, log and completion UI.
- `src/gaming/backend.rs`: embedded Python process lifecycle and JSON-lines protocol; no GTK dependencies.
- `scripts/gaming.py`: discovery, pure planning, version checks, locks, polkit, multilib helper, interactive command streaming and final verification. Embedded in the Rust executable, so release archives need no additional script installation.
- `tests/gaming_smoke.py`: fixture tests for hardware, distributions, native/Flatpak planning and ordering, cancellation, prompt streaming, failures, remote trust, multilib backups, and concurrent-operation refusal. These tests never install packages.
- Rust tests check the embedded catalog/protocol. The ignored graphical test checks checkbox defaults and controls and is included in `tests/native_ui_smoke.py`.
- Gaming tests run in CI, release validation, and the PKGBUILD check phase. Runtime packaging includes Python, polkit, and util-linux.

Run `python tests/gaming_smoke.py`, `cargo test --offline`, `cargo clippy --offline -- -D warnings`, and `cargo build --release --offline --locked`. Existing smoke suites, desktop/Lua validation, and native UI tests remain applicable.

Manual verification on disposable Arch VMs is required for graphical authentication (including dismissal/missing agent), live interactive Pacman/Flatpak prompts, multilib disabled/enabled/nonstandard layouts, mirror failures, cancellation at every stage, retries after partial completion, AMD/Intel/NVIDIA/hybrid hardware and real 32-bit Vulkan games. Check the full application layout at small window sizes and the completion screen. Verify Steam launch and a Proton game; installing packages alone cannot establish game compatibility.

Sources used for library mappings and fallback identities: [ArchWiki Vulkan](https://wiki.archlinux.org/title/Vulkan), [ArchWiki Steam](https://wiki.archlinux.org/title/Steam), [Arch Steam package](https://archlinux.org/packages/multilib/x86_64/steam/), [Heroic Flathub manifest](https://github.com/flathub/com.heroicgameslauncher.hgl), and [Faugus on Flathub](https://flathub.org/en/apps/io.github.Faugus.faugus-launcher). Runtime package metadata remains the source of truth for availability.

The implementation's development-environment results and remaining manual checks are recorded in [gaming-validation.md](gaming-validation.md).
