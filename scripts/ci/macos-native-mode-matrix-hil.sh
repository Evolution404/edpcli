#!/usr/bin/env bash
# Production CLI 5x5 destructive source -> target on a disposable OS block device.
# EDPCLI_HIL_SECTOR_BYTES defaults to the historical 4Kn case; supported
# formal matrix runs may request 512, 1024, 2048, or 4096B.
set -euo pipefail
sector="${EDPCLI_HIL_SECTOR_BYTES:-4096}"
case "$sector" in 512|1024|2048|4096) ;; *) echo "unsupported test sector size: $sector" >&2; exit 2;; esac
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
  attached="$(hdiutil attach -nomount -noverify -blocksize "$sector" "$img" 2>&1)"
  # Recent macOS prints a deprecation warning before the actual device path.
  disk="$(printf '%s\n' "$attached" | awk '$1 ~ /^\/dev\/disk[0-9]+$/ {print $1; exit}')"
  [[ "$disk" =~ ^/dev/disk[0-9]+$ ]] || exit 1
  diskutil info -plist "$disk" | plutil -convert json -o - - | python3 -c '
import json,sys
p=json.load(sys.stdin)
assert p.get("WholeDisk") is True
assert p.get("Internal") is False
assert p.get("VirtualOrPhysical") == "Virtual"
assert p.get("BusProtocol") == "Disk Image"
assert p.get("DeviceBlockSize") == int(sys.argv[1])
assert p.get("TotalSize") == 536870912
' "$sector"
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
  if ! sudo -n "$bin" provision write --disk "$disk" --target "$mode" \
    --include-virtual --yes --backup-dir "$root" > "$root/cli-write.log" 2>&1; then
    echo "formal CLI native write failed: sector=${sector} target=$mode device=$disk" >&2
    tail -35 "$root/cli-write.log" >&2
    return 1
  fi
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
  local row
  row="$(printf '%s\n' "$table" | grep -E "^[[:space:]]*$(basename "$disk")[[:space:]]" || true)"
  [[ "$row" == *"$label"* ]] || {
    echo "source-mode readback mismatch expected $mode" >&2
    echo "$table" >&2
    exit 1
  }
}
# Eulerian walk over every directed pair, including 5 same-mode pairs:
# exactly 25 distinct source->target edges with only 26 production writes.
# The source for every edge is the freshly reopened, verified prior target.
case "${EDPCLI_HIL_SEGMENT:-full}" in
  full) sequence=(plain plain mode0 plain mode1 plain mode2 plain mode3 mode0 mode0 mode1 mode0 mode2 mode0 mode3 mode1 mode1 mode2 mode1 mode3 mode2 mode2 mode3 mode3 plain) ;;
  tail) sequence=(mode2 mode3 mode3 plain) ;;
  *) echo "unsupported HIL segment: ${EDPCLI_HIL_SEGMENT}" >&2; exit 2 ;;
esac
expected=$(( ${#sequence[@]} - 1 ))
attach
write_mode "${sequence[0]}"
reattach
assert_mode "${sequence[0]}"
count=0
for ((i=1; i<${#sequence[@]}; i++)); do
  source="${sequence[i-1]}"
  target="${sequence[i]}"
  echo "[mode-matrix] ${sector}B $source -> $target"
  write_mode "$target"
  reattach
  assert_mode "$target"
  count=$((count+1))
  echo "[PASS] $count/$expected $source -> $target"
done
[[ "$count" -eq "$expected" ]]
echo "PASS CLI ${sector}B ${EDPCLI_HIL_SEGMENT:-full} $count source->target destructive transitions; $((count + 1)) OS raw writes"
