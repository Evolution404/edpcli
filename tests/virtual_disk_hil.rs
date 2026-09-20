#![cfg(all(
    feature = "ci-virtual-disk",
    any(target_os = "linux", target_os = "windows")
))]

use std::collections::BTreeMap;
use std::time::Duration;

use edpcli::common::{METADATA_SECTOR_COUNT, SECTOR};
use edpcli::diskio::{atomic_write_sectors, FileDev, SectorDev};

fn read_metadata(dev: &mut dyn SectorDev) -> BTreeMap<u32, Vec<u8>> {
    (0u32..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            let data = dev
                .read_sector(lba)
                .unwrap_or_else(|error| panic!("read LBA{lba}: {error}"));
            (lba, data)
        })
        .collect()
}

fn deterministic_patch() -> BTreeMap<u32, Vec<u8>> {
    (0u32..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            let mut data = vec![0u8; SECTOR];
            for (index, byte) in data.iter_mut().enumerate() {
                *byte = ((lba as usize * 37 + index * 13 + 0x5a) % 251) as u8;
            }
            data[..12].copy_from_slice(b"EDPCLI-VHIL\0");
            data[12..16].copy_from_slice(&lba.to_le_bytes());
            (lba, data)
        })
        .collect()
}

#[test]
#[ignore = "requires a disposable OS virtual disk created by the HIL workflow"]
fn raw_virtual_disk_atomic_roundtrip_and_restore() {
    let path = std::env::var("EDPCLI_VIRTUAL_DISK_PATH")
        .expect("EDPCLI_VIRTUAL_DISK_PATH must point to the disposable loop/VHD");
    assert!(
        edpcli::platform::is_raw_device_path(&path),
        "HIL path must be a raw device: {path}"
    );

    // 先走产品平台层的卸载/锁卷逻辑。ci_prepare_virtual_write 内部还会二次确认：
    // Linux 必须是 /dev/loopN；Windows 必须是 Virtual/FileBackedVirtual VHD。
    let _guard = edpcli::platform::ci_prepare_virtual_write(&path)
        .unwrap_or_else(|error| panic!("prepare virtual write {path}: {error}"));

    let mut dev = FileDev::open_rdwr(&path, Duration::from_secs(5))
        .unwrap_or_else(|error| panic!("open virtual raw device {path}: {error}"));
    let original = read_metadata(&mut dev);
    let patch = deterministic_patch();
    assert_ne!(
        original, patch,
        "virtual disk unexpectedly matches HIL pattern"
    );

    atomic_write_sectors(&mut dev, &patch).expect("atomic write HIL pattern");
    assert_eq!(
        read_metadata(&mut dev),
        patch,
        "HIL write read-back mismatch"
    );

    atomic_write_sectors(&mut dev, &original).expect("restore original virtual disk metadata");
    assert_eq!(
        read_metadata(&mut dev),
        original,
        "virtual disk was not restored bit-for-bit"
    );
}
