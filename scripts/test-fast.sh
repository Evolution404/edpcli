#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

cargo fmt --all -- --check
git diff --check
exec python3 scripts/test-full.py --profile fast "$@"
