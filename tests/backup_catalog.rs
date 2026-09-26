use crate::common;

use std::fs;

use common::*;
use edpcli::application::media_identity::{
    match_media_identity, BackupAffinity, BackupAffinityPolicy, ControlledLineageEvidence,
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentitySnapshot,
    ProtocolIdentityEvidence, SerialQuality,
};
use edpcli::backup_catalog::BackupCatalog;
use edpcli::edpb::{self, CoreCapture};
use edpcli::metainfo::backup_ownership;
use edpcli::provision::DiskProvisionKind;

fn write_edpb(path: &std::path::Path, data: &[u8], snapshot_id: &str) {
    let capture = CoreCapture {
        snapshot_id: snapshot_id.into(),
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
        lba0_12: data,
    };
    edpb::write_core_backup(path, &capture).unwrap();
}

fn identity(
    serial: Option<&str>,
    device_id: Option<&str>,
    onlyid: Option<&str>,
    kind: DiskProvisionKind,
) -> MediaIdentitySnapshot {
    MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            serial_sha256: serial.map(str::to_string),
            serial_quality: if serial.is_some() {
                SerialQuality::Usable
            } else {
                SerialQuality::Missing
            },
            vendor: Some("Netac".into()),
            product: Some("OnlyDisk".into()),
            revision: Some("1.00".into()),
            transport: None,
            total_sectors: Some(122_880_000),
            logical_sector_size: Some(512),
        },
        protocol: ProtocolIdentityEvidence {
            device_id: device_id.map(str::to_string),
            onlyid: onlyid.map(str::to_string),
            provision_kind: Some(kind),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence::default(),
        observation: IdentityObservation::default(),
    }
}

fn copied_catalog() -> Option<(TmpDir, BackupCatalog)> {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return None;
    };
    let tmp = TmpDir::new("backup_catalog");
    let first = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.edpb",
    );
    write_edpb(&first, &data, "catalog-first");

    // 同一 onlyid 制造第二份，并故意让 mtime 与文件名时间相反。
    // 列表创建时间排序来自文件名；复制/touch 不应改变 [1][2] 编号。
    let second = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260911_172300.edpb",
    );
    write_edpb(&second, &data, "catalog-second");
    set_mtime(&first, 1_789_100_000); // 文件名较旧，但 mtime 较新
    set_mtime(&second, 1_789_000_000); // 文件名较新，但 mtime 较旧

    let catalog = BackupCatalog::load(&tmp.0);
    Some((tmp, catalog))
}

#[test]
fn backup_affinity_policy_confirms_a_b_c_and_keeps_d_possible_only() {
    let a1 = identity(Some(&"11".repeat(32)), None, None, DiskProvisionKind::Plain);
    let a2 = identity(
        Some(&"11".repeat(32)),
        Some("disk&ven_netac&prod_onlydisk"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    assert_eq!(
        BackupAffinityPolicy::classify(&match_media_identity(&a1, &a2, None)),
        BackupAffinity::Confirmed
    );

    let b1 = identity(
        None,
        Some("disk&ven_netac&prod_onlydisk"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let b2 = b1.clone();
    assert_eq!(
        BackupAffinityPolicy::classify(&match_media_identity(&b1, &b2, None)),
        BackupAffinity::Confirmed
    );

    let lineage = ControlledLineageEvidence { linked: true };
    let c_target = identity(
        None,
        Some("disk&ven_netac&prod_onlydisk"),
        Some("99"),
        DiskProvisionKind::Mode2,
    );
    assert_eq!(
        BackupAffinityPolicy::classify(&match_media_identity(&a1, &c_target, Some(&lineage))),
        BackupAffinity::Confirmed
    );

    let d1 = identity(None, None, None, DiskProvisionKind::Plain);
    let d2 = d1.clone();
    assert_eq!(
        BackupAffinityPolicy::classify(&match_media_identity(&d1, &d2, None)),
        BackupAffinity::Possible
    );
}

#[test]
fn scanned_verified_backup_exposes_canonical_identity_projection() {
    let Some((_tmp, catalog)) = copied_catalog() else {
        return;
    };
    let meta = catalog.entries()[0].meta.as_ref().expect("verified meta");
    let identity = meta
        .identity
        .as_ref()
        .expect("verified EDPB must expose canonical identity");
    assert_eq!(
        identity.protocol.device_id.as_deref(),
        Some("disk&ven_netac&prod_onlydisk")
    );
    assert_eq!(identity.protocol.onlyid.as_deref(), Some("1402259934"));
    assert_eq!(
        identity.protocol.provision_kind,
        Some(DiskProvisionKind::Mode0)
    );
}

#[test]
fn catalog_order_prefers_backup_name_time_over_filesystem_mtime() {
    let Some((_tmp, catalog)) = copied_catalog() else {
        return;
    };
    assert_eq!(catalog.entries().len(), 2);
    assert!(catalog.entries()[0]
        .path
        .to_string_lossy()
        .contains("20260911_172300"));
    assert!(catalog.entries()[1]
        .path
        .to_string_lossy()
        .contains("20260910_172300"));
}

#[test]
fn target_resolution_is_confined_to_backup_root() {
    let Some((tmp, catalog)) = copied_catalog() else {
        return;
    };
    let name = catalog.entries()[0]
        .path
        .file_name()
        .unwrap()
        .to_string_lossy();
    let entry = catalog.resolve_target(&name).unwrap();
    assert!(entry.path.starts_with(&tmp.0));

    let outside = tmp.0.parent().unwrap().join("outside.edpb");
    fs::write(&outside, vec![0u8; 16]).unwrap();
    assert!(catalog.resolve_target(outside.to_str().unwrap()).is_err());
}

#[test]
fn ownership_uses_lba8_cached_during_catalog_scan() {
    let Some((_tmp, catalog)) = copied_catalog() else {
        return;
    };
    let entry = &catalog.entries()[0];
    assert!(entry.lba8.is_some());

    // 扫描完成后移除源文件；归属信息仍应从 BackupEntry 的内存 LBA8 得到，
    // 证明 backup list 不会为了 Dept/User 再次打开同一个 .edpb。
    fs::remove_file(&entry.path).unwrap();
    let ownership = backup_ownership(entry).expect("缓存 LBA8 应可解析归属信息");
    assert!(ownership
        .dept
        .as_deref()
        .unwrap_or_default()
        .contains("泰州供电公司"));
    assert_eq!(ownership.user.as_deref(), Some("宋旭琳"));
}
#[test]
fn september_10_netac_backup_with_conflicting_edpf_tables_is_plain() {
    let mode0 = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/backup/disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid949028302_20260910_172420.bin"
    ))
    .expect("recorded backup image");
    assert_eq!(
        edpcli::provision::DiskProvisionKind::from_metadata(&mode0, "disk&ven_netac&prod_onlydisk"),
        edpcli::provision::DiskProvisionKind::Mode0
    );
    let conflicting = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/audit/protocol/gold/strict-encrypted/disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid949028302_20260910_172433.bin"
    ))
    .expect("recorded conflicting image");
    assert_eq!(
        edpcli::provision::DiskProvisionKind::from_metadata(
            &conflicting,
            "disk&ven_netac&prod_onlydisk"
        ),
        edpcli::provision::DiskProvisionKind::Plain
    );
}
