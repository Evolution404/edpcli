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

CURRENT_VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
CURRENT_BRANCH="$(git symbolic-ref --short -q HEAD 2>/dev/null || printf detached)"
printf 'install: current workspace version=%s branch=%s commit=%s
' \
    "$CURRENT_VERSION" "$CURRENT_BRANCH" "$(git rev-parse --short=12 HEAD 2>/dev/null || printf unknown)"
printf 'install: building release profile from current working tree (no git pull)
'
"$CARGO_BIN" build --release --locked

exec "$ROOT/scripts/install-local.sh" "$ROOT/target/release/edpcli"
