#!/usr/bin/env bash
set -euo pipefail

image="${TMPDIR:-/tmp}/edpcli-plain-virtual-hil-$$.img"
device=""
mount_point=""

cleanup() {
  set +e
  if [[ -n "$mount_point" && -d "$mount_point" ]]; then
    diskutil unmount "$mount_point" >/dev/null 2>&1 || true
  fi
  if [[ -n "$device" ]]; then
    diskutil eject "$device" >/dev/null 2>&1 || true
  fi
  rm -f "$image"
}
trap cleanup EXIT

truncate -s 256m "$image"
attach="$(hdiutil attach -nomount -imagekey diskimage-class=CRawDiskImage "$image")"
device="$(printf '%s\n' "$attach" | awk 'NR==1{print $1}')"
[[ "$device" =~ ^/dev/disk[0-9]+$ ]] || {
  echo "unexpected disk image device: $device" >&2
  exit 1
}
raw="/dev/r${device#/dev/}"

export EDPCLI_PLAIN_VIRTUAL_DISK_PATH="$raw"
export CARGO_TARGET_DIR="${TMPDIR:-/tmp}/edpcli-plain-virtual-hil-target"
cargo_bin="$(zsh -lic 'command -v cargo')"
[[ -x "$cargo_bin" ]] || {
  echo "cargo not found in user login shell" >&2
  exit 1
}
"$cargo_bin" test --features ci-virtual-disk --test plain_macos_virtual_hil -- --ignored --nocapture

# Force detach/reattach so OS parsing cannot succeed from the same raw handle/cache.
diskutil eject "$device" >/dev/null
device=""
attach="$(hdiutil attach -nomount -imagekey diskimage-class=CRawDiskImage "$image")"
device="$(printf '%s\n' "$attach" | awk 'NR==1{print $1}')"
partition="${device}s1"

for _ in $(seq 1 30); do
  if diskutil info "$partition" >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done
diskutil info "$partition" >/dev/null

diskutil mount "$partition" >/dev/null
mount_point="$(diskutil info -plist "$partition" | plutil -extract MountPoint raw -o - -)"
[[ -n "$mount_point" && -d "$mount_point" ]] || {
  echo "Plain exFAT partition did not mount" >&2
  exit 1
}
marker="$mount_point/edpcli-virtual-hil.txt"
printf 'edpcli-plain-virtual-hil\n' > "$marker"
sync
[[ "$(cat "$marker")" == "edpcli-plain-virtual-hil" ]] || {
  echo "Plain exFAT file readback mismatch" >&2
  exit 1
}
diskutil unmount "$partition" >/dev/null
mount_point=""

echo "macOS Plain virtual-disk HIL PASS: $device ($partition)"
