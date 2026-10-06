#!/bin/sh
set -eu

SOURCE="${1:-target/release/edpcli}"
TARGET="${HOME}/.local/bin/edpcli"
CANDIDATE=''
BACKUP=''
PUBLISHED=0
LOCK="${TARGET%/*}/.edpcli-install.lock"

fail() { printf 'install-local: %s\n' "$*" >&2; exit 1; }
cleanup() {
    result=$?
    if [ "$result" -ne 0 ] && [ "$PUBLISHED" = 1 ]; then
        if [ -n "$BACKUP" ]; then
            if mv -f "$BACKUP" "$TARGET"; then BACKUP='';
            else
                printf 'install-local: rollback failed; previous binary retained at %s\n' "$BACKUP" >&2
                BACKUP=''
            fi
        else rm -f "$TARGET"; fi
    fi
    [ -z "$CANDIDATE" ] || rm -f "$CANDIDATE"
    [ -z "$BACKUP" ] || rm -f "$BACKUP"
    rmdir "$LOCK" 2>/dev/null || true
}
resolve_active() {
    if command -v zsh >/dev/null 2>&1; then
        zsh -lic 'command -v edpcli' 2>/dev/null || true
    else
        "${SHELL:-/bin/sh}" -lc 'command -v edpcli' 2>/dev/null || true
    fi
}
interactive_paths() {
    if command -v zsh >/dev/null 2>&1; then zsh -lic 'print -l $path' 2>/dev/null
    else "${SHELL:-/bin/sh}" -lc 'printf "%s\n" "$PATH"' 2>/dev/null | tr ':' '\n'; fi
}

[ -f "$SOURCE" ] || fail "source binary not found: $SOURCE"
[ -x "$SOURCE" ] || fail "source binary is not executable: $SOURCE"
if [ "$(uname -s)" = Darwin ]; then
    command -v zsh >/dev/null 2>&1 || fail 'zsh is required to verify the macOS interactive command'
fi
mkdir -p "${TARGET%/*}"
mkdir "$LOCK" 2>/dev/null || fail 'another local installation is running'
trap cleanup 0
trap 'exit 130' HUP INT TERM
resolved="$(resolve_active)"
if [ -n "$resolved" ]; then
    [ "$resolved" = "$TARGET" ] || fail "interactive shell resolves edpcli to $resolved"
else
    interactive_paths | awk -v target="${TARGET%/*}" '$0 == target {found=1} END {exit !found}' || fail "interactive PATH does not contain ${TARGET%/*}"
fi
source_sha="$(shasum -a 256 "$SOURCE" | awk '{print $1}')"
CANDIDATE="$(mktemp "${TARGET%/*}/.edpcli-candidate.XXXXXX")"
install -m 0755 "$SOURCE" "$CANDIDATE"
version="$("$CANDIDATE" --version)" || fail 'candidate --version failed'
candidate_sha="$(shasum -a 256 "$CANDIDATE" | awk '{print $1}')"
[ "$source_sha" = "$candidate_sha" ] || fail 'candidate SHA-256 mismatch'
if [ -e "$TARGET" ] || [ -L "$TARGET" ]; then
    BACKUP="$(mktemp "${TARGET%/*}/.edpcli-previous.XXXXXX")"
    cp -Pp "$TARGET" "$BACKUP"
fi
mv -f "$CANDIDATE" "$TARGET"
CANDIDATE=''
PUBLISHED=1
resolved="$(resolve_active)"
[ "$resolved" = "$TARGET" ] || fail "interactive shell changed to ${resolved:-<not found>}; rolling back"
target_sha="$(shasum -a 256 "$TARGET" | awk '{print $1}')"
[ "$source_sha" = "$target_sha" ] || fail 'published SHA-256 mismatch; rolling back'
printf 'installed: %s\nsha256:   %s\n%s\n' "$TARGET" "$target_sha" "$version"
