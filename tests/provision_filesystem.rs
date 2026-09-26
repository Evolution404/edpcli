use std::io;

use edpcli::{
    application::provision::{plan_format_targets, FormatOptions},
    backup_deep::{analyze_partition, keys::decrypt_mode2, AnalysisStatus, PartitionReader},
    backup_metadata::PartitionGeometry,
    protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry,
    provision::{
        build_empty_exfat, build_official_exfat_partitions, build_official_partition_filesystem,
        encrypt_sparse_mode2, wrap_file_key, wrap_legacy_lba7_file_key, FileKeyWrapMode,
        OfficialFilesystemFormat, OfficialPartitionFilesystems, OfficialPartitionMode,
        OfficialPartitionSizes, OfficialProvisionPlan, PartitionRole, SparseFilesystemImage,
    },
};

struct ImageReader<'a> {
    image: &'a SparseFilesystemImage,
    decrypt_key: Option<[u8; 16]>,
}

impl PartitionReader for ImageReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> io::Result<Vec<u8>> {
        let sector = self
            .image
            .sector_or_zero(relative_lba)
            .ok_or_else(|| io::Error::other("sector out of range"))?;
        if let Some(key) = self.decrypt_key {
            decrypt_mode2(&sector, &key).map_err(io::Error::other)
        } else {
            Ok(sector.to_vec())
        }
    }
}

fn geometry(sectors: u64) -> PartitionGeometry {
    PartitionGeometry {
        index: 0,
        partition_type: 2,
        partition_count: 1,
        need_disturb: 1,
        need_encrypt: 0,
        start_sector: 63,
        sector_size: 512,
        partition_size: sectors * 512,
        sector_count: sectors,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    }
}

const FILE_KEY: [u8; 16] = [
    0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66, 0xcb, 0x42,
];

