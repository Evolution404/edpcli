# edpcli repository instructions

## Commit hygiene

- Run `cargo fmt --all` before every commit, including commits created through remote/API tooling where local Git hooks cannot run.
- Local clones should install the repository-managed hook with `scripts/install-git-hooks.sh` on macOS/Linux or `scripts/install-git-hooks.ps1` on Windows.
- The pre-commit hook auto-formats staged Rust files and re-stages them. It fails closed for partially staged Rust files so it never stages unrelated edits.
- Keep CI `cargo fmt --all -- --check` enabled as the final repository-level guard.
- Do not bypass the hook with `--no-verify` unless the user explicitly requests it.
