#!/usr/bin/env bash
# Formal CLI provisioning 512B and 4096B on disposable OS disk images only.
set -euo pipefail
root="$(mktemp -d /tmp/edpcli-cli-hil.XXXXXX)"
device=""
cleanup() {
  set +e
  if [[ -n "$device" ]]; then diskutil eject "$device" >/dev/null 2>&1 || true; fi
  rm -rf "$root"
}
trap cleanup EXIT
cargo build --quiet --locked
bin="$(pwd)/target/debug/edpcli"
for sector in 512 4096; do
  img="$root/n$sector.img"
  mkfile -n 512m "$img"
  attach() {
    local attach_result
    attach_result="$(hdiutil attach -nomount -noverify -blocksize "$sector" "$img")"
    device="$(printf '%s\n' "$attach_result" | awk 'NR==1{print $1}')"
    [[ "$device" =~ ^/dev/disk[0-9]+$ ]] || exit 1
    diskutil info -plist "$device" | plutil -convert json -o - - | python3 -c '
import json,sys
v=json.load(sys.stdin)
assert v.get("WholeDisk") is True
assert v.get("Internal") is False
assert v.get("VirtualOrPhysical") == "Virtual"
assert v.get("BusProtocol") == "Disk Image"
assert v.get("DeviceBlockSize") == int(sys.argv[1])
assert v.get("TotalSize") == 536870912
' "$sector"
    local inventory
    inventory="$(hdiutil info)"
    [[ "$inventory" == *"$img"* ]]
  }
  attach
  for mode in plain mode0 mode1 mode2 mode3 plain; do
    echo "=== CLI native $sector target=$mode disk=$device ==="
    if [[ -r "/dev/r$(basename "$device")" && -w "/dev/r$(basename "$device")" ]]; then
      "$bin" provision write --include-virtual --disk "$device" --target "$mode" --yes --backup-dir "$root"
    else
      sudo -n "$bin" provision write --include-virtual --disk "$device" --target "$mode" --yes --backup-dir "$root"
    fi
    diskutil eject "$device" >/dev/null
    device=""
    attach
    if [[ "$mode" == "plain" ]]; then
      # Formal CLI formatter must produce an actually mountable 512/4096B
      # ExFAT volume. Verify a file survives detach and reattach.
      volume="$(printf '%ss1' "$device")"
      diskutil mount "$volume" >/dev/null
      mountpoint="$(diskutil info -plist "$volume" | plutil -extract MountPoint raw -o - -)"
      [[ -n "$mountpoint" && -d "$mountpoint" ]]
      printf 'edpcli native %s persisted\n' "$sector" > "$mountpoint/edpcli-native-hil.txt"
      sync
      diskutil unmount "$volume" >/dev/null
      diskutil eject "$device" >/dev/null
      device=""
      attach
      volume="$(printf '%ss1' "$device")"
      diskutil mount "$volume" >/dev/null
      mountpoint="$(diskutil info -plist "$volume" | plutil -extract MountPoint raw -o - -)"
      [[ "$(cat "$mountpoint/edpcli-native-hil.txt")" == "edpcli native $sector persisted" ]]
      diskutil unmount "$volume" >/dev/null
    fi
  done
  diskutil eject "$device" >/dev/null
  device=""
done
echo 'PASS formal edpcli CLI native OS block HIL: 12 destructive writes + reattach'
