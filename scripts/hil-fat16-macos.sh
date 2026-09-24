#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
  echo "This HIL runs on macOS only." >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/edpcli-fat16-hil.XXXXXX")"
disk=""
cleanup() {
  if [[ -n "$disk" ]]; then
    diskutil unmountDisk "$disk" >/dev/null 2>&1 || true
    hdiutil detach "$disk" >/dev/null 2>&1 || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT

cd "$repo_root"
cargo run --quiet --example hil_provision_front -- "$scratch/boot.img" fat16
attached="$(hdiutil attach -nomount -imagekey diskimage-class=CRawDiskImage "$scratch/boot.img")"
disk="$(printf '%s\n' "$attached" | awk '/^\/dev\/disk[0-9]+[[:space:]]/ { print $1; exit }')"
if [[ -z "$disk" ]]; then
  echo "hdiutil did not expose a whole disk: $attached" >&2
  exit 1
fi
partition="${disk}s1"
fsck_msdos -n "/dev/r${partition#/dev/}"
info="$(diskutil info "$partition")"
printf '%s\n' "$info" | grep -Eq 'File System Personality:[[:space:]]*(MS-DOS FAT16|FAT16)'
printf '%s\n' "$info" | grep -Eq 'Volume Name:[[:space:]]*启动区'
diskutil mount "$partition"
mount_point="$(diskutil info -plist "$partition" | plutil -extract MountPoint raw -o - -)"
printf 'edpcli FAT16 HIL\n' > "$mount_point/edpcli-hil.txt"
printf 'edpcli FAT16 HIL\n' | cmp - "$mount_point/edpcli-hil.txt"
diskutil unmount "$partition"
diskutil mount "$partition"
mount_point="$(diskutil info -plist "$partition" | plutil -extract MountPoint raw -o - -)"
printf 'edpcli FAT16 HIL\n' | cmp - "$mount_point/edpcli-hil.txt"
diskutil unmount "$partition"
echo "FAT16 native mount, label and file round-trip passed on $partition"
