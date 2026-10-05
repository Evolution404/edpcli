#!/usr/bin/env bash
set -euo pipefail

image="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/edpcli-virtual-hil-$$.img"
device=""
mount_point=""

attach_image() {
  if diskutil image --help >/dev/null 2>&1; then
    diskutil image attach --noMount "$image"
  else
    hdiutil attach -nomount -imagekey diskimage-class=CRawDiskImage "$image"
  fi
}

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

truncate -s 128m "$image"
attach="$(attach_image)"
device="$(printf '%s\n' "$attach" | awk 'NR==1{print $1}')"
[[ "$device" =~ ^/dev/disk[0-9]+$ ]] || {
  echo "unexpected disk image device: $device" >&2
  exit 1
}

diskutil partitionDisk "$device" 1 MBR FAT32 EDPCLI_HIL R >/dev/null
partition="${device}s1"
mount_point="$(diskutil info -plist "$partition" | plutil -extract MountPoint raw -o - -)"
[[ -n "$mount_point" && -d "$mount_point" ]] || {
  echo "virtual HIL partition did not mount" >&2
  exit 1
}
printf 'edpcli-virtual-hil\n' > "$mount_point/marker.txt"
sync

total_bytes="$(diskutil info -plist "$device" | plutil -extract TotalSize raw -o - -)"
export EDPCLI_VIRTUAL_DISK_PATH="/dev/r${device#/dev/}"
export EDPCLI_VIRTUAL_DISK_SECTORS="$((total_bytes / 512))"
export CARGO_TARGET_DIR="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/edpcli-virtual-hil-target"
cargo_bin="$(zsh -lic 'command -v cargo')"
[[ -x "$cargo_bin" ]] || {
  echo "cargo not found in user login shell" >&2
  exit 1
}

"$cargo_bin" test --features ci-virtual-disk --test virtual_disk_hil -- --ignored --nocapture

# Force detach/reattach so the restoration proof does not rely on the same raw
# handle or filesystem cache. The Rust HIL restores LBA0-12 bit-for-bit.
mount_point=""
diskutil eject "$device" >/dev/null
device=""
attach="$(attach_image)"
device="$(printf '%s\n' "$attach" | awk 'NR==1{print $1}')"
[[ "$device" =~ ^/dev/disk[0-9]+$ ]] || {
  echo "unexpected reattached disk image device: $device" >&2
  exit 1
}
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
[[ "$(cat "$mount_point/marker.txt")" == "edpcli-virtual-hil" ]] || {
  echo "filesystem marker was not preserved after raw roundtrip" >&2
  exit 1
}

echo "macOS virtual-disk HIL PASS: $device ($partition)"
