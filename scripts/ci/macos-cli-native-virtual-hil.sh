#!/usr/bin/env bash
# Formal CLI provisioning and independent crypto/FS readback on disposable OS disk images.
# EDPCLI_HIL_SECTORS optionally selects any subset of 512 1024 2048 4096.
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
# Build the read-only independent verifier once; the formal CLI is the ONLY writer.
verifier="$(cargo test --locked --features ci-virtual-disk --test native_macos_4kn_virtual_hil --no-run --message-format=json | python3 -c '
import json,sys
for line in sys.stdin:
    record=json.loads(line)
    if record.get("reason")=="compiler-artifact" and record.get("target",{}).get("name")=="native_macos_4kn_virtual_hil":
        path=record.get("executable")
        if path: print(path)
')"
[[ -x "$verifier" ]] || { echo "independent verifier build failed" >&2; exit 1; }
verify_crypto() {
  local raw="/dev/r$(basename "$device")"
  local share_password="${1:-0000aaaa}" encrypt_password="${2:-0000aaaa}"
  local args=(EDPCLI_CRYPTO_HIL_RAW="$raw" EDPCLI_CRYPTO_HIL_SECTOR="$sector" EDPCLI_CRYPTO_HIL_MODE="$mode" EDPCLI_CRYPTO_HIL_BYTES=536870912 EDPCLI_CRYPTO_HIL_SHARE_PASSWORD="$share_password" EDPCLI_CRYPTO_HIL_ENCRYPT_PASSWORD="$encrypt_password")
  # Read-only access; escalation is only for opening the guarded disk node.
  if [[ -r "$raw" ]]; then
    env "${args[@]}" "$verifier" --ignored --exact native_cli_crypto_hil::formal_cli_native_crypto_readback --nocapture
  else
    sudo -n env "${args[@]}" "$verifier" --ignored --exact native_cli_crypto_hil::formal_cli_native_crypto_readback --nocapture
  fi
}
for sector in ${EDPCLI_HIL_SECTORS:-512 4096}; do
  case "$sector" in 512|1024|2048|4096) ;; *) echo "unsupported HIL sector: $sector" >&2; exit 2;; esac
  img="$root/n$sector.img"
  mkfile -n 512m "$img"
  attach() {
    local attach_result
    attach_result="$(hdiutil attach -nomount -noverify -blocksize "$sector" "$img" 2>&1)"
    device="$(printf '%s\n' "$attach_result" | awk '$1 ~ /^\/dev\/disk[0-9]+$/ {print $1; exit}')"
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
  for mode in ${EDPCLI_HIL_MODES:-plain mode0 mode1 mode2 mode3 plain}; do
    case "$mode" in plain|mode0|mode1|mode2|mode3) ;; *) echo "unsupported HIL mode: $mode" >&2; exit 2;; esac
    echo "=== CLI native $sector target=$mode disk=$device ==="
    if [[ -r "/dev/r$(basename "$device")" && -w "/dev/r$(basename "$device")" ]]; then
      "$bin" provision write --include-virtual --disk "$device" --target "$mode" --yes --backup-dir "$root"
    else
      sudo -n "$bin" provision write --include-virtual --disk "$device" --target "$mode" --yes --backup-dir "$root"
    fi
    diskutil eject "$device" >/dev/null
    device=""
    attach
    # Reopened OS block device: parse/decrypt from real bytes, not authored writes.
    verify_crypto
    if [[ "$mode" == "mode0" ]]; then
      # Prove explicit independent passwords, never the implicit default.
      # These are public test-only values, never credentials for real USBs.
      share_password="P1Share2026!"
      encrypt_password="P1Encrypt2026!"
      echo "[P1] custom target passwords mode0, sector=$sector, disposable Disk Image"
      if [[ -r "/dev/r$(basename "$device")" && -w "/dev/r$(basename "$device")" ]]; then
        "$bin" provision write --include-virtual --disk "$device" --target mode0 --yes --backup-dir "$root" --share-target-password "$share_password" --encrypt-target-password "$encrypt_password"
      else
        sudo -n "$bin" provision write --include-virtual --disk "$device" --target mode0 --yes --backup-dir "$root" --share-target-password "$share_password" --encrypt-target-password "$encrypt_password"
      fi
      diskutil eject "$device" >/dev/null
      device=""
      attach
      verify_crypto "$share_password" "$encrypt_password"
      echo "[P1] PASS custom target passwords with independent FileKey unwrap"
    fi
    if [[ "$mode" == "plain" ]]; then
      # Formal CLI formatter must produce an actually mountable native-sector
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
echo "PASS formal edpcli CLI native OS block HIL: sectors=${EDPCLI_HIL_SECTORS:-512 4096}, modes=${EDPCLI_HIL_MODES:-plain mode0 mode1 mode2 mode3 plain}, reattach, native crypto/LCE/FAT readback"
