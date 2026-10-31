# Gaming implementation validation

Validation performed in the restricted development environment on 2026-10-31. No real package installation, privileged configuration edit, commit, or push was performed.

| Check | Result |
| --- | --- |
| `python3 tests/gaming_smoke.py` | 17 passed, using fixtures and fake transactions |
| `cargo test --offline` | 66 passed, 5 failed, 8 ignored |
| Rust suite excluding the five socket-dependent GitHub tests | 66 passed, 8 ignored |
| `cargo clippy --offline -- -D warnings` | Passed |
| `cargo build --release --offline --locked` | Passed |
| `cargo fmt --check`, `git diff --check` | Passed |
| Existing setup/store Python suite | 6 passed |
| Existing snapshot Python suite | 7 passed |
| Existing maintenance Python suite | 3 passed |
| Existing GitHub-build Python suite | 4 passed |
| Lua syntax and Hyprland validation | Passed; 44 unique shortcuts/settings |
| Desktop-file validation | Passed |
| PKGBUILD/install-desktop/package-release Bash syntax | Passed |
| `python3 tests/native_ui_smoke.py` | Blocked at GTK initialization; no usable display in the sandbox |

The five Rust failures all occur at `src/github.rs:373`, where the existing test fixture calls `TcpListener::bind` and receives `PermissionDenied: Operation not permitted`. They are:

- `github::tests::checks_recipes_and_preserves_card_metadata`
- `github::tests::one_failed_recipe_does_not_discard_verified_results`
- `github::tests::repeated_searches_reuse_recipe_checks_including_missing_files`
- `github::tests::respects_candidate_limit`
- `github::tests::reports_api_errors_and_rejects_bad_tokens`

The graphical suite stops at its first existing test with `Failed to initialize GTK`. The new Gaming widget test is registered in that suite but could not be exercised here. CI retains all tests without exclusions.

The backend tests cover recommended catalog metadata, Arch-family detection, GPU/driver mappings and hybrid GPUs, native/Flatpak status and fallback planning, unavailable packages, rejection of third-party repositories and unexpected Flathub URLs, explicit permissions, Pacman lock checks, shared lock contention, native-before-Flatpak ordering, retry skipping, private PTY prompt streaming without a newline, cancellation while responding to a prompt, command failure, disconnected input, multilib backup/mode preservation and stale-file/backup refusal, multilib permission refusal, NVIDIA version compatibility, and System Check output/unsupported graphics stacks. The embedded Rust process/protocol test also passes.

Before release, run the full Rust and graphical suites in CI or an unrestricted development environment. Verify live polkit authentication and dismissal, Pacman/Flatpak transaction prompts in the response field, multilib edits and retained backup, mirrors/network failures, cancellation and retries, supported GPU and hybrid systems including 32-bit Vulkan games, small-window layout, and the completion summary on disposable Arch installations. See [Gaming setup](gaming.md) for package fallbacks, safety behavior, and supported hardware limits.
