# One-Click Developer Setup

On the Software home screen, click **Developer Tools** below Hyprland and Install from GitHub. Essentials are selected initially. Choose **Recommended**, **Advanced**, or one of the four developer presets, click **Apply Preset**, and customize individual tools and sources. Category selection/clearing and Select All/Deselect All are available. **Install Developer Essentials** and **Install Missing Tools** both install only the current selection and required dependencies, skipping installed tools. Select All can include incompatible or unavailable choices; planning explains these before making changes.

The page uses the existing GTK4/libadwaita styling. It includes a stage progress bar, expandable live logs, source selectors, per-tool Installed/Not Installed/Failed indicators, a readiness report, a dedicated expandable readiness report, and a completion summary. The navigation scrolls horizontally to keep all sections accessible on smaller windows.

## Presets

- **Recommended:** Git/Git LFS, base-devel, curl/wget/jq/unzip, CMake, Ninja, pkgconf, GDB, OpenSSH.
- **Advanced:** Basic Developer plus Rust, Go, Node.js/npm, Clang/LLVM, Java, terminal utilities, Podman/Distrobox, database clients, GUI development tools and Codex. Excludes services, Ollama, and AUR-only choices by default.
- **Basic Developer:** Essentials, Python/pip/uv and Neovim.
- **Full Stack Developer:** Basic Developer, Node.js/npm, SQLite, PostgreSQL and MariaDB clients, Docker and Docker Compose.
- **Linux App Developer:** Essentials, Rust via rustup, GTK4/libadwaita, Qt6, Meson, Flatpak Builder, AppStream and Arch devtools.
- **AI Developer:** Git/Git LFS, base-devel, essential utilities, Python/pip/uv, Node.js/npm and Codex. Ollama remains an optional checkbox; models are never downloaded automatically.

Applying a preset changes checkboxes; it does not start installation or erase customized source choices. Compose requires Docker; Distrobox reuses a selected/installed Docker or Podman engine, otherwise the plan explicitly adds Podman. pip/uv require Python. AUR builds add Git/base-devel; upstream Codex adds Node.js/npm. Required dependencies are listed in the approval plan.

## Installation and safety

A read-only preflight verifies enabled official `core`, `extra`, and `multilib` package databases, installed packages, package metadata, declared conflicts, and Pacman's dependency resolution. Repository-qualified native targets and dependency versions appear in the review window. Installing uses interactive `pkexec --disable-internal-agent /usr/bin/pacman -S --needed -- ...`. A graphical polkit authentication agent must be running. No repository refresh, partial `-Sy` transaction, or automatic whole-system upgrade is performed. Keep Arch up to date using the existing System Update workflow before installing; stale mirrors/databases produce an error and can be retried after updating.

All commands use argument arrays. No shell command is assembled from selections. Pacman's interactive prompts appear in the live log; use the response field to enter `y`, `n`, or a provider number. No `--noconfirm`, forced overwrite, or automatic replacement response is used. Metadata failures and installed-package conflicts stop installation rather than being interpreted as safe. Normal dependency packages are installed as part of the approved transaction; unrelated developer applications are not selected implicitly.

Developer Setup uses Gaming's private-PTY interactive runner and JSON-lines process protocol, plus the same package/update locks as Gaming and the existing terminal workflows. Pacman and subsequent Flatpak/npm/build stages run sequentially. Known Pacman database locks stop stages. ArchEasy advisory locks cannot govern unrelated external package managers; avoid starting external operations while setup runs. Pacman's own database lock remains authoritative.

Review windows show exact operations before execution. Later AUR archive paths, build dependencies and upstream npm versions are confirmed separately when known. **Cancel After Current Stage** does not kill a live package transaction. Let it finish or decline its pending prompt. Later stages stop, installed packages remain, and **Retry Failed Packages** uses the previous sources and only the tools still incomplete. Closing ArchEasy is blocked during an active session. No rollback or removal of successfully installed packages is attempted.

Installing development tools does not enable Docker, Ollama, database servers, SSH or other services. It does not initialize databases, add privileged groups, change login shells, edit shell profiles, run `git lfs install`, or configure Starship. Docker group access is root-equivalent and is explained in the UI; this feature never grants it. Existing service/account/shell configuration is preserved.

## Source-specific behavior

**Editors:** Code - OSS uses Arch's `code` package. Microsoft Visual Studio Code uses the distinct `com.visualstudio.code` Flatpak or explicitly reviewed `visual-studio-code-bin` AUR package. VSCodium uses `com.vscodium.codium` or explicitly reviewed `vscodium-bin`. A native Code - OSS/Microsoft conflict is rejected; separate Flatpak editions can coexist. A `code` executable alone is never treated as proof of a particular edition.

**Flatpak:** Only explicitly selected Flatpak sources are used. Existing user and system Flatpaks count as installed; new apps install for the user. Creating the official Flathub user remote requires separate approval, and its URL is reverified. App availability is checked with `flatpak remote-info` before installing. An existing unrecognized remote URL is rejected. Sandboxed editors may need additional SDKs, filesystem access and host-tool integrations; these are not granted automatically.

