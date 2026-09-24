# edpcli repository instructions

## Commit hygiene

- Run `cargo fmt --all` before every commit, including commits created through remote/API tooling where local Git hooks cannot run.
- Local clones should install the repository-managed hook with `scripts/install-git-hooks.sh` on macOS/Linux or `scripts/install-git-hooks.ps1` on Windows.
- The pre-commit hook auto-formats staged Rust files and re-stages them. It fails closed for partially staged Rust files so it never stages unrelated edits.
- Keep CI `cargo fmt --all -- --check` enabled as the final repository-level guard.
- Do not bypass the hook with `--no-verify` unless the user explicitly requests it.

## Local developer install on macOS

- The user's active edpcli installation is the user-local binary at `~/.local/bin/edpcli`. Do not install development/test builds to `/usr/local/bin/edpcli` unless the user explicitly asks for a system-wide install.
- Never decide the install target from the automation runner's plain `command -v edpcli`; its PATH can differ from the user's interactive shell. Resolve the user's actual command with `zsh -lic 'command -v edpcli'`.
- For a locally built test version, use `scripts/install-local.sh [path-to-binary]` instead of calling `install` directly.
- After installation, verify both: (1) `zsh -lic 'command -v edpcli'` resolves to `$HOME/.local/bin/edpcli`; and (2) the source and installed binary SHA-256 values are identical.
