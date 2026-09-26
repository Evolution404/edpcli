use super::*;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn portable_test_temp_file(stem: &str, extension: &str) -> std::path::PathBuf {
    let sequence = TEST_TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{stem}-{}-{sequence}.{extension}",
        std::process::id()
    ))
}

struct Lba3Dev([u8; SECTOR]);

impl SectorDev for Lba3Dev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        if lba != 3 {
            return Err(io::Error::other("unexpected read"));
        }
        Ok(self.0.to_vec())
    }

    fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
        Err(io::Error::other("read-only test device"))
    }
}

#[test]
fn mode2_quick_and_exact_encrypt_capacity_are_partition_scoped() {
    let quick = target_encrypt_capacity_override(
        OfficialPartitionMode::WholeDiskEncrypted,
        Some(128),
        None,
    )
    .unwrap()
    .unwrap();
    let exact = target_encrypt_capacity_override(
        OfficialPartitionMode::WholeDiskEncrypted,
        None,
        Some(128 * 2048),
    )
    .unwrap()
    .unwrap();
    assert_eq!(quick.sectors(), 128 * 2048);
    assert_eq!(exact.sectors(), 128 * 2048);
    assert_eq!(quick.sectors(), exact.sectors());
}

#[test]
fn preserve_requires_a_full_prewrite_source_metadata_snapshot() {
    assert!(validate_preserve_source_snapshot(false, None).is_ok());
    assert!(validate_preserve_source_snapshot(true, None).is_err());
    assert!(validate_preserve_source_snapshot(true, Some(&vec![0; 12 * SECTOR])).is_err());
    assert!(validate_preserve_source_snapshot(true, Some(&vec![0; 13 * SECTOR])).is_ok());
}

#[test]
fn manufacturer_lba3_is_copied_verbatim_into_the_write_plan() {
    let metadata = ProvisionImage::from_bytes(vec![0; 13 * SECTOR]).unwrap();
    let mut patch = BTreeMap::new();
    patch.insert(3, vec![0; SECTOR]);
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: crate::platform::NativeTransport::Uas,
        inquiry: None,
    };
    let mut prepared = PreparedNewProvision {
        disk: 4,
        device_id: "disk&ven_netac&prod_onlydisk".into(),
        mode: OfficialPartitionMode::BootShareCombined,
        force_change_password: false,
        pass_info_policy: PassInfoPolicy::default(),
        lce_start_lba: 900,
        write_image: OfficialProvisionWriteImage {
            metadata,
            total_sectors: 1024,
            patch,
        },
        format_targets: Vec::new(),
        target_plan: None,
        source_metadata: None,
        plan: OfficialProvisionPlan::new(
            OfficialPartitionMode::BootShareCombined,
            OfficialPartitionSizes::new(32, 64, 128),
            crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
                chs_bytes: 0,
                start_byte_offset: 0,
                start_lba: 900,
                size_bytes: 3072,
                size_sectors: 6,
            },
            wrap_legacy_lba7_file_key(b"0000aaaa", [0; 8]),
            wrap_file_key(b"0000aaaa", [0; 16], FileKeyWrapMode::Sm4),
        )
        .unwrap(),
        expected_onlyid: "1".into(),
        expected_serial: None,
        expected_probe: probe,
        expected_lba3: None,
    };
    let expected = [0xa5; SECTOR];
    let mut dev = Lba3Dev(expected);
    capture_manufacturer_lba3(&mut dev, &mut prepared).unwrap();
    assert_eq!(prepared.write_image.patch.get(&3).unwrap(), &expected);
    assert_eq!(prepared.expected_lba3, Some(expected));
}

#[derive(Default)]
struct MemoryDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    fail_at: Option<u32>,
}

impl SectorDev for MemoryDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0; SECTOR]))
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        if self.fail_at == Some(lba) {
            return Err(io::Error::other("injected format failure"));
        }
        self.sectors.insert(lba, data.to_vec());
        Ok(())
    }
}

#[test]
fn plain_prewrite_snapshot_rejects_stale_lba7_metadata() {
    let total_sectors = 100_000;
    let plan = PlainProvisionPlan::default_for_disk(total_sectors).unwrap();
    let write_plan = build_plain_provision_write_plan(&plan, None, &[0x1234_5678]).unwrap();
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x3535),
        pid: Some(0x6300),
        transport: crate::platform::NativeTransport::Uas,
        inquiry: None,
    };
    let mut source_metadata = vec![0u8; 13 * SECTOR];
    source_metadata[3 * SECTOR..4 * SECTOR].fill(0xa5);
    let prepared = PreparedPlainProvision {
        disk: 4,
        device_id: "disk&ven_aigo&prod_u335".into(),
        plan,
        write_plan,
        source_kind: crate::provision::DiskProvisionKind::Plain,
        source_lce_start_lba: None,
        source_metadata: source_metadata.clone(),
        expected_probe: probe,
    };
    let mut dev = MemoryDev::default();
    for lba in 0..13u32 {
        dev.sectors.insert(
            lba,
            source_metadata[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec(),
        );
    }

    verify_reopened_snapshot(&mut dev, &prepared.source_metadata).unwrap();
    dev.sectors.get_mut(&7).unwrap()[0] ^= 1;
    let error = verify_reopened_snapshot(&mut dev, &prepared.source_metadata).unwrap_err();
    assert!(error.msg.contains("LBA7"));
}

