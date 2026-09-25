# edpcli repository instructions

## Commit hygiene

- Run `cargo fmt --all` before every commit, including commits created through remote/API tooling where local Git hooks cannot run.
- Local clones should install the repository-managed hook with `scripts/install-git-hooks.sh` on macOS/Linux or `scripts/install-git-hooks.ps1` on Windows.
- The pre-commit hook auto-formats staged Rust files and re-stages them. It fails closed for partially staged Rust files so it never stages unrelated edits.
- Keep CI `cargo fmt --all -- --check` enabled as the final repository-level guard.
- Do not bypass the hook with `--no-verify` unless the user explicitly requests it.

## Test gates and long-running validation

- Use `scripts/test-fast.sh` for routine local/AI validation. It runs formatting/diff checks plus the core suites and suites affected by the current change.
- Use `python3 scripts/test-full.py --profile full` before merge/release and after broad refactors. The full runner compiles once from Cargo JSON artifacts, runs non-HIL test binaries with bounded parallelism/per-binary timeouts, reports per-suite duration, and runs doctests separately.
- Virtual/real HIL remains separate from fast/full. Do not enable `ci-virtual-disk` in ordinary gates.
- WebCodex/AI must not serialize `cargo check --all-targets` and `cargo test --all-targets` inside one 120-second synchronous shell call. Launch the repository full runner with a >=600-second budget; when the environment returns a durable Job, observe that same Job to its final exit code instead of retrying it.

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
