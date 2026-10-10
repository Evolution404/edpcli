#!/usr/bin/env bash
# Production CLI 5x5 destructive source -> target on the same disposable 4Kn OS block device.
set -euo pipefail
root="$(mktemp -d /tmp/edpcli-native-mode-matrix.XXXXXX)"
img="$root/matrix.img"
disk=""
cleanup() {
  set +e
  if [[ -n "$disk" ]]; then diskutil eject "$disk" >/dev/null 2>&1 || true; fi
  rm -rf "$root"
}
trap cleanup EXIT
mkfile -n 512m "$img"
cargo build --quiet --locked
bin="$(pwd)/target/debug/edpcli"

attach() {
  local attached inventory
  attached="$(hdiutil attach -nomount -noverify -blocksize 4096 "$img")"
  disk="$(printf '%s\n' "$attached" | awk 'NR==1{print $1}')"
  [[ "$disk" =~ ^/dev/disk[0-9]+$ ]] || exit 1
  diskutil info -plist "$disk" | plutil -convert json -o - - | python3 -c '
import json,sys
p=json.load(sys.stdin)
assert p.get("WholeDisk") is True
assert p.get("Internal") is False
assert p.get("VirtualOrPhysical") == "Virtual"
assert p.get("BusProtocol") == "Disk Image"
assert p.get("DeviceBlockSize") == 4096
assert p.get("TotalSize") == 536870912
'
  inventory="$(hdiutil info)"
  [[ "$inventory" == *"$img"* ]] || exit 1
}
reattach() {
  diskutil eject "$disk" >/dev/null
  disk=""
  attach
}
write_mode() {
  local mode="$1"
  sudo -n "$bin" provision write --disk "$disk" --target "$mode" \
    --include-virtual --yes --backup-dir "$root" >/dev/null
}
assert_mode() {
  local mode="$1" label
  case "$mode" in
    plain) label='虚拟 · 普通盘';;
    mode0) label='虚拟 · mode0';;
    mode1) label='虚拟 · mode1';;
    mode2) label='虚拟 · mode2';;
    mode3) label='虚拟 · mode3';;
  esac
  local table
  table="$(sudo -n "$bin" list --include-virtual)"
  [[ "$table" == *"$label"* ]] || {
    echo "source-mode readback mismatch expected $mode" >&2
    echo "$table" >&2
    exit 1
  }
}
attach
count=0
for source in plain mode0 mode1 mode2 mode3; do
  for target in plain mode0 mode1 mode2 mode3; do
    echo "[mode-matrix] $source -> $target"
    write_mode "$source"
    reattach
    assert_mode "$source"
    write_mode "$target"
    reattach
    assert_mode "$target"
    count=$((count+1))
    echo "[PASS] $count/25 $source -> $target"
  done
done
echo "PASS CLI 4Kn all 25 source->target destructive transitions; 50 OS raw writes"