fn format_test_plan(mode: OfficialPartitionMode, key: &[u8; 16]) -> OfficialProvisionPlan {
    OfficialProvisionPlan::new(
        mode,
        OfficialPartitionSizes::new(32, 64, 128),
        crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
            chs_bytes: 0,
            start_byte_offset: 4_194_000 * SECTOR as u64,
            start_lba: 4_194_000,
            size_bytes: 3072,
            size_sectors: 6,
        },
        wrap_legacy_lba7_file_key(b"0000aaaa", [0; 8]),
        wrap_file_key(b"0000aaaa", *key, FileKeyWrapMode::Sm4),
    )
    .unwrap()
}

#[test]
fn format_executor_uses_the_same_matrix_and_preserves_protocol_sectors() {
    let key = [0x42; 16];
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = format_test_plan(mode, &key);
        let serials = vec![0x1234_5678; plan.format_targets().unwrap().len()];
        let options = FormatOptions {
            boot: mode != OfficialPartitionMode::WholeDiskEncrypted
                && mode != OfficialPartitionMode::BootShareCombined,
            share: mode != OfficialPartitionMode::WholeDiskEncrypted,
            encrypt: mode != OfficialPartitionMode::IntranetExtranetDualPartition,
            ..FormatOptions::default()
        };
        let choices = plan_format_targets(&plan, &options, &serials, &key).unwrap();
        let mut dev = MemoryDev::default();
        for lba in 0..13u32 {
            dev.sectors.insert(lba, vec![lba as u8; SECTOR]);
        }
        for choice in choices.iter().filter(|choice| choice.selected) {
            execute_partition_format(&mut dev, choice).unwrap();
            let raw = dev
                .read_sector(choice.target.geometry.start_sector as u32)
                .unwrap();
            if choice.target.physically_encrypted {
                assert_ne!(raw.get(3..11), Some(&b"EXFAT   "[..]));
                assert_ne!(raw.get(54..62), Some(&b"FAT16   "[..]));
            } else if choice.filesystem == Some(OfficialFilesystemFormat::Fat16) {
                assert_eq!(raw.get(54..62), Some(&b"FAT16   "[..]));
            } else {
                assert_eq!(raw.get(3..11), Some(&b"EXFAT   "[..]));
            }
        }
        for lba in 0..13u32 {
            assert_eq!(dev.read_sector(lba).unwrap(), vec![lba as u8; SECTOR]);
        }
        if mode == OfficialPartitionMode::WholeDiskEncrypted {
            assert!(!dev.sectors.contains_key(&63));
        }
    }
}

#[test]
fn format_failure_keeps_the_protocol_and_prior_successful_partition() {
    let key = [0x42; 16];
    let plan = format_test_plan(OfficialPartitionMode::DefaultThreePartition, &key);
    let choices = plan_format_targets(
        &plan,
        &FormatOptions {
            boot: true,
            share: true,
            ..FormatOptions::default()
        },
        &[1, 2, 3],
        &key,
    )
    .unwrap();
    let mut dev = MemoryDev::default();
    for lba in 0..13u32 {
        dev.sectors.insert(lba, vec![0xa5; SECTOR]);
    }
    execute_partition_format(&mut dev, &choices[0]).unwrap();
    dev.fail_at = Some(choices[1].target.geometry.start_sector as u32);
    assert!(execute_partition_format(&mut dev, &choices[1]).is_err());
    assert_eq!(
        &dev.read_sector(choices[0].target.geometry.start_sector as u32)
            .unwrap()[54..62],
        b"FAT16   "
    );
    for lba in 0..13u32 {
        assert_eq!(dev.read_sector(lba).unwrap(), vec![0xa5; SECTOR]);
    }
}

#[test]
fn portable_test_temp_file_name_uses_safe_ascii_components() {
    let path = portable_test_temp_file("edpcli-provision-export", "img");
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .expect("portable test temp path must have a UTF-8 file name");
    assert!(name.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
    }));
    assert!(!name.contains(':'));
}

