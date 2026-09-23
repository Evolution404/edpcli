mod common;

use std::path::PathBuf;
use std::process::Command;

use common::*;
use edpcli::backup_metadata::parse_partition_geometry;
use edpcli::common::SECTOR;
use edpcli::edpb::{
    self, ArtifactCompleteness, ArtifactInput, CoreCapture, Extent, MetadataCapture, Region,
    RestorePolicy, SemanticStatus,
};

fn fixture_edpb(key: &str, tag: &str) -> Option<(TmpDir, PathBuf)> {
    let data = load_disk_image(key)?;
    let (disk, sectors, vid, pid, device_id, onlyid) = match key {
        "netac" => (
            6,
            122_880_000u64,
            "0dd8",
            "2005",
            "disk&ven_netac&prod_onlydisk",
            "1402259934",
        ),
        "aigo" => (
            4,
            245_760_000u64,
            "3535",
            "6300",
            "disk&ven_aigo&prod_u335&rev_pmap",
            "1987718388",
        ),
        _ => return None,
    };
    let tmp = TmpDir::new(tag);
    let path = tmp.0.join(format!("{key}.edpb"));
    let capture = CoreCapture {
        snapshot_id: format!("inspect-cli-{key}"),
        created_epoch: 1_789_000_000,
        disk_number: Some(disk),
        vid: vid.into(),
        pid: pid.into(),
        device_id: device_id.into(),
        onlyid: Some(onlyid.into()),
        total_sectors: Some(sectors),
        logical_sector_size: 512,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "encrypted".into(),
        lba0_12: &data,
    };
    edpb::write_core_backup(&path, &capture).unwrap();
    Some((tmp, path))
}

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn valid_exfat_boot(partition_start: u64, sector_count: u64) -> [u8; SECTOR] {
    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    let fat_offset = 24u64;
    let fat_length = 1024u64.min(sector_count.saturating_sub(fat_offset + 1).max(1));
    let heap_offset = fat_offset + fat_length;
    let sectors_per_cluster = 8u64;
    let cluster_count = sector_count
        .saturating_sub(heap_offset)
        .checked_div(sectors_per_cluster)
        .unwrap_or(0)
        .min(u32::MAX as u64);
    if cluster_count == 0 {
        return boot;
    }
    put64(&mut boot, 64, partition_start);
    put64(&mut boot, 72, sector_count);
    put32(&mut boot, 80, fat_offset as u32);
    put32(&mut boot, 84, fat_length as u32);
    put32(&mut boot, 88, heap_offset as u32);
    put32(&mut boot, 92, cluster_count as u32);
    put32(&mut boot, 96, 2);
    put16(&mut boot, 104, 0x0100);
    boot[108] = 9;
    boot[109] = 3;
    boot[110] = 1;
    boot[111] = 0x80;
    boot[112] = 0xff;
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn captured_partition_edpb(deep: bool) -> Option<(TmpDir, PathBuf, u64)> {
    let data = load_disk_image("aigo")?;
    let device_id = "disk&ven_aigo&prod_u335&rev_pmap";
    let total_sectors = 245_760_000u64;
    let partition = parse_partition_geometry(&data, device_id, total_sectors)
        .ok()?
        .into_iter()
        .find(|partition| partition.partition_type == 2)?;
    let boot = valid_exfat_boot(partition.start_sector, partition.sector_count);
    if boot[510..512] != [0x55, 0xaa] {
        return None;
    }
    let mut extent_data = boot.to_vec();
    extent_data.extend_from_slice(&[0x5au8; SECTOR]);

    let tag = if deep {
        "inspect_deep"
    } else {
        "inspect_metadata"
    };
    let tmp = TmpDir::new(tag);
    let path = tmp.0.join(format!("{tag}.edpb"));
    let extent_id = "extent.inspect.partition".to_string();
    let region_id = "region.inspect.partition".to_string();
    let tail_extent_id = "extent.inspect.last_lba".to_string();
    let tail_region_id = "region.inspect.last_lba".to_string();
    let capture = MetadataCapture {
        core: CoreCapture {
            snapshot_id: format!("inspect-cli-{tag}"),
            created_epoch: 1_789_000_000,
            disk_number: Some(4),
            vid: "3535".into(),
            pid: "6300".into(),
            device_id: device_id.into(),
            onlyid: Some("1987718388".into()),
            total_sectors: Some(total_sectors),
            logical_sector_size: SECTOR as u32,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "inspect-fixture".into(),
            lba0_12: &data,
        },
        regions: vec![
            Region {
                id: region_id.clone(),
                role: "partition.inspect_fixture".into(),
                start_lba: Some(partition.start_sector),
                sector_count: Some(2),
                semantic_status: SemanticStatus::Identified,
            },
            Region {
                id: tail_region_id.clone(),
                role: "device.last_lba_fixture".into(),
                start_lba: Some(total_sectors - 1),
                sector_count: Some(1),
                semantic_status: SemanticStatus::Unknown,
            },
        ],
        extents: vec![
            Extent {
                id: extent_id.clone(),
                region_id,
                start_lba: partition.start_sector,
                sector_count: 2,
                purpose: "inspect_offline_fixture".into(),
            },
            Extent {
                id: tail_extent_id.clone(),
                region_id: tail_region_id,
                start_lba: total_sectors - 1,
                sector_count: 1,
                purpose: "inspect_last_legal_lba_fixture".into(),
            },
        ],
        artifacts: vec![
            ArtifactInput {
                id: "raw.inspect.partition".into(),
                kind: "raw_sectors".into(),
                media_type: "application/octet-stream".into(),
                source_extent_ids: vec![extent_id],
                derivation: None,
                restore_policy: RestorePolicy::EvidenceOnly,
                completeness: ArtifactCompleteness::Complete,
                data: extent_data,
            },
            ArtifactInput {
                id: "raw.inspect.last_lba".into(),
                kind: "raw_sectors".into(),
                media_type: "application/octet-stream".into(),
                source_extent_ids: vec![tail_extent_id],
                derivation: None,
                restore_policy: RestorePolicy::EvidenceOnly,
                completeness: ArtifactCompleteness::Complete,
                data: vec![0xa5; SECTOR],
            },
        ],
        notes: vec!["inspect 离线回归夹具".into()],
    };
    if deep {
        edpb::write_deep_backup(&path, &capture).ok()?;
    } else {
        edpb::write_metadata_backup(&path, &capture).ok()?;
    }
    Some((tmp, path, partition.start_sector))
}

#[test]
fn inspect_meta_backup_file_is_offline_and_renders_structure() {
    let Some((_tmp, path)) = fixture_edpb("netac", "inspect_cli_offline") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "meta"])
        .arg(&path)
        .args(["--lba", "7"])
        .output()
        .expect("run edpcli inspect");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("来源"));
    assert!(stdout.contains("LBA7"));
    assert!(stdout.contains("EDPF magic"));
    assert!(stdout.contains("协议解码"));
    assert!(stdout.contains("类型") || stdout.contains("EDPF magic"));
    assert!(!stdout.contains("+0x000:"));
    assert!(!stdout.contains("需要管理员权限"));
}

