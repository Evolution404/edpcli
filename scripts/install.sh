#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT"

CARGO_BIN="${CARGO:-}"
if [ -z "$CARGO_BIN" ]; then
    if command -v cargo >/dev/null 2>&1; then
        CARGO_BIN="$(command -v cargo)"
    elif [ -x "${HOME:-}/.cargo/bin/cargo" ]; then
        CARGO_BIN="${HOME}/.cargo/bin/cargo"
    else
        printf 'install: cargo not found; install Rust or set CARGO=/path/to/cargo\n' >&2
        exit 127
    fi
fi

if [ -n "$(git status --porcelain 2>/dev/null || true)" ]; then
    printf 'install: warning: worktree has uncommitted changes; installing current working tree
' >&2
fi

printf 'install: building release from %s
' "$(git rev-parse --short=12 HEAD 2>/dev/null || printf unknown)"
"$CARGO_BIN" build --release --locked

exec "$ROOT/scripts/install-local.sh" "$ROOT/target/release/edpcli"
