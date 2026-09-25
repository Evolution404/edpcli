#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

cargo fmt --all -- --check
git diff --check
cargo clippy --all-targets --locked -- -D warnings
exec python3 scripts/test-full.py --profile fast --max-seconds "${EDPCLI_FAST_MAX_SECONDS:-45}" "$@"
