# ArchEasy

A Rust + GTK4 software browser for Arch Linux. Search Pacman, AUR, and Flatpak together, filter by source, see installed packages, and start installation from a package card.

The top bar includes **About** (app version and installation information) and a **Theme** selector: System, Light, Dark, or Custom. Theme changes apply immediately and are saved in `$XDG_CONFIG_HOME/archeasy/theme` (normally `~/.config/archeasy/theme`). System uses your desktop’s GTK appearance. The **Colours…** button beside Theme opens **Customize ArchEasy theme**. Choose background, surface, text, secondary text, accent, and border colours using colour pickers or #RRGGBB values. **Apply theme** saves and selects Custom; **Cancel** leaves the saved theme unchanged. **Reset colours** stages the default palette until you apply it. Custom colours are saved in `~/.config/archeasy/custom-theme.json` (under `$XDG_CONFIG_HOME` when set).

The app background includes the Arch logo with a subtle blue halo. Buttons have rounded styling, hover glow, visible keyboard focus, and pressed feedback. **Sound on / Muted** toggles the short click sound and saves the preference in `~/.config/archeasy/sound` (or `$XDG_CONFIG_HOME/archeasy/sound`). Sound playback uses PipeWire `pw-play`, PulseAudio `paplay`, or ALSA `aplay`, whichever is available, and runs outside the UI thread.

## Run

On opening, the Arch logo scales and fades in with blue light rings and the ArchEasy wordmark, then reveals the app after 1.8 seconds. Click or press Escape to skip. The intro is omitted when desktop GTK animations are disabled.

```sh
cargo run --release
```

Requires GTK4 (4.10+), Rust, pacman, curl, and a supported terminal (Konsole, Kitty, GNOME Terminal, Xfce Terminal, or xterm). Flatpak search requires Flatpak and configured remotes. The Flathub setup button adds the official remote to your user account. AUR installs require an existing `paru` or `yay` helper; ArchEasy never runs an AUR build as root.

The Install button opens an interactive terminal. Passwords, transaction confirmation, dependency choices, and AUR build review remain in the package manager. Pacman uses `sudo pacman -Syu --needed` so installation can also update the system. Flatpak installation uses your user account. Each result has a link beside Install: **Arch package** opens the official Arch package listing filtered to that exact package name, **AUR page** opens its AUR page and build files, and Flathub results have a **Flathub page** link. Links remain available for installed packages. Close an installation terminal only after its transaction finishes. Search again afterward to refresh installed status.

Search errors appear above the results. Install errors appear in a dialog. If a terminal crashes immediately, ArchEasy tries another supported terminal and reports startup failures. After a launch failure, search again to retry. Pacman searches the locally synchronized database; AUR searches the online RPC API; Flatpak searches configured AppStream catalogs. Results are limited to 150 cards; use a more specific search for large result sets. No automatic package installation or removal occurs on app startup.

## System updates

The top-bar **System update** button opens a terminal and runs:

1. `sudo pacman -Syu` for official packages.
2. `paru -Sua` or `yay -Sua` for AUR packages only.
3. `flatpak update --user` for your account.
4. `flatpak update --system` for the default system-wide Flatpak installation.

Review the usual password, package, and build prompts in that terminal. No confirmations are bypassed. If Pacman fails or is cancelled, AUR is skipped; Flatpak steps still run independently. Missing tools, failed commands, and skipped steps appear in the final summary. A per-user lock prevents duplicate ArchEasy update sessions. Keep the terminal open until package operations finish; it waits at the summary so you can read it. Nothing updates automatically on app startup.

## AppCleaner

The **AppCleaner** tab beside **Configs** includes the standalone AppCleaner's functions: installed application discovery and search, background scans of matching settings/cache/data folders, folder sizes and partial-scan indicators, individual folder selection, a confirmation listing exact paths, and moving selected items to the desktop Trash. Restore items through your file manager's Trash. Package uninstall commands can be copied for separate review and execution; cleanup does not uninstall packages.

