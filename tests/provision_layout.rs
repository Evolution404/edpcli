use edpcli::protocol::{
    edpf::EdpPartitionType, lba7_compat::locate_lba7_compatibility_extent_from_geometry,
};
use edpcli::provision::{
    build_official_partition_layout, generate_official_image, official_mbr_partition_type,
    OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan,
    OfficialProvisionValidator, OnlyId, ProvisionEntropy, ProvisionImage, ProvisionMetadata,
    ProvisionProfile, ProvisionSpec, TargetIdentity, OFFICIAL_PARTITION_START_SECTOR,
    WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
};
use edpcli::{
    crypto::{a6b0_full, crc32_bare, xor_rolling},
    platform::{HardwareProbe, InquiryInfo, NativeTransport},
};

fn types(mode: OfficialPartitionMode) -> Vec<u32> {
    build_official_partition_layout(mode, OfficialPartitionSizes::new(32, 64, 128), 512)
        .unwrap()
        .into_iter()
        .map(|entry| entry.partition_type.raw())
        .collect()
}

#[test]
fn official_four_modes_emit_the_verified_partition_type_sequences() {
    assert_eq!(
        types(OfficialPartitionMode::DefaultThreePartition),
        vec![1, 2, 4]
    );
    assert_eq!(types(OfficialPartitionMode::BootShareCombined), vec![2, 4]);
    assert_eq!(types(OfficialPartitionMode::WholeDiskEncrypted), vec![1, 4]);
    assert_eq!(
        types(OfficialPartitionMode::IntranetExtranetDualPartition),
        vec![1, 2]
    );
}

#[test]
fn official_mbr_selector_matches_the_first_party_writer_branches() {
    assert_eq!(
        official_mbr_partition_type(OfficialPartitionMode::DefaultThreePartition),
        0x0e
    );
    assert_eq!(
        official_mbr_partition_type(OfficialPartitionMode::BootShareCombined),
        0x07
    );
    assert_eq!(
        official_mbr_partition_type(OfficialPartitionMode::WholeDiskEncrypted),
        0x0b
    );
    assert_eq!(
        official_mbr_partition_type(OfficialPartitionMode::IntranetExtranetDualPartition),
        0x0e
    );
}

#[test]
fn official_layouts_are_contiguous_from_lba63() {
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let layout =
            build_official_partition_layout(mode, OfficialPartitionSizes::new(32, 64, 128), 512)
                .unwrap();
        assert_eq!(layout[0].start_sector, OFFICIAL_PARTITION_START_SECTOR);
        for pair in layout.windows(2) {
            assert_eq!(pair[0].end_sector_exclusive(), pair[1].start_sector);
        }
    }
}

#[test]
fn whole_disk_encrypted_keeps_the_verified_0x7e00_type1_compatibility_geometry() {
    let layout = build_official_partition_layout(
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionSizes::new(999, 999, 128),
        512,
    )
    .unwrap();
    assert_eq!(layout.len(), 2);
    assert_eq!(layout[0].partition_type, EdpPartitionType::Boot);
    assert_eq!(layout[0].size_bytes, WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES);
    assert_eq!(layout[0].sector_count(), 63);
    assert_eq!(layout[1].partition_type, EdpPartitionType::Encrypt);
    assert_eq!(
        layout[1].size_bytes,
        128 * 1024 * 1024 - WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES
    );
}

#[test]
fn mode_specific_unused_size_fields_do_not_change_the_layout() {
    let a = build_official_partition_layout(
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionSizes::new(1, 64, 128),
        512,
    )
    .unwrap();
    let b = build_official_partition_layout(
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionSizes::new(999, 64, 128),
        512,
    )
    .unwrap();
    assert_eq!(a, b);
}

fn official_spec() -> ProvisionSpec {
    let probe = HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: NativeTransport::Uas,
        inquiry: Some(InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, 16_777_216).unwrap();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "USER06",
        "江苏省电力有限公司",
        "江苏电力!SAFE6",
    )
    .unwrap();
    ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap()
}

fn official_plan(mode: OfficialPartitionMode) -> OfficialProvisionPlan {
    let compat = locate_lba7_compatibility_extent_from_geometry(1024, 255, 63, 512).unwrap();
    OfficialProvisionPlan::new(mode, OfficialPartitionSizes::new(32, 64, 128), compat).unwrap()
}

