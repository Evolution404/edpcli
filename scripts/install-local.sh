#!/bin/sh
set -eu

SOURCE="${1:-target/release/edpcli}"
TARGET="${HOME}/.local/bin/edpcli"

fail() {
    printf 'install-local: %s\n' "$*" >&2
    exit 1
}

[ -f "$SOURCE" ] || fail "source binary not found: $SOURCE"
[ -x "$SOURCE" ] || fail "source binary is not executable: $SOURCE"

mkdir -p "${TARGET%/*}"
install -m 0755 "$SOURCE" "$TARGET"

source_sha="$(shasum -a 256 "$SOURCE" | awk '{print $1}')"
target_sha="$(shasum -a 256 "$TARGET" | awk '{print $1}')"
[ "$source_sha" = "$target_sha" ] || fail "SHA-256 mismatch after install"

if command -v zsh >/dev/null 2>&1; then
    resolved="$(zsh -lic 'command -v edpcli' 2>/dev/null || true)"
    [ "$resolved" = "$TARGET" ] || {
        printf 'install-local: interactive zsh resolves edpcli to: %s\n' "${resolved:-<not found>}" >&2
        zsh -lic 'type -a edpcli' 2>/dev/null >&2 || true
        fail "interactive shell is not using $TARGET"
    }
fi

printf 'installed: %s\n' "$TARGET"
printf 'sha256:   %s\n' "$target_sha"
"$TARGET" --version