AppCleaner includes Cyan, Dark, Light, and Custom themes, saved separately and applied only inside its tab, plus About AppCleaner. Its **Customize theme** button opens the same six-colour editor; its custom palette is stored independently in `~/.config/archeasy/appcleaner-custom-theme.json`. Its first theme uses your standalone AppCleaner preference if available. Discovery starts on first opening the tab. Matches use exact desktop IDs/names; shared folders, top-level symlinks, and folders outside your home roots are excluded. No folders are selected automatically. Close the target app and review every path before cleanup, because application data can contain personal files. Trash failures are reported without permanent deletion.

The embedded AppCleaner code's MIT license is included in `assets/AppCleaner-LICENSE`.

## Cloud Backup

The **Cloud Backup** tab saves existing user configuration files through rclone, with Google Drive and Dropbox connection buttons and support for existing rclone accounts. Requires `rclone` on PATH. Sign-in opens in a terminal/browser; return and click **Refresh accounts** when finished. ArchEasy reads account names/types, not OAuth tokens, and does not copy the rclone credentials into its settings. The suggested list excludes the rclone config folder.

Choose an account, set a relative cloud folder (default `ArchEasy/ConfigBackups`), tick config paths or use **Add files… / Add folder…**, then choose **Save selected configs to cloud**. Nothing is selected by default. Review the exact paths and destination in the confirmation. Config files can contain passwords/tokens. Backups are uploaded as normal cloud files; encryption depends on the configured remote, such as an rclone crypt remote.

Each backup gets a unique dated folder containing `manifest.json` and `files/`, with source paths relative to your home directory. Files are staged privately, with a limit of 512 MiB and 100,000 entries. Linked paths and special files are excluded; nested links are counted as skipped. A file changing while it is copied aborts the snapshot. Close applications whose config you are backing up for a consistent result. Temporary snapshots are removed when the task finishes; failed uploads may leave a partial cloud folder, which is reported. Transfers run in the background; keep ArchEasy open until the status reports completion.

**List backups** reads the selected account/folder. **Download backup…** copies the selected snapshot into a new subfolder of a location you choose, refusing an existing destination. Downloads never apply configs automatically. Review `manifest.json` and copy the desired contents of `files/` into your home folder to restore. Existing cloud backups and local config files are not removed by these operations.

Validation uses a temporary local rclone alias remote for upload/list/download and a graphical test for selection, cancellation, upload confirmation, and backup listing. Google Drive/Dropbox browser authentication and live account transfers require your account sign-in; they are not exercised by automated tests.