**AUR:** AUR sources require both a per-tool source choice and the global advanced opt-in. Availability is checked against AUR, then recipes are cloned from fixed `aur.archlinux.org` package URLs with Git hooks/templates disabled. A pinned revision and every tracked text build file are displayed for explicit trust approval before any PKGBUILD is evaluated. Changed revisions, symlinks, binary/oversized reviews or missing recipe metadata stop the build. Declared architecture-specific build/runtime/test dependencies must resolve from official repositories and receive their own approval. Recursive unreviewed AUR dependency builds are refused. Dependency-query errors stop the build; only Pacman’s documented missing-dependency result is accepted. Recipe hashes are checked immediately after the final build approval. `makepkg --nodeps --cleanbuild` runs as the user; it cannot initiate dependency installation through makepkg. Only the selected package's locally built archives, with verified package identities and paths, are offered for explicit `pacman -U` approval. User build code can access user files: approving a review is a trust decision, not a sandbox or an ArchEasy audit. Custom makepkg output directories and complex recipes may require manual installation.

**Rust:** The official rustup package and a separately approved stable minimal toolchain are used. Distribution `rust`/known variants are not replaced; deselect rustup or resolve the existing toolchain choice manually. A working installed stable toolchain is reused. Existing default toolchains are preserved; no shell file is changed. If a different default exists, `rustup run stable cargo` selects the installed toolchain without changing that default. Readiness probes disable automatic rustup downloads.

**Codex:** Official Arch `openai-codex` is preferred. The optional upstream npm source validates `@openai/codex` on the official npm registry, pins its version, displays integrity metadata and uses `--ignore-scripts`. Installation is user-local at `~/.local/share/archeasy/developer/npm`; no global npm configuration or shell PATH is changed. The executable is `~/.local/share/archeasy/developer/npm/node_modules/.bin/codex`. Version verification must succeed; scripts are not enabled automatically to repair a failed install. Authentication is a separate user action. No API key or account credentials are requested by ArchEasy.

**Redis:** Arch moved Redis to the AUR in favor of Valkey. The Redis checkbox is never silently substituted with Valkey. If the official package is unavailable, explicitly choose the reviewed AUR source or deselect Redis.

## Environment check

**Check Development Environment** reports GCC, Clang, LLVM, Python/pip/uv, Rust/Cargo, Go, Node.js/npm, Java, Ruby, PHP, Git, Docker/Podman, Codex and editor versions; source-aware tool status; missing essential packages; GTK4/libadwaita and Qt6 toolkit status/versions; and configured rustup toolchains. Missing or unconfigured commands are reported clearly. Each version probe is bounded and receives no GUI protocol input. Results appear in a dedicated expandable report and remain available in the live logs. The upstream Codex source is checked at its user-local executable path. Readiness means installed tools, not active services or a validated real project build.

## Implementation files

- `data/developer-tools.json`: reusable package manifest, source identities, dependencies, notes, categories and presets (52 selectable tool entries).
- `scripts/developer.py`: pure dependency/source planner, readiness checks, preflights, locks, reviewed AUR lifecycle, upstream install policy and partial-failure recovery.
- `src/developer/backend.rs`: embeds the manifest/backend and loads Gaming's shared process/transaction runtime, without GTK dependencies.
- `src/developer/mod.rs`: GUI selections, category/preset controls, status, review windows, progress, logs and retries.
- `src/gaming/backend.rs`: reusable process entry point for both backends; existing Gaming API retained.
- `scripts/gaming.py`: shared read-only query runner now closes subprocess stdin so probes cannot consume GUI protocol messages.
- `src/main.rs` and `src/ux.rs`: setup pages, home-screen shortcuts and scrollable navigation.
- `tests/developer_smoke.py`: mocked unit/integration coverage; the Rust test checks embedded manifest/protocol, and an ignored graphical test checks defaults, presets and controls.
- CI, release validation, PKGBUILD and native UI smoke test registration include Developer Setup tests. All runtime code/data are embedded, so release archives need no external developer script installation.

## Validation and remaining checks

Development-environment results: 32 developer tests, 17 Gaming tests, and 20 existing Python smoke tests pass. The Rust suite reports 67 passed, five existing GitHub fixture failures at `TcpListener::bind` (`src/github.rs:373`, sandbox `PermissionDenied`), and nine ignored tests. Clippy with warnings denied, release build, rustfmt, Python syntax, desktop-file validation, Bash syntax and Lua/Hyprland checks pass. The graphical suite cannot initialize GTK in this environment; its first existing test stops before the new Developer Tools widget test. CI retains all tests without exclusions.

Before release, verify the full GUI and layout at small window sizes, live polkit approval/dismissal, package-manager prompts, cancellation and retries, absent/stale repositories, real AUR recipe/dependency/archive flows, Flathub sandbox behavior, rustup with existing toolchains, and upstream Codex's script-disabled installation on supported architectures. Use disposable Arch systems for live package changes. No live installation, commit or push was performed during implementation.

Reference checks: [Arch Code variants](https://wiki.archlinux.org/title/Visual_Studio_Code), [Arch rustup](https://archlinux.org/packages/extra/x86_64/rustup/), [Arch openai-codex](https://archlinux.org/packages/extra/x86_64/openai-codex/), [official Codex CLI documentation](https://developers.openai.com/codex/cli), [VSCodium Flathub](https://flathub.org/en/apps/com.vscodium.codium), and [Arch Redis/Valkey transition](https://archlinux.org/news/valkey-to-replace-redis-in-the-extra-repository/). Availability is always rechecked against the running system.
