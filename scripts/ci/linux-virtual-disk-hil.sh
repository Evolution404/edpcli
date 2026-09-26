#!/usr/bin/env bash
set -euo pipefail

image="${RUNNER_TEMP:-/tmp}/edpcli-virtual-hil.img"
mount_dir="${RUNNER_TEMP:-/tmp}/edpcli-virtual-hil-mount"
loop=""

cleanup() {
  set +e
  if mountpoint -q "$mount_dir" 2>/dev/null; then
    sudo umount "$mount_dir"
  fi
  if [[ -n "$loop" ]]; then
    sudo losetup -d "$loop" 2>/dev/null || true
  fi
  rm -f "$image"
  rmdir "$mount_dir" 2>/dev/null || true
}
trap cleanup EXIT

rm -f "$image"
mkdir -p "$mount_dir"
truncate -s 128M "$image"

loop="$(sudo losetup --find --show "$image")"
printf 'label: dos\n,64M,L,*\n' | sudo sfdisk "$loop"
sudo losetup -d "$loop"
loop="$(sudo losetup --find --show --partscan "$image")"
partition="${loop}p1"

for _ in $(seq 1 30); do
  [[ -b "$partition" ]] && break
  sleep 0.2
done
[[ -b "$partition" ]] || { echo "partition not created: $partition" >&2; exit 1; }

sudo mkfs.ext4 -q -F "$partition"
sudo mount "$partition" "$mount_dir"
echo "edpcli-virtual-hil" | sudo tee "$mount_dir/marker.txt" >/dev/null
sudo sync

export EDPCLI_VIRTUAL_DISK_PATH="$loop"
export EDPCLI_VIRTUAL_DISK_SECTORS="$(sudo blockdev --getsz "$loop")"
export CARGO_TARGET_DIR="${RUNNER_TEMP:-/tmp}/edpcli-virtual-hil-target"
cargo_bin="$(command -v cargo)"

# prepare_write 需要执行真实 umount2，因此让测试进程以 root 运行；测试入口本身只接受
# /dev/loopN，无法被用于真实 USB/SATA/NVMe 设备。
sudo --preserve-env=HOME,PATH,RUSTUP_HOME,CARGO_HOME,CARGO_TARGET_DIR,EDPCLI_VIRTUAL_DISK_PATH,EDPCLI_VIRTUAL_DISK_SECTORS \
  "$cargo_bin" test --features ci-virtual-disk --test virtual_disk_hil -- --ignored --nocapture

if mountpoint -q "$mount_dir"; then
  echo "product prepare_write did not unmount the loop partition" >&2
  exit 1
fi

# Rust HIL 测试会把 LBA0-12 bit-for-bit 恢复。重新挂载并读取文件，证明分区表和文件系统
# 仍然完整，而不是仅仅在同一 raw fd 上读到了缓存。
sudo mount "$partition" "$mount_dir"
marker="$(sudo cat "$mount_dir/marker.txt")"
[[ "$marker" == "edpcli-virtual-hil" ]] || {
  echo "filesystem marker was not preserved after raw roundtrip" >&2
  exit 1
}
sudo umount "$mount_dir"

echo "Linux virtual-disk HIL PASS: $loop"