References: [rclone account setup](https://rclone.org/commands/rclone_config_create/), [copy](https://rclone.org/commands/rclone_copy/), [listing](https://rclone.org/commands/rclone_lsjson/).

## Personal configs

The **Configs** tab contains seven presets based on `Projects/python/notes_configs/nix_os_config`: Fastfetch ASCII, Fastfetch image, Fish, Starship, Ghostty, Kitty, and Hyprland. Supporting images, Fish functions/colors, and Kitty theme are taken from your current config/dotfiles where the notes reference external assets. See [preset sources](presets/README.md) for the exact adaptations.

- **View config** shows files and customization instructions before applying.
- **Diff, apply & show** previews current/proposed files side by side, highlights changed lines, checks the files have not changed since review, backs up everything, then applies. Fish uses **Set up Fish** and the terminal flow below. Missing dependencies can be installed with **Install app**.
- **Show result** previews the currently installed configuration.
- **Restore backup** restores the latest un-restored backup for that app, including removing files introduced by the preset. Both Fastfetch variants share the same backup history.

Configs go under `$XDG_CONFIG_HOME` (normally `~/.config`). Backups are under `$XDG_DATA_HOME/archeasy/backups` (normally `~/.local/share/archeasy/backups`). Only the listed files change. A linked config file is backed up as a symlink and replaced locally; linked parent directories are refused to avoid modifying another config manager's files. Existing files are saved before atomic replacement; backups are private. Individual config applies preserve the login shell. Fish/stack setup can install dependencies and change the login shell only with the options you select in its review dialog. It never activates NixOS settings.

No config is applied automatically when ArchEasy starts.

### Fish and terminal stack

Each card has a green **Dependencies met** badge or a yellow list of missing apps. **Refresh dependencies** rechecks installed commands after an installation. The Fish card checks Fish and Starship. **Install app** installs missing dependencies through the usual interactive Pacman terminal.

**Set up Fish** opens a side-by-side diff before any writes. Keep **Install missing dependencies first** checked to install missing apps using `sudo pacman -Syu --needed` (includes Arch system updates). Optionally check **Make Fish my default login shell after applying**; it is off by default. One terminal checks/installs dependencies, calls ArchEasy's non-GUI apply helper, optionally runs `chsh -s /usr/bin/fish`, then holds the final summary. Passwords remain in the terminal. Installation failure prevents config application. Apply failure skips the shell change. A failed/cancelled shell change keeps the applied configs and their backup. Shell changes take effect at the next login; no logout is forced. A separate **Make Fish default** button is enabled after a Fish/stack apply and installation.

**Terminal setup · Apply all** combines Fish, Starship, Ghostty, and the Fastfetch ASCII preset, including a guarded Fish `conf.d/archeasy-starship.fish` integration file. One complete backup is captured before any config file is written. **Restore stack backup** restores all files in that snapshot, including removing introduced files; it does not uninstall packages or change the login shell. Reviews are refused if current configs changed while dependencies were being installed. Setup sessions use a per-user lock to avoid simultaneous setup terminals.

### Custom presets and Gist export

**Import current config…** copies an existing UTF-8 config file under `XDG_CONFIG_HOME` into a named local preset under `$XDG_DATA_HOME/archeasy/presets` (`~/.local/share/archeasy/presets`). It leaves the original file unchanged. Imported files are limited to 1 MiB; symlinks and linked parents are refused. Presets are private (700 directories, 600 JSON files). Imported cards support diff/apply, backup restore, and Gist export.

**Export to Gist** opens a full content review before publishing. Use **GitHub sign-in** in that dialog to authorize GitHub CLI through the browser. A user-local official GitHub CLI can be used from `~/.local/bin/gh`; **Install GitHub CLI** offers the Arch package if needed. Publishing requires the explicit **Publish Gist** action. Default visibility is secret/unlisted; checking **Publish publicly** adds `--public`. Secret Gists are accessible to anyone with the URL: review passwords, tokens, and private data before publishing. Credentials are handled by GitHub CLI, not entered into ArchEasy. The completed URL can be copied from the dialog. Binary assets are base64-encoded with a manifest mapping files back to config paths. Temporary export files are removed after the CLI returns.

### Hyprland

**Hyprland · Desktop preset** adapts `Projects/dotfiles/dotfiles/hypr/hyprland.conf` to modern `hypr/hyprland.lua` using the [upstream 0.56.2 example](https://github.com/hyprwm/Hyprland/blob/v0.56.2/example/hyprland.lua). It preserves your gaps, rounded corners, blur, blue/pink borders, animations, and core shortcuts. Duplicate shortcuts, external pywal/AGS paths, and hard-coded wallpapers are omitted. Terminal and file-manager commands have installed-program fallbacks. Optional startup commands are guarded.

Use **Install Hyprland & tools** to start an interactive Pacman installation of the compositor and its companion tools. **Apply config** saves a backup and writes the Lua file; **Setup guide** explains dependencies, shortcuts, and how to choose Hyprland at login. It does not start a nested compositor or log you out. Active Hyprland sessions may automatically reload the new file. Existing legacy `.conf` files are preserved. Hyprland is not installed on this machine, so validation covers Lua syntax, preset structure, shortcuts, and backup/restore; compositor behavior still needs testing in a Hyprland session.

## Verify

```sh
cargo test --offline
cargo clippy --offline -- -D warnings
luac -p presets/hypr/hyprland.lua
lua tests/validate-hyprland.lua presets/hypr/hyprland.lua
```

References: https://wiki.archlinux.org/title/Pacman , https://wiki.archlinux.org/title/Aurweb_RPC_interface , https://docs.flatpak.org/en/latest/using-flatpak.html

Config workflow references: [GitHub CLI Gist publishing](https://cli.github.com/manual/gh_gist_create), [GitHub sign-in](https://cli.github.com/manual/gh_auth_login), [chsh](https://man.archlinux.org/man/chsh.1).