fn official_plan(mode: OfficialPartitionMode) -> OfficialProvisionPlan {
    let compat = locate_lba7_compatibility_extent_from_geometry(1024, 255, 63, 512).unwrap();
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
fn first_party_filesystem_config_defaults_and_normalizes_like_the_writer() {
    assert_eq!(
        OfficialFilesystemFormat::first_party_default(),
        OfficialFilesystemFormat::ExFat
    );
    for value in [
        None,
        Some("exfat"),
        Some("exFat"),
        Some("unknown"),
        Some("fat16"),
    ] {
        assert_eq!(
            OfficialFilesystemFormat::from_first_party_config(value),
            OfficialFilesystemFormat::ExFat
        );
    }
    assert_eq!(
        OfficialFilesystemFormat::from_first_party_config(Some("NTFS")),
        OfficialFilesystemFormat::Ntfs
    );
    assert_eq!(
        OfficialFilesystemFormat::from_first_party_config(Some("fat32")),
        OfficialFilesystemFormat::Fat32
    );
    assert_eq!(OfficialFilesystemFormat::Ntfs.windows_format_name(), "NTFS");
    assert_eq!(
        OfficialFilesystemFormat::ExFat.windows_format_name(),
        "exFat"
    );
}

#[test]
fn portable_empty_exfat_round_trips_through_the_existing_deep_parser() {
    let volume_sectors = 256 * 1024 * 1024 / 512;
    let image = build_empty_exfat(63, volume_sectors, 0x1234_5678, "EDPTEST").unwrap();
    assert_eq!(image.volume_sectors(), volume_sectors);
    assert!(image.metadata_bytes() < 2 * 1024 * 1024);
    assert_eq!(&image.sector_or_zero(0).unwrap()[3..11], b"EXFAT   ");

    let partition = geometry(volume_sectors);
    let mut reader = ImageReader {
        image: &image,
        decrypt_key: None,
    };
    let report = analyze_partition(&partition, &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.filesystem.as_deref(), Some("exfat"));
    assert_eq!(report.file_count, Some(0));
    assert_eq!(report.directory_count, Some(0));
    assert_eq!(
        report
            .entries
            .as_ref()
            .unwrap()
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        ["/"]
    );
}

#[test]
#[ignore = "Q0 red contract: enable when adaptive exFAT geometry is implemented before Q8"]
fn ch14_q0_475_gib_exfat_formatter_round_trips_through_canonical_parser() {
    const VOLUME_SECTORS: u64 = 998_107_136;
    let image = build_empty_exfat(63, VOLUME_SECTORS, 0x1234_5678, "EDPTEST")
        .expect("formatter must choose a geometry within the parser's validated domain");
    let boot = image.sector_or_zero(0).unwrap();
    let cluster_count = u32::from_le_bytes(boot[92..96].try_into().unwrap());
    assert!(cluster_count <= 4_194_304);
    assert!(
        boot[109] >= 8,
        "64 KiB clusters exceed this geometry's budget"
    );

    let mut reader = ImageReader {
        image: &image,
        decrypt_key: None,
    };
    let report = analyze_partition(&geometry(VOLUME_SECTORS), &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.filesystem.as_deref(), Some("exfat"));
}

#[test]
fn mode2_sparse_encryption_round_trips_to_the_same_valid_exfat() {
    let volume_sectors = 512 * 1024 * 1024 / 512;
    let plain = build_empty_exfat(2048, volume_sectors, 0x89ab_cdef, "SAFE6").unwrap();
    let key = [
        0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66, 0xcb,
        0x42,
    ];
    let encrypted = encrypt_sparse_mode2(&plain, &key);
    assert_eq!(encrypted.volume_sectors(), plain.volume_sectors());
    assert_ne!(
        encrypted.sectors().get(&0).unwrap(),
        plain.sectors().get(&0).unwrap()
    );

    for (&lba, expected) in plain.sectors() {
        let cipher = encrypted.sectors().get(&lba).unwrap();
        assert_eq!(decrypt_mode2(cipher, &key).unwrap(), expected);
    }

    let partition = geometry(volume_sectors);
    let mut reader = ImageReader {
        image: &encrypted,
        decrypt_key: Some(key),
    };
    let report = analyze_partition(&partition, &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.filesystem.as_deref(), Some("exfat"));
    assert_eq!(report.file_count, Some(0));
}

#[test]
fn all_four_modes_build_the_verified_plaintext_and_encrypted_exfat_matrix() {
    let cases = [
        (
            OfficialPartitionMode::DefaultThreePartition,
            vec![false, true, true],
        ),
        (OfficialPartitionMode::BootShareCombined, vec![false, true]),
        (OfficialPartitionMode::WholeDiskEncrypted, vec![true]),
        (
            OfficialPartitionMode::IntranetExtranetDualPartition,
            vec![false, true],
        ),
    ];
    for (mode, expected_encryption) in cases {
        let plan = official_plan(mode);
        let logical_count = plan.logical_partitions(512).unwrap().len();
        let serials = (0..logical_count)
            .map(|index| 0x1111_0001u32.wrapping_add(index as u32))
            .collect::<Vec<_>>();
        let images = build_official_exfat_partitions(&plan, &FILE_KEY, "SAFE6", &serials).unwrap();
        assert_eq!(
            images
                .iter()
                .map(|image| image.physically_encrypted)
                .collect::<Vec<_>>(),
            expected_encryption,
            "mode {mode:?}"
        );
        for image in images {
            let raw = image.image.sectors().get(&0).unwrap();
            if image.physically_encrypted {
                assert_ne!(
                    &raw[3..11],
                    b"EXFAT   ",
                    "encrypted physical boot leaked in mode {mode:?}"
                );
            }
            let boot = if image.physically_encrypted {
                decrypt_mode2(raw, &FILE_KEY).unwrap()
            } else {
                raw.to_vec()
            };
            assert_eq!(&boot[3..11], b"EXFAT   ", "mode {mode:?}");
            assert_eq!(
                u64::from_le_bytes(boot[64..72].try_into().unwrap()),
                image.geometry.start_sector
            );
            assert_eq!(
                u64::from_le_bytes(boot[72..80].try_into().unwrap()),
                image.geometry.sector_count()
            );
        }
    }
}

#[test]
fn formatting_defaults_off_and_selects_only_actual_mode_targets() {
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = official_plan(mode);
        let serials = (0..plan.logical_partitions(512).unwrap().len())
            .map(|i| i as u32 + 1)
            .collect::<Vec<_>>();
        let defaults =
            plan_format_targets(&plan, &FormatOptions::default(), &serials, &FILE_KEY).unwrap();
        assert!(defaults.iter().all(|choice| !choice.selected));
        assert!(defaults
            .iter()
            .all(|choice| choice.prepared_image.is_none()));
        assert!(defaults
            .iter()
            .all(|choice| choice.verification_image.is_none()));
        if mode == OfficialPartitionMode::WholeDiskEncrypted {
            assert_eq!(defaults[0].target.role, PartitionRole::CompatibilityReserve);
            assert!(!defaults[0].target.format_capable);
            let invalid = FormatOptions {
                boot: true,
                ..Default::default()
            };
            assert!(plan_format_targets(&plan, &invalid, &serials, &FILE_KEY).is_err());
        }
        let mut options = FormatOptions::default();
        if defaults.iter().any(|choice| {
            choice.target.role == PartitionRole::Share
                || choice.target.role == PartitionRole::BootShareCombined
        }) {
            options.share = true;
            options.share_label = "自定义交换".into();
        } else {
            options.encrypt = true;
            options.encrypt_label = "自定义数据".into();
        }
        let selected = plan_format_targets(&plan, &options, &serials, &FILE_KEY).unwrap();
        assert_eq!(selected.iter().filter(|choice| choice.selected).count(), 1);
        let choice = selected.iter().find(|choice| choice.selected).unwrap();
        let image = choice.prepared_image.as_ref().unwrap();
        assert!(choice.verification_image.is_some());
        assert_eq!(
            image.physically_encrypted,
            choice.target.physically_encrypted
        );
        let raw = image.image.sectors().get(&0).unwrap();
        if choice.target.physically_encrypted {
            assert_ne!(&raw[3..11], b"EXFAT   ");
        }
        let boot = if choice.target.physically_encrypted {
            decrypt_mode2(raw, &FILE_KEY).unwrap()
        } else {
            raw.to_vec()
        };
        assert_eq!(&boot[3..11], b"EXFAT   ");
        assert_eq!(
            u64::from_le_bytes(boot[64..72].try_into().unwrap()),
            choice.target.geometry.start_sector
        );
        assert_eq!(
            u64::from_le_bytes(boot[72..80].try_into().unwrap()),
            choice.target.geometry.sector_count()
        );
        assert_eq!(
            u32::from_le_bytes(boot[100..104].try_into().unwrap()),
            choice.volume_serial
        );
        let mut reader = ImageReader {
            image: &image.image,
            decrypt_key: choice.target.physically_encrypted.then_some(FILE_KEY),
        };
        let report = analyze_partition(
            &geometry(choice.target.geometry.sector_count()),
            &mut reader,
        );
        assert_eq!(
            report.status,
            AnalysisStatus::Parsed,
            "{mode:?}: {}",
            report.reason
        );
        assert_eq!(report.file_count, Some(0));
    }
}

