#!/usr/bin/env bash
set -euo pipefail
directory="$(mktemp -d /tmp/edpcli-4kn-hil.XXXXXX)"
image="$directory/disk.img"
device=""
cleanup() {
  set +e
  if [[ -n "$device" ]]; then diskutil eject "$device" >/dev/null 2>&1 || true; fi
  rm -rf "$directory"
}
trap cleanup EXIT
mkfile -n 512m "$image"
verify_identity() {
  diskutil info -plist "$device" | plutil -convert json -o - - | python3 -c '
import json,sys
v=json.load(sys.stdin)
assert v.get("WholeDisk") is True
assert v.get("Internal") is False
assert v.get("VirtualOrPhysical")=="Virtual"
assert v.get("BusProtocol")=="Disk Image"
assert v.get("DeviceBlockSize")==4096
assert v.get("TotalSize")==536870912
'
  image_inventory="$(hdiutil info)"
  [[ "$image_inventory" == *"$image"* ]] || { echo "test image not attached" >&2; exit 1; }
}
attach() {
  output="$(hdiutil attach -nomount -noverify -blocksize 4096 "$image")"
  device="$(printf '%s\n' "$output" | awk 'NR==1{print $1}')"
  [[ "$device" =~ ^/dev/disk[0-9]+$ ]] || exit 1
  verify_identity
}
cargo test --features ci-virtual-disk --test native_macos_4kn_virtual_hil --no-run
testbin="$(find target/debug/deps -maxdepth 1 -type f -name 'native_macos_4kn_virtual_hil-*' -perm -111 -print | head -n 1)"
[[ -x "$testbin" ]]
bin="$(pwd)/$testbin"
run_stage() {
  action="$1"; mode="$2"; wal="$3"
  raw="/dev/r$(basename "$device")"
  verify_identity
  if [[ -r "$raw" && -w "$raw" ]]; then
    env EDPCLI_4KN_HIL_RAW="$raw" EDPCLI_4KN_HIL_BYTES=536870912 EDPCLI_4KN_HIL_MODE="$mode" EDPCLI_4KN_HIL_ACTION="$action" EDPCLI_4KN_HIL_WAL="$wal" "$bin" --ignored --exact macos_4kn_native_full_block_hil --nocapture
  else
    sudo -n env EDPCLI_4KN_HIL_RAW="$raw" EDPCLI_4KN_HIL_BYTES=536870912 EDPCLI_4KN_HIL_MODE="$mode" EDPCLI_4KN_HIL_ACTION="$action" EDPCLI_4KN_HIL_WAL="$wal" "$bin" --ignored --exact macos_4kn_native_full_block_hil --nocapture
  fi
}
attach
for mode in Plain Mode0 Mode1 Mode2 Mode3 Plain; do
  echo "4Kn virtual block target=$mode on $device"
  wal="$directory/$mode-$(date +%s).wal"
  run_stage write "$mode" "$wal"
  diskutil eject "$device" >/dev/null
  device=""
  attach
  run_stage verify "$mode" "$wal"
  if [[ "$mode" == "Plain" ]]; then
    partition="$(printf '%ss1' "$device")"
    diskutil mount "$partition" >/dev/null
    mountpoint="$(diskutil info -plist "$partition" | plutil -extract MountPoint raw -o - -)"
    [[ -n "$mountpoint" && -d "$mountpoint" ]]
    printf '4kn-virtual-block-acceptance\n' > "$mountpoint/edpcli-hil-marker.txt"
    sync
    [[ "$(cat "$mountpoint/edpcli-hil-marker.txt")" == "4kn-virtual-block-acceptance" ]]
    diskutil unmount "$partition" >/dev/null
  fi
done
echo "macOS 4Kn native virtual disk HIL PASS"
