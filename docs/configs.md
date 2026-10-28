# Config presets, sharing and history

[Back to README](../README.md)

## Personal configs

The **Configs** tab includes Fastfetch ASCII/image, Fish, Starship, Ghostty, Kitty, Hyprland, and starter presets for Neovim, Tmux, Waybar, Rofi, Wofi and btop. Original preset sources are documented in [presets/README.md](../presets/README.md).

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

**Community presets…** reads `presets/community.json`, or your override at `~/.config/archeasy/community.json`. The starter catalog includes official Catppuccin Mocha/Latte Kitty themes and the bundled Danny Hyprland preset. Entries can be ArchEasy Gist URLs or objects with `name`, `description`, `url`, and optional GitHub file `target` and `dependencies`. Downloaded files are saved as local presets and reviewed through the same diff, dependency, backup and conflict flows. Kitty themes add an include to the existing Kitty config. Downloads do not execute remote scripts. GitHub file entries require a full commit SHA, and downloads use that exact revision. The bundled Catppuccin entries are pinned. Community Gist/JSON entries require a `sha256` of the canonical imported plan JSON; a mismatch stops the download before saving or applying.


## Publishing checks

Before publishing to either a Gist or repository, ArchEasy scans for private-key headers, common GitHub/Slack/OpenAI/AWS token patterns, credential filenames, and nonempty password/token/key assignments. Findings block publishing and report only file names and line numbers. Remove credentials from the preset and review again. This heuristic scan cannot detect every secret; unlisted Gists remain readable by anyone with the URL.

Binary config bytes are preserved through apply, history, ZIP transfer and the exported restore helper. Path substitutions apply only to valid UTF-8 without NUL bytes, independently of filename extensions.

For a community Gist/JSON pin, hash the compact canonical `Plan::json()` representation (the exact `id`, `name`, `files` byte arrays and `dependencies` imported by ArchEasy, with Rust serde_json’s sorted object keys). Recurate and update the hash when intentionally changing the preset. Manual Import URL remains a separate review flow.
