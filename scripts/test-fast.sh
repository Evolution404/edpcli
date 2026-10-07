#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

# The owned watchdog includes formatting, Clippy and the runner; worker avoids recursion.
if [ "${1:-}" != "--gate-worker" ]; then
    exec uv run --locked python scripts/test_gate.py --phase fast -- sh "$0" --gate-worker "$@"
fi
shift
fast_budget=$(uv run --locked python scripts/test_gate.py --budget-key fast_max_seconds) # default 60

cargo fmt --all -- --check
git diff --check
uv run --locked python scripts/test-change-scope.py

changed_paths=$(uv run --locked python scripts/test-full.py --list-changed-paths)

if printf '%s\n' "$changed_paths" | grep -Eq '(^|/)[^/]+\.rs$|^Cargo\.(toml|lock)$|^rust-toolchain(\.toml)?$|^\.cargo/'; then
    cargo clippy --all-targets --locked -- -D warnings
else
    echo "[fast] clippy skipped: no Rust/Cargo inputs changed"
fi

echo "[gate] table-scroll: all table kinds / both edges / active headers / renderer contract"
cargo test --locked --test tui_suite table_scroll_gate -- --test-threads=1

exec uv run --locked python scripts/test-full.py --profile fast --max-seconds "${EDPCLI_FAST_MAX_SECONDS:-$fast_budget}" "$@"
