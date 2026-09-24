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

## WebCodex real-time progress log

- Every new WebCodex work session MUST create a new session log under `audit/ai-progress/` before starting substantive analysis, editing, testing, or release work. Never reuse a previous session's log file.
- File name format: `YYYYMMDD-HHMMSS-<wc_sess_id>.log`. If the WebCodex session id is unavailable, use `YYYYMMDD-HHMMSS-manual.log`.
- Append progress continuously after each meaningful action or verified finding so the user can watch it with `tail -f`; do not wait until the final report to backfill the log.
- Each line MUST use local machine time and the format `[YYYY-MM-DD HH:MM:SS TZ] [TAG] message`.
- Use concise factual tags such as `[START]`, `[WORK]`, `[FOUND]`, `[DECISION]`, `[PASS]`, `[FAIL]`, `[VALIDATION]`, `[GIT]`, `[REVIEW]`, `[NEXT]`, `[BLOCKED]`, and `[DONE]`.
- Record only observable actions, verified findings, validation results, Git state, blockers, and next steps. Do not write hidden chain-of-thought or speculative reasoning into the progress log.
- Runtime `.log` files are intentionally Git-ignored. The durable format and behavior contract is documented in `audit/ai-progress/README.md` and must remain tracked.
