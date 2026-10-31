# System management delivery verification

Working tree was clean before implementation. No commits, pushes, pull requests, releases or destructive host operations were performed.

## Architecture and scope

Rust GTK4/Libadwaita page adapters embed a Python standard-library backend, following existing ArchEasy helper patterns. Navigation and the existing update entry point route into it. Structured argument arrays, bounded diagnostics, private persistent operation records, streamed terminal output, a per-user management lock and narrowly elevated snapshot/pacman commands support all three pages. Existing portable ZIP capture/unpack is reused. There are no new Rust dependencies.

See [operation-by-operation behavior and limitations](system-management.md). This is a conservative management expansion, not a complete graphical installer or automated recovery product. Automatic deletion/retention, root rollback, driver replacement, firmware flashing and AUR helper execution are intentionally guided/manual. Snapshot size and boot-layout validation are not inferred when unavailable. Remote Restic URLs, automatic Arch-news matching, exact reboot detection and a GUI Cancel action remain unimplemented. Diagnostic cancellation is covered in backend tests. Physical external-disk identity and snapshot recovery viability require manual verification.

## Added files

- `src/management.rs`: three GTK pages, diagnostic actions and terminal adapters; isolated widget construction test.
- `scripts/system-management.py`: shared backup, updater and hardware services, validation and logging.
- `tests/system_management_test.py`: 15 pure/mocked unit and integration cases.
- `docs/system-management.md`: operation guide, dependencies, permissions, safety, limitations and VM recovery drills.
- `docs/system-management-verification.md`: this delivery record.

## Modified files

- `src/main.rs`: navigation and existing system-update routing.
- `src/system_tools.rs`: official-only update review, explicit optional snapshot selection.
- `README.md`: module summary and updated system-update behavior.
- `docs/backups.md`: integration with the existing backup workflows.
- `docs/development.md`: new test command.
- `scripts/PKGBUILD`: optional dependencies and management test hook.
- `.github/workflows/ci.yml`: new backend suite as a normal user.
- `tests/native_ui_smoke.py`: isolated management page test included in graphical checks.

## Commands and results

Executed from the repository root:

| Exact command | Result |
| --- | --- |
| `python3 -W error::ResourceWarning tests/system_management_test.py` | 15 passed |
| `cargo test --offline` | 68 passed, 5 failed, 11 ignored |
| `cargo clippy --offline -- -D warnings` | Passed |
| `cargo build --offline` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tests/setup_store_smoke.py` | 6 passed |
| `python3 tests/snapshot_smoke.py` | 7 passed |
| `python3 tests/maintenance_smoke.py` | 3 passed |
| `python3 tests/gaming_smoke.py` | 17 passed |
| `python3 tests/developer_smoke.py` | 32 passed |
| `python3 tests/github_build_smoke.py` | 4 passed |
| `python3 tests/github_release_smoke.py` | 27 passed |
| `luac -p presets/hypr/hyprland.lua` | Passed |
| `lua tests/validate-hyprland.lua presets/hypr/hyprland.lua` | Passed |
| `desktop-file-validate assets/archeasy.desktop` | Passed |
| `bash -n scripts/PKGBUILD` | Passed |
| `python3 -m py_compile scripts/system-management.py tests/system_management_test.py` | Passed |
| `git diff --check` | Passed |

The seven Python smoke commands were executed together in this exact loop:

```sh
bash -c 'for t in setup_store snapshot maintenance gaming developer github_build github_release; do python3 "tests/${t}_smoke.py" || exit; done'
```

All five Rust failures are existing GitHub mock-server tests at `src/github.rs:373`, where `TcpListener::bind("127.0.0.1:0")` returns `PermissionDenied: Operation not permitted` under this environment's socket restrictions. Tests were not removed or altered to bypass that failure:

- `github::tests::checks_recipes_and_preserves_card_metadata`
- `github::tests::one_failed_recipe_does_not_discard_verified_results`
- `github::tests::repeated_searches_reuse_recipe_checks_including_missing_files`
- `github::tests::reports_api_errors_and_rejects_bad_tokens`
- `github::tests::respects_candidate_limit`

Graphical tests were not run here: no isolated Xvfb runner is installed. The new page-construction test is included in the existing graphical smoke runner and CI. Live package transactions, physical GPU/firmware checks, Snapper/Timeshift creation and real Restic repository/password interactions were not performed on the host. Use the disposable-VM checklist in the operation guide before treating these modules as release-ready.

## Sources consulted

- [Arch system maintenance and unsupported partial upgrades](https://wiki.archlinux.org/title/System_maintenance)
- [checkupdates manual](https://man.archlinux.org/man/extra/pacman-contrib/checkupdates.8.en)
- [Snapper manual and layout-dependent rollback](https://snapper.io/manpages/snapper.html)
- [Restic command manual](https://restic.readthedocs.io/en/stable/manual_rest.html)
