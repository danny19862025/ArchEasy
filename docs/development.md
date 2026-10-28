# Development and releases

[Back to README](../README.md)

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

MIT licensed; see [LICENSE](../LICENSE). The bundled Ribeye wordmark font uses the [SIL Open Font License 1.1](../assets/fonts/Ribeye-OFL.txt). It is embedded and loaded automatically, including from standalone binaries; no separate font download or system installation is required. Third-party AppCleaner attribution is retained in [assets/AppCleaner-LICENSE](../assets/AppCleaner-LICENSE); asset notes are in [assets/README.md](../assets/README.md).

[CI](../.github/workflows/ci.yml) fetches locked dependencies before running the offline Verify commands in an Arch Linux container. Six graphical Rust tests run separately under Xvfb with a private D-Bus session and software rendering. Every test gets isolated settings/data directories; the ordinary Rust run skips those graphical cases and the live GitHub API test.

Maintainers can build a binary archive and SHA-256 file with `bash scripts/package-release.sh` after `cargo fetch --locked`. Pushing a matching version tag (for example `v0.1.0`) runs [the release workflow](../.github/workflows/release.yml), verifies the code, and publishes the archive and checksums. The workflow also supports manual dispatch for an existing tag. Presets, scripts, logo and click sound used by the app are embedded in the binary.

### Native cards and guided setup

Software search updates as you type (220 ms debounce; 900 ms for All/GitHub to reduce API requests), with All, Pacman, AUR, Flatpak, GitHub, and Installed filters. Each source appears as it finishes; older queries cannot replace newer results. Press `/` to focus search, `Esc` to clear, and Down or Enter to focus results. Use arrows to select a row and Enter to request installation in your terminal. Source, available download size, and installed status appear directly in each card. Missing size metadata is labelled explicitly. Package icons use the active desktop icon theme, including Papirus names with symbolic fallbacks.

First launch offers helper setup, Danny’s configs, and an update review, with Skip. Config review shows the file count, backup location, and an expandable View Diff. Successful config application offers an Undo toast using that operation’s backup. The system update review checks package counts before enabling Start, lists the actual commands, and offers available snapshot tools. Official update counts use cached Pacman metadata and may change after refreshing. System, Light, and Dark use Libadwaita’s style manager; dark appearance uses a softer blue halo.

Run `python3 tests/native_ui_smoke.py` in a desktop session to verify live search, stale-result handling, Installed filtering, onboarding, toast actions, and update review using fake package tools and temporary settings. A preview is saved to `/tmp/archeasy-native-preview.png`.

If a repository database error prevents Pacman from separating native and foreign packages, Installed falls back to its local package list. The Pacman circle includes AUR/local entries, the AUR circle reads N/A with “Included in Pacman”, and a status message explains the repository error. Source percentages return after repairing repository metadata and refreshing.

## Release audit (28 October 2026)

The original [v0.1.0 tag run](https://github.com/danny19862025/ArchEasy/actions/runs/40286400807) failed in `cargo test --offline`. A subsequent [manual release run](https://github.com/danny19862025/ArchEasy/actions/runs/40287055839) succeeded. [v0.1.0](https://github.com/danny19862025/ArchEasy/releases/tag/v0.1.0) has its binary archive and checksum; [v0.1.2](https://github.com/danny19862025/ArchEasy/releases/tag/v0.1.2) is also published, with binary and Arch package assets. Download names in the README now use that published version. Local source fixes are not included in those existing assets until a new release is built and published.
