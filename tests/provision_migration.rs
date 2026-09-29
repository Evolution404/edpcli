use edpcli::{
    backup_metadata::PartitionGeometry,
    filesystem::analysis::{
        analyze_partition, stream_file_payload, AnalysisStatus, FileEntry, FilePayloadExtent,
        FilePayloadLocator, PartitionReader,
    },
    filesystem::{build_migrated_filesystem, FilesystemKind, FilesystemMigrationEntry},
    protocol::edpf::EdpPartitionType,
    provision::{
        build_migration_manifest, Extent, FilesystemProfile, MigrationBudgets, MigrationInventory,
        MigrationPreflightError, MigrationSource, MigrationStagedEntry, MigrationTransform,
        PartitionRole, PhysicalCryptoProfile, SourceRegion, TargetPartitionGeometry,
    },
};

fn source_region(
    role: PartitionRole,
    partition_type: EdpPartitionType,
    start_lba: u64,
    sector_count: u64,
) -> SourceRegion {
    SourceRegion {
        role,
        partition_type: partition_type.raw(),
        extent: Extent {
            start_lba,
            sector_count,
        },
        physical_crypto: PhysicalCryptoProfile::Plain,
        filesystem: FilesystemProfile::Known(FilesystemKind::ExFat),
        key_profile: None,
    }
}

fn file(path: &str, logical_size: u64, start_lba: u64, sector_count: u64) -> FileEntry {
    FileEntry {
        path: path.into(),
        is_directory: false,
        logical_size,
        allocated_size: Some(sector_count * 512),
        mtime: None,
        ctime: None,
        attributes: 0x20,
        payload_locator: Some(FilePayloadLocator {
            logical_size,
            extents: vec![FilePayloadExtent {
                start_lba,
                sector_count,
            }],
        }),
    }
}

fn directory(path: &str) -> FileEntry {
    FileEntry {
        path: path.into(),
        is_directory: true,
        logical_size: 0,
        allocated_size: None,
        mtime: None,
        ctime: None,
        attributes: 0x10,
        payload_locator: None,
    }
}

fn combined_target(sector_count: u64) -> TargetPartitionGeometry {
    TargetPartitionGeometry {
        role: PartitionRole::BootShareCombined,
        partition_type: EdpPartitionType::Share,
        start_lba: 63,
        sector_count,
        physically_encrypted: false,
        filesystem: Some(FilesystemKind::ExFat),
    }
}

#[test]
fn migration_manifest_requires_complete_staging_and_typed_transforms() {
    let boot = source_region(PartitionRole::Boot, EdpPartitionType::Boot, 63, 20_417);
    let share = source_region(
        PartitionRole::Share,
        EdpPartitionType::Share,
        20_480,
        20_000,
    );
    let sources = vec![
        MigrationSource {
            source_index: 0,
            region: boot,
            transform: MigrationTransform::BootToBootShareCombined,
        },
        MigrationSource {
            source_index: 1,
            region: share,
            transform: MigrationTransform::ShareToBootShareCombined,
        },
    ];
    let inventories = vec![
        MigrationInventory {
            source_index: 0,
            region: boot,
            entries: vec![directory("/boot/"), file("/boot/a.bin", 512, 8, 1)],
        },
        MigrationInventory {
            source_index: 1,
            region: share,
            entries: vec![file("/share.txt", 600, 10, 2)],
        },
    ];
    let target = combined_target(80_000);
    let manifest = build_migration_manifest(
        &sources,
        &target,
        &inventories,
        MigrationBudgets {
            staging_available_bytes: 2_000,
            target_available_bytes: 80_000 * 512,
        },
    )
    .unwrap();

    assert_eq!(manifest.total_logical_bytes, 1_112);
    assert_eq!(manifest.staging_required_bytes, 1_112);
    assert_eq!(manifest.file_count, 2);
    assert_eq!(manifest.directory_count, 1);
    assert_eq!(manifest.entries.len(), 3);
    assert_eq!(
        manifest.entries[0].transform,
        MigrationTransform::BootToBootShareCombined
    );
}