#[test]
fn sparse_export_includes_selected_format_images() {
    let key = [0x42; 16];
    let plan = format_test_plan(OfficialPartitionMode::DefaultThreePartition, &key);
    let choices = plan_format_targets(
        &plan,
        &FormatOptions {
            boot: true,
            ..FormatOptions::default()
        },
        &[0x1234_5678, 2, 3],
        &key,
    )
    .unwrap();
    let mut patch = BTreeMap::new();
    patch.insert(0, vec![0x5a; SECTOR]);
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x3535),
        pid: Some(0x6300),
        transport: crate::platform::NativeTransport::Uas,
        inquiry: None,
    };
    let prepared = PreparedNewProvision {
        disk: 4,
        device_id: "disk&ven_aigo&prod_u335".into(),
        mode: plan.mode,
        force_change_password: false,
        pass_info_policy: PassInfoPolicy::default(),
        lce_start_lba: plan.lba7_compatibility_extent.start_lba,
        write_image: OfficialProvisionWriteImage {
            metadata: ProvisionImage::from_bytes(vec![0; 13 * SECTOR]).unwrap(),
            total_sectors: 1_000_000,
            patch,
        },
        format_targets: choices,
        target_plan: None,
        source_metadata: None,
        plan,
        expected_onlyid: "1".into(),
        expected_serial: None,
        expected_probe: probe,
        expected_lba3: None,
    };
    let path = portable_test_temp_file("edpcli-provision-export", "img");
    let _ = std::fs::remove_file(&path);
    export_sparse_provision_image(&path, &prepared).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    use std::io::{Read, Seek};
    let mut mbr = [0u8; SECTOR];
    file.read_exact(&mut mbr).unwrap();
    assert_eq!(mbr, [0x5a; SECTOR]);
    file.seek(SeekFrom::Start(63 * SECTOR as u64)).unwrap();
    let mut boot = [0u8; SECTOR];
    file.read_exact(&mut boot).unwrap();
    assert_eq!(&boot[54..62], b"FAT16   ");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn format_hardware_gate_rejects_changed_serial_probe_capacity_and_device_id() {
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x3535),
        pid: Some(0x6300),
        transport: crate::platform::NativeTransport::Uas,
        inquiry: Some(crate::platform::InquiryInfo {
            vendor: "aigo".into(),
            product: "U335".into(),
            revision: "PMAP".into(),
        }),
    };
    let total = 16_777_216;
    let device_id = TargetIdentity::from_probe(&probe, total)
        .unwrap()
        .device_id()
        .to_string();
    let check =
        |fresh: &crate::platform::HardwareProbe, capacity, serial: Option<&str>, id: &str| {
            verify_format_hardware(&probe, total, id, Some("SERIAL-1"), fresh, capacity, serial)
        };
    assert!(check(&probe, total, Some("SERIAL-1"), &device_id).is_ok());
    assert!(check(&probe, total, Some("SERIAL-2"), &device_id).is_err());
    assert!(check(&probe, total, None, &device_id).is_err());
    assert!(check(&probe, total + 1, Some("SERIAL-1"), &device_id).is_err());
    assert!(check(&probe, total, Some("SERIAL-1"), "disk&ven_other&prod_other").is_err());
    let mut changed = probe.clone();
    changed.vid = Some(0x0951);
    assert!(check(&changed, total, Some("SERIAL-1"), &device_id).is_err());
    let mut changed = probe.clone();
    changed.inquiry.as_mut().unwrap().revision = "DIFF".into();
    assert!(check(&changed, total, Some("SERIAL-1"), &device_id).is_err());
}

#[test]
fn protocol_readback_gate_rejects_changed_onlyid_and_layout() {
    let total = 16_777_216u64;
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: crate::platform::NativeTransport::Uas,
        inquiry: Some(crate::platform::InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total).unwrap();
    let device_id = target.device_id().to_string();
    let spec = ProvisionSpec::new(
        target,
        ProvisionMetadata::new(
            OnlyId::parse("1402259934").unwrap(),
            "USER06",
            "江苏省电力有限公司",
            "江苏电力!SAFE6",
        )
        .unwrap(),
        ProvisionProfile::canonical_v1(),
    )
    .unwrap();
    let plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionSizes::new(32, 64, 128),
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total, 512).unwrap(),
        wrap_legacy_lba7_file_key(b"0000aaaa", [0x7d; 8]),
        wrap_file_key(b"0000aaaa", [0x42; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap();
    let write_image =
        build_official_provision_protocol_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &plan)
            .unwrap();
    let mut dev = MemoryDev {
        sectors: write_image.patch.clone(),
        fail_at: None,
    };
    let prepared = PreparedNewProvision {
        disk: 4,
        device_id,
        mode: plan.mode,
        force_change_password: false,
        pass_info_policy: PassInfoPolicy::default(),
        lce_start_lba: plan.lba7_compatibility_extent.start_lba,
        write_image,
        format_targets: vec![],
        target_plan: None,
        source_metadata: None,
        plan,
        expected_onlyid: "1402259934".into(),
        expected_serial: None,
        expected_probe: probe,
        expected_lba3: Some([0; SECTOR]),
    };
    verify_protocol_readback(&mut dev, &prepared).unwrap();
    dev.sectors.get_mut(&4).unwrap()[4] ^= 1;
    assert!(verify_protocol_readback(&mut dev, &prepared).is_err());
    dev.sectors
        .insert(4, prepared.write_image.patch[&4].clone());
    dev.sectors.get_mut(&12).unwrap()[0] ^= 1;
    assert!(verify_protocol_readback(&mut dev, &prepared).is_err());
}
