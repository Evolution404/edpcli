#![cfg(all(feature = "ci-virtual-disk", target_os = "macos"))]

use std::time::Duration;

use edpcli::{
    common::SECTOR,
    diskio::{execute_write_transaction, FileDev, SectorDev, WriteTransactionPlan},
    platform,
    provision::{
        build_plain_provision_write_plan, DiskProvisionKind, OfficialFilesystemFormat,
        PlainPartitionSpec, PlainProvisionPlan,
    },
    sysinfo::SysRunner,
};

#[test]
#[ignore = "requires a disposable macOS raw disk image created by the HIL script"]
fn macos_plain_virtual_disk_provisions_and_reidentifies() {
    let raw = std::env::var("EDPCLI_PLAIN_VIRTUAL_DISK_PATH")
        .expect("EDPCLI_PLAIN_VIRTUAL_DISK_PATH must point to /dev/rdiskN");
    let disk = platform::parse_disk_selector(&raw).expect("parse virtual disk selector");
    let runner = SysRunner;
    let total_sectors =
        platform::disk_total_sectors(&runner, disk).expect("read virtual disk total sectors");
    assert!(total_sectors > 32_768);

    let _guard = platform::ci_prepare_virtual_write(&raw)
        .unwrap_or_else(|error| panic!("prepare virtual write {raw}: {error}"));
    let mut dev = FileDev::open_rdwr(&raw, Duration::from_secs(5))
        .unwrap_or_else(|error| panic!("open virtual raw disk {raw}: {error}"));

    let lba3 = [0xA3u8; SECTOR];
    dev.write_sector(3, &lba3).expect("seed LBA3 sentinel");
    dev.sync().expect("sync LBA3 sentinel");

    let layout = PlainProvisionPlan::new(
        total_sectors,
        vec![PlainPartitionSpec::new(
            2_048,
            total_sectors - 2_048,
            OfficialFilesystemFormat::ExFat,
            "EDPCLI-HIL",
        )],
    )
    .expect("build Plain virtual layout");
    let plain = build_plain_provision_write_plan(&layout, None, &[0x1234_5678])
        .expect("build Plain write plan");
    let transaction =
        WriteTransactionPlan::from_plain_provision(&plain).expect("map Plain transaction");
    execute_write_transaction(&mut dev, &transaction).expect("execute Plain transaction");

    assert_eq!(dev.read_sector(3).expect("read LBA3"), lba3);
    let lba7 = dev.read_sector(7).expect("read LBA7");
    let lba12 = dev.read_sector(12).expect("read LBA12");
    assert_eq!(
        DiskProvisionKind::from_sectors(&lba7, &lba12, "macos-virtual-hil"),
        DiskProvisionKind::Plain
    );
    let mbr = dev.read_sector(0).expect("read MBR");
    assert_eq!(&mbr[510..512], &[0x55, 0xaa]);
    assert_eq!(mbr[0x1be + 4], 0x07);
    assert_eq!(
        u32::from_le_bytes(mbr[0x1be + 8..0x1be + 12].try_into().unwrap()),
        2_048
    );
}
