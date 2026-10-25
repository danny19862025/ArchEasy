# ArchEasy

A Rust + GTK4 software browser for Arch Linux. Search Pacman, AUR, and Flatpak together, filter by source, see installed packages, and start installation from a package card.

The top bar includes **About** (app version and installation information) and a **Theme** selector: System, Light, or Dark. Theme changes apply immediately and are saved in `$XDG_CONFIG_HOME/archeasy/theme` (normally `~/.config/archeasy/theme`). System uses your desktop’s GTK appearance.

The app background includes the Arch logo with a subtle blue halo. Buttons have rounded styling, hover glow, visible keyboard focus, and pressed feedback. **Sound on / Muted** toggles the short click sound and saves the preference in `~/.config/archeasy/sound` (or `$XDG_CONFIG_HOME/archeasy/sound`). Sound playback uses PipeWire `pw-play`, PulseAudio `paplay`, or ALSA `aplay`, whichever is available, and runs outside the UI thread.

## Run

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

## Personal configs

The **Configs** tab contains seven presets based on `Projects/python/notes_configs/nix_os_config`: Fastfetch ASCII, Fastfetch image, Fish, Starship, Ghostty, Kitty, and Hyprland. Supporting images, Fish functions/colors, and Kitty theme are taken from your current config/dotfiles where the notes reference external assets. See [preset sources](presets/README.md) for the exact adaptations.

- **View config** shows files and customization instructions before applying.
- **Apply & show** writes the preset and opens a new shell, terminal, or command preview. Missing applications can be installed with **Install app**. Optional Fish commands are checked before use.
- **Show result** previews the currently installed configuration.
- **Restore backup** restores the latest un-restored backup for that app, including removing files introduced by the preset. Both Fastfetch variants share the same backup history.

Configs go under `$XDG_CONFIG_HOME` (normally `~/.config`). Backups are under `$XDG_DATA_HOME/archeasy/backups` (normally `~/.local/share/archeasy/backups`). Only the listed files change. A linked config file is backed up as a symlink and replaced locally; linked parent directories are refused to avoid modifying another config manager's files. Existing files are saved before atomic replacement; backups are private. Applying a preset does not change the login shell, activate NixOS settings, or install packages automatically.

No config is applied automatically when ArchEasy starts.

### Hyprland

**Hyprland · Danny’s desktop** adapts `Projects/dotfiles/dotfiles/hypr/hyprland.conf` to modern `hypr/hyprland.lua` using the [upstream 0.56.2 example](https://github.com/hyprwm/Hyprland/blob/v0.56.2/example/hyprland.lua). It preserves your gaps, rounded corners, blur, blue/pink borders, animations, and core shortcuts. Duplicate shortcuts, external pywal/AGS paths, and hard-coded wallpapers are omitted. Terminal and file-manager commands have installed-program fallbacks. Optional startup commands are guarded.

Use **Install Hyprland & tools** to start an interactive Pacman installation of the compositor and its companion tools. **Apply config** saves a backup and writes the Lua file; **Setup guide** explains dependencies, shortcuts, and how to choose Hyprland at login. It does not start a nested compositor or log you out. Active Hyprland sessions may automatically reload the new file. Existing legacy `.conf` files are preserved. Hyprland is not installed on this machine, so validation covers Lua syntax, preset structure, shortcuts, and backup/restore; compositor behavior still needs testing in a Hyprland session.

## Verify

```sh
cargo test --offline
cargo clippy --offline -- -D warnings
luac -p presets/hypr/hyprland.lua
lua tests/validate-hyprland.lua presets/hypr/hyprland.lua
```

References: https://wiki.archlinux.org/title/Pacman , https://wiki.archlinux.org/title/Aurweb_RPC_interface , https://docs.flatpak.org/en/latest/using-flatpak.html
