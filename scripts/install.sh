#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT"

if [ -n "$(git status --porcelain 2>/dev/null || true)" ]; then
    printf 'install: warning: worktree has uncommitted changes; installing current working tree
' >&2
fi

printf 'install: building release from %s
' "$(git rev-parse --short=12 HEAD 2>/dev/null || printf unknown)"
cargo build --release --locked

exec "$ROOT/scripts/install-local.sh" "$ROOT/target/release/edpcli"