#[test]
fn filesystem_stage_fails_closed_on_wrong_key_or_unsupported_portable_profile() {
    let plan = official_plan(OfficialPartitionMode::BootShareCombined);
    let serials = [1, 2];
    let mut wrong = FILE_KEY;
    wrong[0] ^= 1;
    assert!(
        build_official_exfat_partitions(&plan, &wrong, "SAFE6", &serials)
            .unwrap_err()
            .contains("FileKeyCRC")
    );

    let compat = locate_lba7_compatibility_extent_from_geometry(1024, 255, 63, 512).unwrap();
    let ntfs_plan = OfficialProvisionPlan::new_with_filesystem(
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionSizes::new(32, 64, 128),
        OfficialFilesystemFormat::Ntfs,
        compat,
        wrap_legacy_lba7_file_key(
            b"0000aaaa",
            [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38],
        ),
        wrap_file_key(b"ProofPass1!", FILE_KEY, FileKeyWrapMode::Sm4),
    )
    .unwrap();
    assert!(
        build_official_exfat_partitions(&ntfs_plan, &FILE_KEY, "SAFE6", &serials)
            .unwrap_err()
            .contains("does not yet implement ntfs")
    );
    assert!(
        plan_format_targets(&ntfs_plan, &FormatOptions::default(), &serials, &FILE_KEY).is_err()
    );
}