#[test]
fn inspect_backup_file_exports_selected_lbas() {
    let Some((tmp, copied)) = fixture_edpb("netac", "inspect_cli") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let export = tmp.0.join("out");
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "decode"])
        .arg(&copied)
        .args(["--lba", "11,12", "--backup-dir"])
        .arg(&tmp.0)
        .arg("--export")
        .arg(&export)
        .output()
        .expect("run edpcli inspect backup");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("onlyid=1402259934"));
    assert!(stdout.contains("PDKB"));
    assert!(stdout.contains("A6B0 整扇 512B"));
    for name in [
        "LBA11_decoded.bin",
        "LBA11_decoded.hex",
        "LBA12_decoded.bin",
        "LBA12_decoded.hex",
    ] {
        assert!(export.join(name).exists(), "missing export {name}");
    }
    assert!(!export.join("LBA11_raw.bin").exists());
}

#[test]
fn raw_and_meta_modes_have_distinct_output_contracts() {
    let Some((_tmp, target)) = fixture_edpb("aigo", "inspect_modes") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };

    let meta = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(["inspect", "meta", target.to_str().unwrap(), "--lba", "9"])
        .output()
        .unwrap();
    assert!(meta.status.success());
    let meta_stdout = String::from_utf8_lossy(&meta.stdout);
    assert!(meta_stdout.contains("LBA: 9"));
    assert!(meta_stdout.contains("区域:"));
    assert!(!meta_stdout.contains("+0x000:"));

    let raw = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(["inspect", "raw", target.to_str().unwrap(), "--lba", "9"])
        .output()
        .unwrap();
    assert!(raw.status.success());
    let raw_stdout = String::from_utf8_lossy(&raw.stdout);
    assert!(raw_stdout.contains("SHA-256"));
    assert!(raw_stdout.contains("+0x000:"));
}

