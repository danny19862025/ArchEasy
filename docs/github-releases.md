# Install from GitHub

The **Install from GitHub** page opens beside **Hyprland** on the Software home screen and installs published releases independently of the existing GitHub PKGBUILD search/build workflow.

1. Paste a public HTTPS GitHub repository URL. An optional `.git` suffix or trailing slash is accepted. Credentials, query strings, fragments, encoded paths and paths below the repository are rejected.
2. **Inspect Repository** reads repository metadata and GitHub's latest stable release endpoint. Drafts and prereleases are excluded; no older or prerelease fallback is chosen automatically.
3. Choose a matching download. Review its app name, version, architecture, installation method, asset URL, size when published, checksum availability and destination paths.
4. **Install Reviewed Release** rechecks the release metadata, downloads the asset, verifies supported published checksums, prepares files in a private staging directory and validates executable architecture before publishing the installation. The app is never launched.
5. **Installed GitHub Apps** lists tracked apps. **Remove Selected GitHub App** requests confirmation and removes only that installation, its launcher and its generated desktop entry. User settings elsewhere are retained.

## Supported first-version downloads

- x86_64 (`x86_64`, `amd64`, `x64`) and aarch64 (`aarch64`, `arm64`). Filenames must explicitly identify the host architecture. Archives and standalone binaries must also identify Linux; AppImage implies Linux.
- AppImages and standalone Linux ELF binaries, including `.bin` files. Actual ELF class, byte order, executable type and machine architecture are checked independently of filenames.
- `.tar.gz`, `.tgz`, `.tar.xz`, `.tar.bz2` and ZIP archives. Archives retain support files; one executable matching the repository name is preferred, otherwise exactly one executable must be identifiable. Shared libraries are excluded from entrypoint selection. Shell launchers are unsupported.
- Published GitHub asset SHA-256/SHA-512 digests, asset-specific SHA-256/SHA-512 sidecars, and common checksum manifests (GNU and BSD formats). A malformed, missing matching or mismatched published checksum stops installation. If no supported published checksum is found, review explicitly says HTTPS only. Checksums provide integrity, not independent publisher authentication; signatures are not verified in this version.

Source-only repositories, releases without a compatible explicitly named asset, installer scripts, source builds, ambiguous multi-executable bundles and archives containing symlinks/hardlinks or special files require manual installation. No README commands are interpreted. No downloads are piped to shells. No app, build script, installer, desktop metadata command or post-install hook is executed. Root installation and removal are refused.

## Storage and recovery

Each repository has a stable hashed directory under `~/.local/share/archeasy/apps`, containing `payload/` and `.archeasy.json`. The record tracks repository, release, architecture, method, selected asset, verified SHA-256, executable, launcher and generated desktop-entry hash. Launchers use `~/.local/bin/archeasy-owner-repository`. Existing files are never overwritten; remove the tracked version before installing another version. Add `~/.local/bin` to PATH yourself if necessary; ArchEasy does not edit shell files.

AppImages and archives containing `.desktop` metadata get a newly generated, safely quoted desktop entry under `~/.local/share/applications`. Commands from downloaded desktop files are never copied. CLI apps get a launcher without a menu entry. Icons, MIME handlers and background services are not registered. AppImages may need FUSE; native binaries may need host libraries. Installation does not assert that the program will run on every Linux distribution or CPU variant.

Downloads use HTTPS GitHub hosts with redirect validation and bounded requests. The installer does not use authentication or forward tokens to asset hosts. GitHub rate-limit, network and missing-release errors are reported in the UI. Downloads are limited to 2 GiB, extraction to 4 GiB and 20,000 archive members. Archive paths, modes and types are checked; only regular files and directories are copied.

An advisory lock serializes release installations/removals. Failures clean staging files. Publishing failures roll back the app and launcher. A process crash can leave a tracked app without its launcher/menu entry; refresh and remove it before retrying. Edited/replaced external launchers or menu entries are preserved and removal asks for manual resolution. Successfully installed apps and user settings are never globally rolled back. There is no automatic upgrade, launch or background updater.

## Implementation and verification

- `src/github_release/mod.rs`: GTK4/libadwaita page and JSON-lines event handling, reusing the existing process adapter.
- `scripts/github-release.py`: embedded standard-library backend for repository inspection, planning, download/checksum verification, safe extraction, tracking and removal.
- `tests/github_release_smoke.py`: mocked metadata/download fixtures and temporary-home install/removal integration tests; never launches downloaded code or accesses the network.
- `src/main.rs`: navigation registration. CI, release workflow and PKGBUILD register the fixture suite.

Run `python tests/github_release_smoke.py`, `cargo test --offline github_release::`, `cargo clippy --offline -- -D warnings`, and `cargo build --release --offline`. Live GitHub downloads and desktop-menu behavior should also be checked on a disposable graphical Linux session before release.


Current workspace validation: 27 release fixture tests and 69 existing Python tests pass. The embedded Rust backend protocol test, standard Clippy with warnings denied, formatting, Python syntax checks, desktop-entry validation and release build pass. The complete Rust suite reports 68 passed, five existing GitHub fixture failures because sandbox policy denies local socket binding, and ten ignored tests. The new graphical test cannot initialize GTK in this environment. An additional all-target Clippy run reports four pre-existing lints in `src/cloud/tests.rs` and `src/configs/community.rs`. No live apps were downloaded or installed, and no commits or pushes were made.