#[test]
fn fat16_and_exfat_can_each_be_physically_encrypted_when_selected() {
    let plan = official_plan(OfficialPartitionMode::DefaultThreePartition).with_filesystems(
        OfficialPartitionFilesystems {
            boot: OfficialFilesystemFormat::ExFat,
            share: OfficialFilesystemFormat::Fat16,
            encrypt: OfficialFilesystemFormat::ExFat,
        },
    );
    let targets = plan.format_targets().unwrap();
    let front =
        build_official_partition_filesystem(&plan, &targets[0], &FILE_KEY, "启动区", 1).unwrap();
    assert_eq!(&front.image.sectors().get(&0).unwrap()[3..11], b"EXFAT   ");
    let share =
        build_official_partition_filesystem(&plan, &targets[1], &FILE_KEY, "交换区", 2).unwrap();
    let raw = share.image.sectors().get(&0).unwrap();
    assert_ne!(&raw[54..62], b"FAT16   ");
    let plain = decrypt_mode2(raw, &FILE_KEY).unwrap();
    assert_eq!(&plain[54..62], b"FAT16   ");
}

#[test]
fn partition_filesystem_uses_partition_key_material_and_plaintext_skips_file_key_crc() {
    let mut plan = official_plan(OfficialPartitionMode::DefaultThreePartition);
    let keys = [[0x11; 16], [0x22; 16], [0x33; 16]];
    for (index, key) in keys.into_iter().enumerate() {
        plan = plan
            .with_partition_key_material(
                index,
                wrap_legacy_lba7_file_key(b"ProofPass1!", [index as u8 + 1; 8]),
                wrap_file_key(b"ProofPass1!", key, FileKeyWrapMode::Sm4),
            )
            .unwrap();
    }
    let targets = plan.format_targets().unwrap();

    let plaintext = build_official_partition_filesystem(&plan, &targets[0], &[0x99; 16], "BOOT", 1)
        .expect("plaintext type1 must not require a FileKeyCRC match");
    assert!(!plaintext.physically_encrypted);

    let encrypted =
        build_official_partition_filesystem(&plan, &targets[1], &[0x22; 16], "SHARE", 2)
            .expect("encrypted partition must validate against its own partition key material");
    assert!(encrypted.physically_encrypted);
    assert!(
        build_official_partition_filesystem(&plan, &targets[1], &[0x33; 16], "SHARE", 2,)
            .unwrap_err()
            .contains("FileKeyCRC")
    );

    let mut mode2 = official_plan(OfficialPartitionMode::WholeDiskEncrypted);
    mode2 = mode2
        .with_partition_key_material(
            0,
            wrap_legacy_lba7_file_key(b"ProofPass1!", [0x44; 8]),
            wrap_file_key(b"ProofPass1!", [0x44; 16], FileKeyWrapMode::Sm4),
        )
        .unwrap()
        .with_partition_key_material(
            1,
            wrap_legacy_lba7_file_key(b"ProofPass1!", [0x55; 8]),
            wrap_file_key(b"ProofPass1!", [0x55; 16], FileKeyWrapMode::Sm4),
        )
        .unwrap();
    let mode2_targets = mode2.format_targets().unwrap();
    assert_eq!(mode2_targets[0].role, PartitionRole::CompatibilityReserve);
    assert!(!mode2_targets[0].format_capable);
    assert!(mode2_targets[1].physically_encrypted);
    build_official_partition_filesystem(&mode2, &mode2_targets[1], &[0x55; 16], "ENCRYPT", 3)
        .expect(
            "mode2 encrypted partition must keep partition slot index after compatibility reserve",
        );
    assert!(build_official_partition_filesystem(
        &mode2,
        &mode2_targets[1],
        &[0x44; 16],
        "ENCRYPT",
        3,
    )
    .unwrap_err()
    .contains("FileKeyCRC"));
}

#[test]
fn exfat_builder_rejects_oversized_labels_and_tiny_volumes() {
    assert!(build_empty_exfat(63, 16, 1, "EDP").is_err());
    assert!(build_empty_exfat(63, 1_000_000, 1, "123456789012").is_err());
}
