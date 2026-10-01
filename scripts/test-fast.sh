#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

cargo fmt --all -- --check
git diff --check

changed_paths=$(git diff --name-only HEAD)
if [ -z "$changed_paths" ]; then
    changed_paths=$(git diff-tree --no-commit-id --name-only -r HEAD 2>/dev/null || true)
fi

if printf '%s\n' "$changed_paths" | grep -Eq '(^|/)[^/]+\.rs$|^Cargo\.(toml|lock)$|^rust-toolchain(\.toml)?$|^\.cargo/'; then
    cargo clippy --all-targets --locked -- -D warnings
else
    echo "[fast] clippy skipped: no Rust/Cargo inputs changed"
fi

echo "[gate] table-scroll: all table kinds / both edges / active headers / renderer contract"
cargo test --locked --test tui_suite table_scroll_gate -- --test-threads=1

exec python3 scripts/test-full.py --profile fast --max-seconds "${EDPCLI_FAST_MAX_SECONDS:-60}" "$@"
