# ArchEasy

A Rust + GTK4 software browser for Arch Linux. Search Pacman, AUR, and Flatpak together, filter by source, see installed packages, and start installation from a package card.

## Screenshots

Store (the **Software** tab), searching official packages:

![ArchEasy Store with Firefox search results](assets/screenshots/store.png)

Configs, with built-in presets and backup controls:

![ArchEasy Configs tab](assets/screenshots/configs.png)

Installed software, with animated percentage circles by source:

![ArchEasy Installed tab with Pacman, AUR and Flatpak percentage circles](assets/screenshots/installed.png)

## Install

### Binary release

Download the `x86_64` archive and its SHA-256 file from [GitHub Releases](https://github.com/danny19862025/ArchEasy/releases). Built on Arch Linux for Arch Linux; GTK4 4.10+ and Libadwaita 1.2+ are required and are not bundled.

```sh
sha256sum -c archeasy-0.1.0-x86_64-linux.tar.gz.sha256
tar -xzf archeasy-0.1.0-x86_64-linux.tar.gz
cd archeasy-0.1.0-x86_64-linux
install -Dm755 archeasy "$HOME/.local/bin/archeasy"
bash scripts/install-desktop.sh
```

Ensure `~/.local/bin` is on your desktop session's `PATH`.

### Build and install with Cargo

```sh
git clone https://github.com/danny19862025/ArchEasy.git
cd ArchEasy
cargo install --path . --locked
bash scripts/install-desktop.sh
```

Cargo installs the executable only. The helper installs the launcher and icon under `$XDG_DATA_HOME` (default `~/.local/share`). Ensure `~/.cargo/bin` is on your desktop session's `PATH`.

### Arch package / AUR recipe

The VCS recipe is [scripts/PKGBUILD](scripts/PKGBUILD). Build it as your normal user in a separate folder:

```sh
mkdir -p /tmp/archeasy-package
cp scripts/PKGBUILD /tmp/archeasy-package/
cd /tmp/archeasy-package
makepkg -si
```

This installs the binary, desktop launcher, icon and licenses. `paru -S archeasy-git` will work once a maintainer submits this recipe to the AUR; adding a PKGBUILD to GitHub does not register an AUR package.

## Appearance

The top bar includes **About** (app version and installation information) and a **Theme** selector: System, Light, Dark, or Custom. Theme changes apply immediately and are saved in `$XDG_CONFIG_HOME/archeasy/theme` (normally `~/.config/archeasy/theme`). System uses your desktop’s GTK appearance. Missing, unreadable or corrupt theme preferences fall back to System, including a Custom selection whose palette is missing or invalid. The **Colours…** button beside Theme opens **Customize ArchEasy theme**. Choose background, surface, text, secondary text, accent, and border colours using colour pickers or #RRGGBB values. **Apply theme** saves and selects Custom; **Cancel** leaves the saved theme unchanged. **Reset colours** stages the default palette until you apply it. Custom colours are saved in `~/.config/archeasy/custom-theme.json` (under `$XDG_CONFIG_HOME` when set).

The app background includes the Arch logo with a subtle blue halo. Buttons have rounded styling, hover glow, visible keyboard focus, and pressed feedback. **Sound on / Muted** toggles the short click sound and saves the preference in `~/.config/archeasy/sound` (or `$XDG_CONFIG_HOME/archeasy/sound`). Sound playback uses PipeWire `pw-play`, PulseAudio `paplay`, or ALSA `aplay`, whichever is available, and runs outside the UI thread.

## Run

On opening, the Arch logo scales and fades in with blue light rings and the ArchEasy wordmark, then reveals the app after 1.8 seconds. Click or press Escape to skip. The intro is omitted when desktop GTK animations are disabled.

```sh
cargo run --release
```

Requires GTK4 (4.10+), Libadwaita (1.2+), Rust, pacman, curl, and a supported terminal (Konsole, Kitty, GNOME Terminal, Xfce Terminal, or xterm). Flatpak search requires Flatpak and configured remotes. **Setup ArchEasy** upgrades Arch and refreshes its package database, installs Flatpak/build tools/Python, builds paru if missing as your desktop user after PKGBUILD review, and adds Flathub for your account. Requires sudo access and a working Arch mirror configuration. [Paru installation](https://github.com/Morganamilo/paru#installation), [Flatpak remotes](https://docs.flatpak.org/en/latest/using-flatpak.html).

The Install button opens an interactive terminal. Passwords, transaction confirmation, dependency choices, and AUR build review remain in the package manager. Pacman uses `sudo pacman -Syu --needed` so installation can also update the system. Flatpak installation uses your user account. Each result has a link beside Install: **Arch package** opens the official Arch package listing filtered to that exact package name, **AUR page** opens its AUR page and build files, and Flathub results have a **Flathub page** link. Links remain available for installed packages. Close an installation terminal only after its transaction finishes. Search again afterward to refresh installed status.

Search errors appear above the results. Install errors appear in a dialog. If a terminal crashes immediately, ArchEasy tries another supported terminal and reports startup failures. After a launch failure, search again to retry. Pacman searches the locally synchronized database; AUR searches the online RPC API; Flatpak searches configured AppStream catalogs. Results are limited to 150 cards; use a more specific search for large result sets. No automatic package installation or removal occurs on app startup.

## System updates

The top-bar **System update** button first offers a Timeshift or Snapper snapshot when installed, or an update without a snapshot. Configure the snapshot tool beforehand; Snapper uses its `root` configuration. A requested snapshot failure stops the update. The terminal then runs:

1. `sudo pacman -Syu` for official packages.
2. `paru -Sua` or `yay -Sua` for AUR packages only.
3. `flatpak update --user` for your account.
4. `flatpak update --system` for the default system-wide Flatpak installation.

Review the usual password, package, and build prompts in that terminal. No confirmations are bypassed. If Pacman fails or is cancelled, AUR is skipped; Flatpak steps still run independently. Missing tools, failed commands, and skipped steps appear in the final summary. A per-user lock prevents duplicate ArchEasy update sessions. Keep the terminal open until package operations finish; it waits at the summary so you can read it. Nothing updates automatically on app startup.

## AppCleaner

The **AppCleaner** tab beside **Configs** includes the standalone AppCleaner's functions: installed application discovery and search, background scans of matching settings/cache/data folders, folder sizes and partial-scan indicators, individual folder selection, a confirmation listing exact paths, and moving selected items to the desktop Trash. Restore items through your file manager's Trash. **Run in terminal** executes the displayed uninstall command with normal password and package-manager confirmation prompts. The command remains visible and can still be copied. Only supported, validated uninstall commands can run; file cleanup stays separate from package removal.

AppCleaner follows ArchEasy’s **System, Light, Dark, or Custom** appearance immediately. Its sidebar and controls use the shared GTK colours, and **Colours…** edits the one custom palette for the whole app. AppCleaner has no separate theme selector or palette; old standalone/embedded AppCleaner theme preferences are ignored.

Discovery starts on first opening the tab. Matches use exact desktop IDs/names; shared folders, top-level symlinks, and folders outside your home roots are excluded. Nothing is selected automatically. Close the target app and review paths before cleanup. Trash failures are reported without permanent deletion.

The embedded AppCleaner code's MIT license is included in `assets/AppCleaner-LICENSE`.

## Cloud Backup

The **Cloud Backup** tab saves existing user configuration files through rclone, with Google Drive and Dropbox connection buttons and support for existing rclone accounts. Requires `rclone` on PATH. Sign-in opens in a terminal/browser; return and click **Refresh accounts** when finished. ArchEasy reads account names/types, not OAuth tokens, and does not copy the rclone credentials into its settings. The suggested list excludes the rclone config folder.

Choose an account, set a relative cloud folder (default `ArchEasy/ConfigBackups`), tick config paths or use **Add files… / Add folder…**, then choose **Save selected configs to cloud**. Nothing is selected by default. Review the exact paths and destination in the confirmation. Config files can contain passwords/tokens. Backups are uploaded as normal cloud files; encryption depends on the configured remote, such as an rclone crypt remote.

Each backup gets a unique dated folder containing `manifest.json` and `files/`, with source paths relative to your home directory. Files are staged privately, with a limit of 512 MiB and 100,000 entries. Linked paths and special files are excluded; nested links are counted as skipped. A file changing while it is copied aborts the snapshot. Close applications whose config you are backing up for a consistent result. Temporary snapshots are removed when the task finishes; failed uploads may leave a partial cloud folder, which is reported. Transfers run in the background; keep ArchEasy open until the status reports completion.

**List backups** reads the selected account/folder. **Download backup…** copies the selected snapshot into a new subfolder of a location you choose, refusing an existing destination. Downloads never apply configs automatically. Review `manifest.json` and copy the desired contents of `files/` into your home folder to restore. Existing cloud backups and local config files are not removed by these operations.

Validation uses a temporary local rclone alias remote for upload/list/download and a graphical test for selection, cancellation, upload confirmation, and backup listing. Google Drive/Dropbox browser authentication and live account transfers require your account sign-in; they are not exercised by automated tests.

References: [rclone account setup](https://rclone.org/commands/rclone_config_create/), [copy](https://rclone.org/commands/rclone_copy/), [listing](https://rclone.org/commands/rclone_lsjson/).

## Portable snapshots and file backups

Click **Snapshots & backups…** at the top of the main window, beside **System update** (also available in **System**). Click **How to create, transfer & restore a backup…** inside that dialog for the full step-by-step guide. Leave **Include installed app lists…** enabled to create a migration ZIP with the existing `my-system.sh` installer and managed configs. Tick the files/folders to include, or use **Add files… / Add folder…**. For a files-only backup, untick the app option. Save the ZIP outside all selected folders. Nothing is selected or backed up automatically.

For local/custom apps, include their project folders, `.local/bin`, desktop launchers and app data. The suggested paths include Projects, Documents, Pictures, Desktop, `.config`, `.local/share` and Flatpak user data (`.var/app`). Package-managed apps reinstall from current repositories; unavailable or vendor packages need their original sources. This is a portable app/file selection, not a disk image or an exact-version system clone.

Backups are private ZIP files (mode 0600) with a dated manifest, SHA-256 checksums, `home/` file contents, restore instructions and a standalone Python helper. Empty folders and ordinary file permissions are retained. Overlapping selections are deduplicated. Linked selections are rejected; nested links and special files are skipped and counted. Limits: 20 GiB and 200,000 entries. Files changing during copy abort the backup. Close apps before capture. The ZIP can contain credentials and is not encrypted; store it privately.

**Verify & unpack backup…** checks a snapshot and extracts into a new, uniquely named folder inside your chosen destination. Existing live files stay untouched. Review `home/` before copying desired files into your home. On another Arch computer, review and run `bash /path/to/unpacked/my-system.sh` to reinstall packaged apps and restore managed configs using the installer's rollback backups. Custom programs may require rebuilding or their original dependencies.

Without ArchEasy, extract only `restore-snapshot.py` from the ZIP, review it, then run:

```sh
python3 restore-snapshot.py restore /path/to/snapshot.zip /path/to/NEW_FOLDER
```

The destination must not exist. Unpacking validates paths, checksums and limits and never runs the included app installer. A failed unpack may leave an incomplete output folder for inspection. Keep ArchEasy open until a running backup or unpack finishes.

## Store queue and installed packages

Tick **Queue** beside search results, including results from different searches, then **Install queue**. One terminal installs packages in the selected order and stops on failure. Earlier completed installs are kept. Pacman refreshes and upgrades the system with the first official package; subsequent official packages use the refreshed database. AUR packages use paru/yay and retain build review prompts. **Clear queue** clears selections; tick a result again to re-add it.

The **Installed** tab shows three percentage circles for **Pacman**, **AUR**, and **Flatpak**, with installed counts. The circles pop into view in sequence and fill/count up when the tab opens or you refresh; they respect the desktop’s reduced-motion setting. Percentages use the full list, regardless of the search filter, and round to 100% when entries exist. The list includes explicitly installed native/foreign Pacman packages and user/system Flatpak apps (separate installations count separately; Flatpak runtimes and Pacman dependency-only packages are excluded). AUR includes locally built/foreign packages, which Pacman cannot distinguish from AUR builds. If a source query fails, known entries remain visible and percentages show as unavailable until a successful refresh. Filter by app, package or source. **Remove** opens `sudo pacman -R` or the matching user/system `flatpak uninstall` command with normal confirmation prompts. Refresh after a transaction to update the list.

## Personal configs

The **Configs** tab includes Fastfetch ASCII/image, Fish, Starship, Ghostty, Kitty, Hyprland, and starter presets for Neovim, Tmux, Waybar, Rofi, Wofi and btop. Original preset sources are documented in [presets/README.md](presets/README.md).

Every built-in preset has **Install & Apply**. First review current/proposed files in two columns, with changed lines highlighted. Confirming saves a private, complete backup directory and ZIP before opening one setup terminal. That terminal installs missing dependencies with `sudo pacman -Syu --needed`, applies the reviewed configs, and optionally changes the login shell for Fish/full setup. The Fish checkbox is off by default. Installation failure prevents application; configs changed since review require another diff. Setup terminals use a per-user lock. **Show result** opens the app or gives session instructions. Nothing applies at startup.

**Danny’s Full Setup** combines Fish, Starship, Ghostty, Kitty, Fastfetch ASCII and Hyprland with its companion tools. Starship is enabled in Fish through a guarded conf.d file. One ZIP covers the entire before-state before any package installation starts. **Restore stack backup** reverses config changes including introduced files; installed packages and login shell are unchanged.

Configs live under `$XDG_CONFIG_HOME` (`~/.config`); backups under `$XDG_DATA_HOME/archeasy/backups` (`~/.local/share/archeasy/backups`). File replacements are atomic, backups are private, linked parents require explicit conflict resolution, and local **Restore backup** preserves original file permissions and symlinks. Python 3 is required for ZIP creation; **Setup ArchEasy** installs it.

**Add screenshot…** saves a scaled, private PNG preview on each preset card. Hyprland includes a clearly labeled illustration of its gaps, rounding and blue/pink borders until you add your actual desktop screenshot. Previews stay local.

### Sharing and importing

**Import current config…** saves a named local preset from a regular UTF-8 file under your config folder, up to 1 MiB. It leaves the source unchanged.

**Export to GitHub** reviews file contents before **Publish preset**. Leave the repository field empty to create a secret/unlisted Gist, or enter an existing `owner/repo` to commit a new `archeasy/<preset>-<timestamp>.json` file on its default branch. Repository visibility controls access; the public checkbox applies only to Gists. Use **GitHub sign-in** or install GitHub CLI from the dialog. GitHub CLI handles authentication; repository exports need write access. Binary Gist assets are base64 encoded with a manifest. Export staging is removed when the command completes. Unlisted Gists can be read by anyone with the URL.

**Import Gist / Repo URL…** accepts an ArchEasy Gist URL or a GitHub blob URL for an exported JSON preset. GitHub CLI fetches the files, paths and size limits are validated, and a local preset is saved. A diff opens before any application; private sources require GitHub sign-in. Repository import supports branch names without slashes. Imported dependencies are restricted to known app commands and remote content is never executed during import.

### Backup ZIP transfer

**Export backups ZIP…** includes all backup history plus the current managed config files referenced by that history. **Save backups to Drive…** creates the same ZIP and uploads it into `ArchEasy/Backups` through an rclone connection; configure Google Drive in **Cloud Backup** first. Upload failures retain the local ZIP for retry.

On a new PC, download the ZIP and select **Restore from ZIP…**. It opens the current managed configs in the diff and saves a local rollback snapshot before applying. Historical before-states remain in the ZIP’s `history/` folder. Portable transfer excludes symlinks, and restoration never extracts arbitrary ZIP paths. ZIP export refuses existing destinations and limits history to 512 MiB/100,000 files and portable configs to 8 MiB/100 files. Close apps before exporting for a consistent snapshot.

### Hyprland

The existing Lua preset preserves your 5px inner/15px outer gaps, 15px rounding, blur and blue/pink borders. **Install & Apply** installs the compositor and companion tools, saves the backup and writes `hypr/hyprland.lua`. The setup guide explains shortcuts and login session selection. Active Hyprland sessions may reload immediately. Existing legacy `.conf` files are preserved. Compositor behavior still requires validation in an actual Hyprland session.

## Config history and managed folders

Every successful **Apply** saves an applied version in the existing private backup directory. **Config timeline…** lists dated versions with relative ages and opens a diff before restoring one. Restores make another backup and record another version. Older backups appear as **before Apply** entries when they contain regular saved files. Those legacy timeline entries restore saved files; use **Restore backup** for the original full rollback, including removing introduced files or restoring links.

If the config directory itself is the root of a Git repository, ArchEasy creates a commit containing only the applied config paths, using an isolated temporary index. Unrelated staged files stay staged and are excluded from the commit. If any selected config path already has staged changes, automatic commit is skipped and the reason appears in the version entry. No-change applies still get a timeline snapshot. Git failure never discards an applied config or its backup. Repository hooks and signing are not invoked for the automatic commit; your configured identity is used, with an ArchEasy identity fallback. ArchEasy does not initialize repositories or push commits.

For linked parent directories, the conflict resolver offers **Overwrite**, **Merge**, and **Skip** per target file. Overwrite detaches the approved parent link into a local copy while keeping its other files; Merge uses the current text in an editable pane so you can combine it with the proposed preset. Skip leaves that file's contents unchanged. The following diff remains the final review before writes. Source-manager files are not overwritten. **Restore backup** recreates original links and retains detached local trees under `managed-retained` so later local additions are not deleted. Managed folder copying is limited to 512 MiB / 100,000 entries.

## Community preset store

**Community presets…** reads `presets/community.json`, or your override at `~/.config/archeasy/community.json`. The starter catalog includes official Catppuccin Mocha/Latte Kitty themes and the bundled Danny Hyprland preset. Entries can be ArchEasy Gist URLs or objects with `name`, `description`, `url`, and optional GitHub file `target` and `dependencies`. Downloaded files are saved as local presets and reviewed through the same diff, dependency, backup and conflict flows. Kitty themes add an include to the existing Kitty config. Downloads do not execute remote scripts. Catalog curation does not pin upstream revisions; review each download.

## System helpers

The **System** tab detects the running kernel, installed linux/linux-zen/linux-lts/linux-hardened packages, GPU PCI information and known NVIDIA driver packages. It distinguishes DKMS from kernel-specific drivers and checks matching headers for detected DKMS setups. Warnings link to the Arch driver guide; no kernel, driver or boot configuration changes automatically. PCI detection needs `pciutils`.

The **Permissions** tab lists user and system Flatpaks. Choose an app, select **Override** for a permission, and use its allow/deny switch. Only selected per-user overrides are changed. The screen shows app permissions and all system/user override layers. Audio access covers speakers and microphone together; devices access includes cameras. Folder permissions can overlap, and portals retain separate permissions. Restart the app after applying. A **Permissions** button is also available on Flatpak search results; refresh the Permissions tab after installation.

Task-based search uses the local `data/search-keywords.json` mapping. For example, “video editor like Premiere” searches Kdenlive, Shotcut and DaVinci Resolve across the selected package sources. It uses no AI service and sends only mapped search terms to the normal package search sources.

**Export my system · setup-my-pc.sh…** in **System** saves a private executable script containing explicit official packages, explicit foreign/AUR packages, user/system Flatpak apps and HTTPS remotes, plus the current files known to ArchEasy's config history. You can publish the applied configs through the existing review dialog and include the resulting Gist URL. Configs are also bundled directly, so the script does not depend on a live Gist. Review the exported file for private config contents. Running it prompts before installation, bootstraps a reviewed paru build if needed, and backs up destination configs before restoring; linked destinations require manual review. This reproduces package/config selections using current repositories, not exact versions, disks, personal files, boot settings or external credentials. Foreign packages unavailable in the AUR need their original installation source.

## Config safety and everyday tools

- **Show what will change** opens a read-only dry run for built-in and imported presets, including Full Setup. It creates no backups or config files and offers no Apply action. **Config timeline…** lists dated versions and previews the diff before restoring.
- Applies touching Fish scripts syntax-check each changed `.fish` file and run `fish -c "echo ok"` with the updated config in a background task. Each check times out after ten seconds. Failure automatically restores the whole transaction, including original file modes and symlinks, reports the backup location and shows a toast. Fish must be installed to validate Fish presets; Install & Apply can install it first. Other config commands and arbitrary startup side effects are outside the file rollback.
- **About** shows the Cargo version, project GitHub link and **Check for updates**. The check requests the latest stable GitHub release only when clicked and opens download links without installing updates.
- The last five completed searches are saved privately and appear as reusable chips below the search bar. Repeated searches move to the front.
- The gear beside **Sound on / Muted** opens a saved 0–100% click-volume slider, including preview. Muting and zero volume stay silent.
- **System → Find orphans** inspects `pacman -Qdt`. Select packages and review removal in the terminal; orphan status is rechecked immediately before `sudo pacman -Rs`, which asks for final transaction confirmation and retains Pacman's normal config-backup behaviour.
- **Clean Pacman cache…** shows `/var/cache/pacman/pkg` size and uses `paccache --remove --keep 3` after confirmation, retaining the newest three cached versions of each package. Requires `pacman-contrib`; use Refresh after cleaning.

References: [Timeshift commands](https://man.archlinux.org/man/timeshift.1.en), [Snapper snapshots](https://man.archlinux.org/man/snapper.8.en), [Flatpak overrides](https://docs.flatpak.org/en/latest/flatpak-command-reference.html#flatpak-override), [NVIDIA driver selection](https://wiki.archlinux.org/title/NVIDIA), [Catppuccin Kitty themes](https://github.com/catppuccin/kitty).

## Verify

```sh
cargo test --offline
cargo clippy --offline -- -D warnings
python3 tests/setup_store_smoke.py
python3 tests/snapshot_smoke.py
luac -p presets/hypr/hyprland.lua
lua tests/validate-hyprland.lua presets/hypr/hyprland.lua
```

References: https://wiki.archlinux.org/title/Pacman , https://wiki.archlinux.org/title/Aurweb_RPC_interface , https://docs.flatpak.org/en/latest/using-flatpak.html

Config workflow references: [GitHub repository contents](https://docs.github.com/en/rest/repos/contents), [GitHub CLI Gist publishing](https://cli.github.com/manual/gh_gist_create), [GitHub sign-in](https://cli.github.com/manual/gh_auth_login), [chsh](https://man.archlinux.org/man/chsh.1).

## Releases and license

MIT licensed; see [LICENSE](LICENSE). The bundled Ribeye wordmark font uses the [SIL Open Font License 1.1](assets/fonts/Ribeye-OFL.txt). It is embedded and loaded automatically, including from standalone binaries; no separate font download or system installation is required. Third-party AppCleaner attribution is retained in [assets/AppCleaner-LICENSE](assets/AppCleaner-LICENSE); asset notes are in [assets/README.md](assets/README.md).

[CI](.github/workflows/ci.yml) fetches locked dependencies before running the offline Verify commands in an Arch Linux container. Six graphical Rust tests are intentionally excluded from headless runs.

Maintainers can build a binary archive and SHA-256 file with `bash scripts/package-release.sh` after `cargo fetch --locked`. Pushing a matching version tag (for example `v0.1.0`) runs [the release workflow](.github/workflows/release.yml), verifies the code, and publishes the archive and checksums. The workflow also supports manual dispatch for an existing tag. Presets, scripts, logo and click sound used by the app are embedded in the binary.

### Native cards and guided setup

Software search updates as you type (220 ms debounce), with All, Pacman, AUR, Flatpak, and Installed filters. Each source appears as it finishes; older queries cannot replace newer results. Press `/` to focus search, `Esc` to clear, and Down or Enter to focus results. Use arrows to select a row and Enter to request installation in your terminal. Source, available download size, and installed status appear directly in each card. Missing size metadata is labelled explicitly. Package icons use the active desktop icon theme, including Papirus names with symbolic fallbacks.

First launch offers helper setup, Danny’s configs, and an update review, with Skip. Config review shows the file count, backup location, and an expandable View Diff. Successful config application offers an Undo toast using that operation’s backup. The system update review checks package counts before enabling Start, lists the actual commands, and offers available snapshot tools. Official update counts use cached Pacman metadata and may change after refreshing. System, Light, and Dark use Libadwaita’s style manager; dark appearance uses a softer blue halo.

Run `python3 tests/native_ui_smoke.py` in a desktop session to verify live search, stale-result handling, Installed filtering, onboarding, toast actions, and update review using fake package tools and temporary settings. A preview is saved to `/tmp/archeasy-native-preview.png`.
