use crate::common;

use std::path::PathBuf;

use edpcli::application::inspect::load_backup_inspect;
use edpcli::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use edpcli::edpb::{self, CoreCapture};

fn netac_edpb(tag: &str) -> Option<(common::TmpDir, PathBuf)> {
    let data = common::load_disk_image("netac")?;
    let tmp = common::TmpDir::new(tag);
    let path = tmp.0.join("netac.edpb");
    edpb::write_core_backup(
        &path,
        &CoreCapture {
            snapshot_id: format!("tui-inspect-{tag}"),
            created_epoch: 1_789_000_000,
            disk_number: Some(6),
            vid: "0dd8".into(),
            pid: "2005".into(),
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            onlyid: Some("1402259934".into()),
            total_sectors: Some(122_880_000),
            logical_sector_size: 512,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "encrypted".into(),
            lba0_12: &data,
        },
    )
    .unwrap();
    Some((tmp, path))
}

#[test]
fn backup_inspect_reuses_domain_analyzer_for_all_metadata_lbas() {
    let Some((_tmp, path)) = netac_edpb("inspect_workspace") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let workspace = load_backup_inspect(&path).expect("inspect backup");
    assert_eq!(workspace.items.len(), METADATA_SECTOR_COUNT);
    for (lba, item) in workspace.items.iter().enumerate() {
        assert_eq!(item.lba, lba as u64);
        assert_eq!(item.raw.len(), 512);
        assert_eq!(item.decoded.as_ref().map(Vec::len), Some(512));
        assert!(item.method.is_some());
    }
}

#[test]
fn backup_inspect_rejects_legacy_bin_images() {
    let tmp = common::TmpDir::new("inspect_reject_7168");
    let path = tmp.0.join("legacy-lba0-13.bin");
    std::fs::write(&path, vec![0u8; METADATA_IMAGE_LEN + SECTOR]).unwrap();

    let err = load_backup_inspect(&path).expect_err("legacy .bin must be rejected");
    assert!(err.message().contains(".edpb"), "{err}");
}