fn u32le(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn u64le(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap())
}

#[test]
fn official_image_generator_emits_all_four_verified_layouts() {
    let spec = official_spec();
    let entropy = ProvisionEntropy::new([0x5a; 252]);
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = official_plan(mode);
        let logical = plan.logical_partitions(512).unwrap();
        let image = generate_official_image(&spec, &entropy, &plan).unwrap();
        let bytes = image.as_bytes();
        let validation = OfficialProvisionValidator::validate(&spec, &image, &plan).unwrap();
        assert_eq!(validation.mode(), mode);
        assert_eq!(validation.device_id(), spec.target().device_id());
        assert_eq!(validation.onlyid(), spec.metadata().onlyid().text());

        let mbr = &bytes[..512];
        assert_eq!(mbr[0x1be + 4], official_mbr_partition_type(mode));
        assert_eq!(u32le(mbr, 0x1be + 8), 63);
        assert_eq!(u32le(mbr, 0x1be + 12) as u64, logical[0].sector_count());

        let crc = crc32_bare(spec.target().device_id().as_bytes());
        let lba7_wire = &bytes[7 * 512..8 * 512];
        let lba7 = xor_rolling(lba7_wire, (crc & 0xffff) ^ (crc >> 16));
        let lba12_wire = &bytes[12 * 512..13 * 512];
        let lba12 = a6b0_full(lba12_wire, &crc.to_le_bytes(), 0);
        assert_eq!(u32le(&lba7, 0x08), logical.len() as u32);
        assert_eq!(u32le(&lba12, 0x08), logical.len() as u32);

        for (index, partition) in logical.iter().enumerate() {
            let b7 = index * 0x40;
            let b12 = index * 0x60;
            let ptype = partition.partition_type.raw();
            assert_eq!(u32le(&lba7, b7 + 0x0c), ptype);
            assert_eq!(u32le(&lba12, b12 + 0x0c), ptype);
            assert_eq!(u32le(&lba7, b7 + 0x10), u32::from(index < 2));
            assert_eq!(u32le(&lba12, b12 + 0x10), u32::from(index < 2));
            assert_eq!(u32le(&lba7, b7 + 0x14), u32::from(ptype != 1));
            assert_eq!(u32le(&lba12, b12 + 0x14), u32::from(ptype != 1));

            assert_eq!(u64le(&lba12, b12 + 0x18), partition.start_sector);
            assert_eq!(u64le(&lba12, b12 + 0x28), partition.size_bytes);
            if index == 0 {
                assert_eq!(u64le(&lba7, b7 + 0x18), partition.start_sector);
                assert_eq!(u64le(&lba7, b7 + 0x28), partition.size_bytes);
            } else {
                assert_eq!(
                    u64le(&lba7, b7 + 0x18),
                    plan.lba7_compatibility_extent.start_lba
                );
                assert_eq!(
                    u64le(&lba7, b7 + 0x28),
                    plan.lba7_compatibility_extent.size_bytes
                );
            }
        }
    }
}

#[test]
fn official_validator_rejects_mbr_and_lba12_tamper() {
    let spec = official_spec();
    let entropy = ProvisionEntropy::new([0x5a; 252]);
    let plan = official_plan(OfficialPartitionMode::DefaultThreePartition);
    let image = generate_official_image(&spec, &entropy, &plan).unwrap();

    let mut bad_mbr = image.as_bytes().to_vec();
    bad_mbr[0x1be + 4] ^= 0x01;
    let bad_mbr = ProvisionImage::from_bytes(bad_mbr).unwrap();
    let err = OfficialProvisionValidator::validate(&spec, &bad_mbr, &plan).unwrap_err();
    assert!(err.contains("MBR"), "{err}");

    let mut bad_lba12 = image.as_bytes().to_vec();
    bad_lba12[12 * 512 + 0x20] ^= 0x01;
    let bad_lba12 = ProvisionImage::from_bytes(bad_lba12).unwrap();
    let err = OfficialProvisionValidator::validate(&spec, &bad_lba12, &plan).unwrap_err();
    assert!(err.contains("LBA12") || err.contains("EDPF"), "{err}");
}