#[test]
fn metadata_and_deep_edpb_inspect_captured_non_protocol_lba_in_all_modes() {
    for deep in [false, true] {
        let Some((_tmp, path, start)) = captured_partition_edpb(deep) else {
            eprintln!(
                "跳过: 无法构造离线 {} EDPB 夹具",
                if deep { "Deep" } else { "Metadata" }
            );
            continue;
        };
        let lba = (start + 1).to_string();

        let raw = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .env("NO_COLOR", "1")
            .args(["inspect", "raw"])
            .arg(&path)
            .args(["--lba", &lba])
            .output()
            .unwrap();
        assert_eq!(
            raw.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&raw.stderr)
        );
        let raw_stdout = String::from_utf8_lossy(&raw.stdout);
        assert!(raw_stdout.contains("+0x000: 5A 5A"), "{raw_stdout}");

        let meta = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .env("NO_COLOR", "1")
            .args(["inspect", "meta"])
            .arg(&path)
            .args(["--lba", &lba])
            .output()
            .unwrap();
        assert_eq!(
            meta.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&meta.stderr)
        );
        let meta_stdout = String::from_utf8_lossy(&meta.stdout);
        assert!(meta_stdout.contains("relative_lba=1"), "{meta_stdout}");
        assert!(
            meta_stdout.contains("物理数据状态: 物理明文文件系统 (exFAT)"),
            "{meta_stdout}"
        );
        assert!(meta_stdout.contains("decode 策略:"), "{meta_stdout}");

        let decode = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .env("NO_COLOR", "1")
            .args(["inspect", "decode"])
            .arg(&path)
            .args(["--lba", &lba])
            .output()
            .unwrap();
        assert_eq!(
            decode.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&decode.stderr)
        );
        let decode_stdout = String::from_utf8_lossy(&decode.stdout);
        assert!(
            decode_stdout.contains("物理盘面已为有效 exFAT 明文文件系统"),
            "{decode_stdout}"
        );
        assert!(decode_stdout.contains("+0x000: 5A 5A"), "{decode_stdout}");
    }
}

#[test]
fn edpb_accepts_last_legal_lba_and_rejects_total_sectors_before_read() {
    let Some((_tmp, path, _start)) = captured_partition_edpb(false) else {
        eprintln!("跳过: 无法构造离线 Metadata EDPB 夹具");
        return;
    };
    let total_sectors = 245_760_000u64;
    let last = (total_sectors - 1).to_string();
    let raw = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "raw"])
        .arg(&path)
        .args(["--lba", &last])
        .output()
        .unwrap();
    assert_eq!(
        raw.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&raw.stderr)
    );
    assert!(String::from_utf8_lossy(&raw.stdout).contains("+0x000: A5 A5"));

    let out_of_range = total_sectors.to_string();
    let rejected = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "raw"])
        .arg(&path)
        .args(["--lba", &out_of_range])
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("越界"), "{stderr}");
    assert!(
        !stderr.contains("EDPB 未采集"),
        "必须先做容量边界检查: {stderr}"
    );
}

#[test]
fn edpb_uncaptured_non_protocol_lba_is_explicit_error() {
    let Some((_tmp, path, start)) = captured_partition_edpb(false) else {
        eprintln!("跳过: 无法构造离线 Metadata EDPB 夹具");
        return;
    };
    let missing = (start + 2).to_string();
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "raw"])
        .arg(&path)
        .args(["--lba", &missing])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("EDPB 未采集"));
}
