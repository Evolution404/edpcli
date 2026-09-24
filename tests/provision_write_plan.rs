use edpcli::{
    backup_metadata::parse_partition_geometry,
    platform::{HardwareProbe, InquiryInfo, NativeTransport},
    protocol::lba7_compat::{
        locate_lba7_compatibility_extent_from_verified_usb_capacity, Lba7CompatibilityExtentLayout,
    },
    provision::{
        build_official_provision_protocol_image, build_official_provision_write_image,
        wrap_file_key, wrap_legacy_lba7_file_key, FileKeyWrapMode, OfficialPartitionMode,
        OfficialPartitionSizes, OfficialProvisionPlan, OnlyId, ProvisionEntropy, ProvisionMetadata,
        ProvisionProfile, ProvisionSpec, TargetIdentity,
    },
};

const FILE_KEY: [u8; 16] = [
    0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66, 0xcb, 0x42,
];

fn spec(total_sectors: u64) -> ProvisionSpec {
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
    let target = TargetIdentity::from_probe(&probe, total_sectors).unwrap();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "USER06",
        "江苏省电力有限公司",
        "江苏电力!SAFE6",
    )
    .unwrap();
    ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap()
}

fn plan(mode: OfficialPartitionMode, total_sectors: u64) -> OfficialProvisionPlan {
    let compat = locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, 512)
        .expect("verified USB geometry");
    OfficialProvisionPlan::new(
        mode,
        OfficialPartitionSizes::new(32, 64, 128),
        compat,
        wrap_legacy_lba7_file_key(
            b"0000aaaa",
            [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38],
        ),
        wrap_file_key(b"ProofPass1!", FILE_KEY, FileKeyWrapMode::Sm4),
    )
    .unwrap()
}

#[test]
fn verified_usb_capacity_locator_matches_all_three_physical_fixtures() {
    // Physical total sectors are intentionally not used as a fixed-distance
    // locator.  Each sample is first translated down to a complete 255x63 CHS
    // cylinder exactly like the first-party DISK_GEOMETRY path.
    for (total_sectors, expected_lce) in [
        (243_625_984u64, 243_623_933u64),
        (245_746_688u64, 245_744_513u64),
        (240_254_976u64, 240_250_283u64),
    ] {
        let layout =
            locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, 512)
                .unwrap();
        assert_eq!(layout.start_lba, expected_lce);
        assert_eq!(layout.size_sectors, 6);
        assert_eq!(layout.size_bytes, 0xC00);
    }
    assert!(locate_lba7_compatibility_extent_from_verified_usb_capacity(1_000_000, 4096).is_none());
}

#[test]
fn all_four_modes_build_one_bounded_write_image() {
    let total = 16_777_216u64;
    let spec = spec(total);
    let entropy = ProvisionEntropy::new([0x5a; 252]);
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = plan(mode, total);
        let logical_count = plan.logical_partitions(512).unwrap().len();
        let serials = (0..logical_count)
            .map(|i| 0x1234_0000u32 + i as u32)
            .collect::<Vec<_>>();
        let image = build_official_provision_write_image(
            &spec, &entropy, &plan, &FILE_KEY, "SAFE6", &serials,
        )
        .unwrap();

        for lba in 0..13u32 {
            assert!(
                image.patch.contains_key(&lba),
                "mode {mode:?} missing LBA{lba}"
            );
        }
        for offset in 0..6u32 {
            assert!(image
                .patch
                .contains_key(&(plan.lba7_compatibility_extent.start_lba as u32 + offset)));
        }
        assert_eq!(image.touched_sector_count(), 19);
        assert!(u64::from(image.highest_touched_lba().unwrap()) < total);
    }
}

#[test]
fn protocol_only_phase_never_writes_a_filesystem_partition() {
    let total = 16_777_216u64;
    let spec = spec(total);
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = plan(mode, total);
        let image = build_official_provision_protocol_image(
            &spec,
            &ProvisionEntropy::new([0x5a; 252]),
            &plan,
        )
        .unwrap();
        let geometry =
            parse_partition_geometry(image.metadata.as_bytes(), spec.target().device_id(), total)
                .unwrap();
        assert_eq!(geometry.len(), plan.logical_partitions(512).unwrap().len());
        let lce = plan.lba7_compatibility_extent.start_lba as u32;
        assert_eq!(image.patch.len(), 19);
        assert!(image
            .patch
            .keys()
            .all(|&lba| lba <= 12 || (lce..lce + 6).contains(&lba)));
        for partition in plan.logical_partitions(512).unwrap() {
            assert!(!image.patch.contains_key(&(partition.start_sector as u32)));
        }
    }
}

#[test]
fn write_image_rejects_lce_overlap_with_a_filesystem() {
    let total = 16_777_216u64;
    let spec = spec(total);
    let mut bad = plan(OfficialPartitionMode::DefaultThreePartition, total);
    bad.lba7_compatibility_extent = Lba7CompatibilityExtentLayout {
        chs_bytes: 0,
        start_byte_offset: 63 * 512,
        start_lba: 63,
        size_bytes: 0xC00,
        size_sectors: 6,
    };
    let err = build_official_provision_write_image(
        &spec,
        &ProvisionEntropy::new([0x5a; 252]),
        &bad,
        &FILE_KEY,
        "SAFE6",
        &[1, 2, 3],
    )
    .unwrap_err();
    assert!(err.contains("overlaps") || err.contains("LCE"), "{err}");
}
