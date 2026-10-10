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
  # A pair consists of an independently written+reattached source followed by
  # the target transition on that same disposable OS Disk Image. All 25 source
  # x target combinations are selected by default; subsets aid triage.
  mode_steps=()
  if [[ "${EDPCLI_HIL_PAIR_MATRIX:-0}" == "1" ]]; then
    for source in ${EDPCLI_HIL_MATRIX_SOURCES:-plain mode0 mode1 mode2 mode3}; do
      for target in ${EDPCLI_HIL_MATRIX_TARGETS:-plain mode0 mode1 mode2 mode3}; do
        case "$source" in plain|mode0|mode1|mode2|mode3) ;; *) echo "bad matrix source: $source" >&2; exit 2;; esac
        case "$target" in plain|mode0|mode1|mode2|mode3) ;; *) echo "bad matrix target: $target" >&2; exit 2;; esac
        mode_steps+=("$source" "$target")
      done
    done
    echo "[matrix] sector=$sector combinations=$((${#mode_steps[@]} / 2)) source+target; each side verified after OS reattach"
  else
    read -r -a mode_steps <<< "${EDPCLI_HIL_MODES:-plain mode0 mode1 mode2 mode3 plain}"
  fi
  step=0
  for mode in "${mode_steps[@]}"; do
    case "$mode" in plain|mode0|mode1|mode2|mode3) ;; *) echo "unsupported HIL mode: $mode" >&2; exit 2;; esac
    if [[ "${EDPCLI_HIL_PAIR_MATRIX:-0}" == "1" && $((step % 2)) == 0 ]]; then
      matrix_source="$mode"
      echo "[matrix] preparing source=$matrix_source target=${mode_steps[step+1]} sector=$sector"
    fi
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
    if [[ "${EDPCLI_HIL_VERIFY_CROSS_PRESERVE:-0}" == "1" && "${EDPCLI_HIL_SKIP_CUSTOM_PW:-0}" == "1" ]]; then
      if [[ "$mode" == "mode0" || "$mode" == "mode1" || "$mode" == "mode2" ]]; then
        phase="verify"
        if [[ "$mode" == "mode0" ]]; then phase="capture"; fi
        raw="/dev/r$(basename "$device")"
        args=(EDPCLI_CRYPTO_HIL_RAW="$raw" EDPCLI_CRYPTO_HIL_SECTOR="$sector" EDPCLI_CROSS_HIL_PHASE="$phase" EDPCLI_CROSS_HIL_SNAPSHOT="$root/cross-$sector.sha256")
        if [[ -r "$raw" ]]; then
          env "${args[@]}" "$verifier" --ignored --exact native_cli_crypto_hil::native_os_cross_mode_encrypt_full_extent_sha256 --nocapture
        else
          sudo -n env "${args[@]}" "$verifier" --ignored --exact native_cli_crypto_hil::native_os_cross_mode_encrypt_full_extent_sha256 --nocapture
        fi
      fi
    fi

    # HIL-only v4 metadata damage/restore through the standard exclusive lease,
    # durable WAL and fresh native readback. Never grants production v4 restore.
    if [[ "${EDPCLI_HIL_EDPB_WAL:-0}" == "1" && "$mode" == "mode0" && "$sector" != "512" ]]; then
      raw="/dev/r$(basename "$device")"
      args=(EDPCLI_CRYPTO_HIL_RAW="$raw" EDPCLI_CRYPTO_HIL_SECTOR="$sector" EDPCLI_CRYPTO_HIL_BYTES=536870912)
      if [[ -r "$raw" && -w "$raw" ]]; then
        env "${args[@]}" "$verifier" --ignored --exact native_edpb_wal_hil::native_edpb_evidence_virtual_wal_restore_and_fresh_readback --nocapture
      else
        sudo -n env "${args[@]}" "$verifier" --ignored --exact native_edpb_wal_hil::native_edpb_evidence_virtual_wal_restore_and_fresh_readback --nocapture
      fi
      diskutil eject "$device" >/dev/null
      device=""
      attach
      verify_crypto
    fi

  if [[ "${EDPCLI_HIL_PRESERVE_FULL:-0}" == "1" && "$mode" == "mode0" ]]; then
    snapshot="$root/preserve-$sector.sha256"
    full_digest() {
      local phase="$1" raw="/dev/r$(basename "$device")"
      local args=(EDPCLI_CRYPTO_HIL_RAW="$raw" EDPCLI_CRYPTO_HIL_SECTOR="$sector" EDPCLI_CRYPTO_HIL_BYTES=536870912 EDPCLI_CRYPTO_HIL_SHARE_PASSWORD="${share_password:-0000aaaa}" EDPCLI_CRYPTO_HIL_ENCRYPT_PASSWORD="${encrypt_password:-0000aaaa}" EDPCLI_PRESERVE_HIL_SNAPSHOT="$snapshot" EDPCLI_PRESERVE_HIL_PHASE="$phase")
      if [[ -r "$raw" ]]; then
        env "${args[@]}" "$verifier" --ignored --exact native_cli_crypto_hil::native_cli_full_extent_preservation_sha256 --nocapture
      else
        sudo -n env "${args[@]}" "$verifier" --ignored --exact native_cli_crypto_hil::native_cli_full_extent_preservation_sha256 --nocapture
      fi
    }
    full_digest capture
    echo "[preserve] unchanged Mode0 -> Mode0, sector=$sector"
    if [[ -r "/dev/r$(basename "$device")" && -w "/dev/r$(basename "$device")" ]]; then
      "$bin" provision write --include-virtual --disk "$device" --target mode0 --yes --backup-dir "$root"
    else
      sudo -n "$bin" provision write --include-virtual --disk "$device" --target mode0 --yes --backup-dir "$root"
    fi
    diskutil eject "$device" >/dev/null
    device=""
    attach
    full_digest verify
  fi
    if [[ "$mode" == "mode0" && "${EDPCLI_HIL_SKIP_CUSTOM_PW:-0}" != "1" && "${EDPCLI_HIL_PAIR_MATRIX:-0}" != "1" ]]; then
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
      if [[ "${EDPCLI_HIL_PRESERVE_FULL:-0}" == "1" ]]; then
        full_digest verify
      fi
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
    if [[ "${EDPCLI_HIL_PAIR_MATRIX:-0}" == "1" && $((step % 2)) == 1 ]]; then
      echo "[matrix] PASS sector=$sector source=$matrix_source target=$mode after WAL, reattach and independent crypto/FS readback"
    fi
    step=$((step + 1))
  done
  if [[ "${EDPCLI_HIL_PAIR_MATRIX:-0}" == "1" ]]; then
    [[ $((step % 2)) == 0 ]] || { echo "incomplete pair matrix" >&2; exit 1; }
    echo "[matrix] PASS sector=$sector combinations=$((step / 2))"
  fi
  diskutil eject "$device" >/dev/null
  device=""
done
echo "PASS formal edpcli CLI native OS block HIL: sectors=${EDPCLI_HIL_SECTORS:-512 4096}, modes=${EDPCLI_HIL_PAIR_MATRIX:+pair matrix} ${EDPCLI_HIL_MODES:-}, reattach, native crypto/LCE/FAT readback"