#[test]
fn migration_manifest_fails_closed_on_budget_path_and_locator_errors() {
    let source = source_region(
        PartitionRole::Share,
        EdpPartitionType::Share,
        20_480,
        20_000,
    );
    let sources = vec![MigrationSource {
        source_index: 0,
        region: source,
        transform: MigrationTransform::ShareToBootShareCombined,
    }];
    let target = combined_target(40_000);
    let inventory = MigrationInventory {
        source_index: 0,
        region: source,
        entries: vec![file("/data.bin", 700, 10, 2)],
    };

    assert!(matches!(
        build_migration_manifest(
            &sources,
            &target,
            std::slice::from_ref(&inventory),
            MigrationBudgets {
                staging_available_bytes: 699,
                target_available_bytes: 40_000 * 512,
            },
        ),
        Err(MigrationPreflightError::StagingBudgetExceeded { .. })
    ));

    let mut missing = inventory.clone();
    missing.entries[0].payload_locator = None;
    assert!(matches!(
        build_migration_manifest(
            &sources,
            &target,
            &[missing],
            MigrationBudgets {
                staging_available_bytes: 10_000,
                target_available_bytes: 40_000 * 512,
            },
        ),
        Err(MigrationPreflightError::MissingPayloadLocator { .. })
    ));

    let mut outside = inventory.clone();
    outside.entries[0].payload_locator = Some(FilePayloadLocator {
        logical_size: 700,
        extents: vec![FilePayloadExtent {
            start_lba: source.extent.sector_count,
            sector_count: 2,
        }],
    });
    assert!(matches!(
        build_migration_manifest(
            &sources,
            &target,
            &[outside],
            MigrationBudgets {
                staging_available_bytes: 10_000,
                target_available_bytes: 40_000 * 512,
            },
        ),
        Err(MigrationPreflightError::PayloadOutsideSource { .. })
    ));
}

struct ImageReader<'a> {
    image: &'a edpcli::filesystem::SparseFilesystemImage,
}

impl PartitionReader for ImageReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        self.image
            .sector_or_zero(relative_lba)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| std::io::Error::other("outside image"))
    }
}

fn roundtrip_migrated_filesystem(filesystem: FilesystemKind, volume_sectors: u64) {
    let payload = (0..1_537)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let staged = vec![
        MigrationStagedEntry {
            source_index: 0,
            transform: MigrationTransform::ShareToBootShareCombined,
            path: "/文档/".into(),
            is_directory: true,
            data: Vec::new(),
            attributes: 0x10,
            mtime: None,
            ctime: None,
        },
        MigrationStagedEntry {
            source_index: 0,
            transform: MigrationTransform::ShareToBootShareCombined,
            path: "/文档/hello-long-name.txt".into(),
            is_directory: false,
            data: payload.clone(),
            attributes: 0x20,
            mtime: None,
            ctime: None,
        },
    ];
    let filesystem_entries = staged
        .iter()
        .map(FilesystemMigrationEntry::from)
        .collect::<Vec<_>>();
    let image = build_migrated_filesystem(
        filesystem,
        2_048,
        volume_sectors,
        0x1234_5678,
        "K6",
        &filesystem_entries,
    )
    .unwrap();

    let geometry = PartitionGeometry {
        index: 0,
        partition_type: 0x07,
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: 2_048,
        sector_size: 512,
        partition_size: volume_sectors * 512,
        sector_count: volume_sectors,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    let mut reader = ImageReader { image: &image };
    let report = analyze_partition(&geometry, &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.file_count, Some(1));
    assert_eq!(report.directory_count, Some(1));
    let entry = report
        .entries
        .as_ref()
        .unwrap()
        .iter()
        .find(|entry| entry.path == "/文档/hello-long-name.txt")
        .unwrap()
        .clone();
    let mut reader = ImageReader { image: &image };
    let mut actual = Vec::new();
    stream_file_payload(&mut reader, &entry, payload.len() as u64, &mut actual).unwrap();
    assert_eq!(actual, payload);
}

#[test]
fn populated_fat16_migration_image_roundtrips_inventory_and_payload() {
    roundtrip_migrated_filesystem(FilesystemKind::Fat16, 20_417);
}

#[test]
fn populated_exfat_migration_image_roundtrips_inventory_and_payload() {
    roundtrip_migrated_filesystem(FilesystemKind::ExFat, 262_144);
}
