use std::collections::BTreeMap;

use edpcli::application::filesystem::{
    DetectionConfidence, DetectionResult, DriverRegistry, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemReader,
    FormatRequest, EXFAT_DRIVER, FAT16_DRIVER, FAT32_DRIVER,
};

struct MemoryReader {
    sectors: u64,
}

impl FilesystemReader for MemoryReader {
    fn sector_size(&self) -> u32 {
        512
    }

    fn sector_count(&self) -> u64 {
        self.sectors
    }

    fn read_sector(&mut self, _relative_lba: u64) -> Result<[u8; 512], FilesystemError> {
        Ok([0u8; 512])
    }
}

struct FakeDriver {
    kind: FilesystemKind,
    confidence: DetectionConfidence,
}

impl FilesystemDriver for FakeDriver {
    fn kind(&self) -> FilesystemKind {
        self.kind
    }

    fn capabilities(&self) -> FilesystemCapabilities {
        FilesystemCapabilities::detect_only()
    }

    fn detect(
        &self,
        _source: &mut dyn FilesystemReader,
    ) -> Result<DetectionResult, FilesystemError> {
        Ok(DetectionResult {
            kind: self.kind,
            confidence: self.confidence,
        })
    }
}

#[test]
fn canonical_filesystem_kind_tokens_are_stable() {
    let cases = [
        (FilesystemKind::Fat12, "fat12", "FAT12"),
        (FilesystemKind::Fat16, "fat16", "FAT16"),
        (FilesystemKind::Fat32, "fat32", "FAT32"),
        (FilesystemKind::ExFat, "exfat", "exFAT"),
        (FilesystemKind::Ntfs, "ntfs", "NTFS"),
    ];
    for (kind, token, display) in cases {
        assert_eq!(kind.config_token(), token);
        assert_eq!(kind.display_name(), display);
    }
}

#[test]
fn registry_selects_strongest_detection_and_fails_closed_on_ties() {
    let fat16 = FakeDriver {
        kind: FilesystemKind::Fat16,
        confidence: DetectionConfidence::Strong,
    };
    let exfat = FakeDriver {
        kind: FilesystemKind::ExFat,
        confidence: DetectionConfidence::Exact,
    };
    let mut reader = MemoryReader { sectors: 10_000 };
    let drivers: [&dyn FilesystemDriver; 2] = [&fat16, &exfat];
    let registry = DriverRegistry::new(&drivers);
    let detected = registry.detect(&mut reader).unwrap().unwrap();
    assert_eq!(detected.kind(), FilesystemKind::ExFat);

    let ntfs = FakeDriver {
        kind: FilesystemKind::Ntfs,
        confidence: DetectionConfidence::Exact,
    };
    let ambiguous_drivers: [&dyn FilesystemDriver; 2] = [&exfat, &ntfs];
    let registry = DriverRegistry::new(&ambiguous_drivers);
    let error = match registry.detect(&mut reader) {
        Ok(_) => panic!("同级不同文件系统识别必须 fail-closed"),
        Err(error) => error,
    };
    assert_eq!(error.kind, FilesystemErrorKind::AmbiguousDetection);
}

#[test]
fn detect_only_driver_rejects_format_through_typed_error() {
    let driver = FakeDriver {
        kind: FilesystemKind::Ntfs,
        confidence: DetectionConfidence::NoMatch,
    };
    let request = FormatRequest::new(FilesystemKind::Ntfs);
    let error = driver.validate_format_request(&request).unwrap_err();
    assert_eq!(error.kind, FilesystemErrorKind::FormatUnsupported);
    assert_eq!(error.filesystem, Some(FilesystemKind::Ntfs));
    assert_eq!(request.volume_label, None);
}

struct PlanReader {
    sector_count: u64,
    sectors: BTreeMap<u64, [u8; 512]>,
}

impl FilesystemReader for PlanReader {
    fn sector_size(&self) -> u32 {
        512
    }

    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sector(&mut self, relative_lba: u64) -> Result<[u8; 512], FilesystemError> {
        if relative_lba >= self.sector_count {
            return Err(FilesystemError::new(
                FilesystemErrorKind::ReadFailure,
                "test read outside partition",
            ));
        }
        Ok(self
            .sectors
            .get(&relative_lba)
            .copied()
            .unwrap_or([0u8; 512]))
    }
}

#[test]
fn fat16_driver_owns_format_metadata_detection_and_verification() {
    let geometry = FilesystemGeometry::new(63, 20_417, 512);
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat16,
        volume_label: Some("BOOT".into()),
        volume_serial: Some(0x1234_5678),
    };
    let plan = FAT16_DRIVER
        .build_format_plan(geometry, &request)
        .expect("driver format plan");
    let legacy =
        edpcli::application::filesystem::build_empty_fat16(63, 20_417, 0x1234_5678, "BOOT")
            .unwrap();
    let writes = plan
        .writes
        .iter()
        .map(|write| (write.relative_lba, write.data))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        &writes.get(&0).expect("FAT16 boot sector")[3..11],
        b"MSDOS5.0",
        "FAT16 formatter must use the observed first-party OEM field, never an edpcli product marker"
    );
    assert_eq!(
        &writes,
        legacy.sectors(),
        "迁移适配入口必须与 driver 字节一致"
    );

    let mut reader = PlanReader {
        sector_count: geometry.sector_count,
        sectors: writes.clone(),
    };
    let detected = FAT16_DRIVER.detect(&mut reader).unwrap();
    assert_eq!(detected.confidence, DetectionConfidence::Exact);
    let metadata = FAT16_DRIVER.read_metadata(&mut reader).unwrap();
    assert_eq!(metadata.kind, FilesystemKind::Fat16);
    assert_eq!(metadata.volume_label.as_deref(), Some("BOOT"));
    assert_eq!(metadata.volume_serial, Some(0x1234_5678));
    let verified = FAT16_DRIVER
        .verify_format(&mut reader, geometry, &plan.expected_metadata)
        .unwrap();
    assert_eq!(verified.metadata, metadata);

    let mut corrupted = PlanReader {
        sector_count: geometry.sector_count,
        sectors: writes,
    };
    corrupted.sectors.get_mut(&0).unwrap()[28..32].copy_from_slice(&64u32.to_le_bytes());
    let error = FAT16_DRIVER
        .verify_format(&mut corrupted, geometry, &plan.expected_metadata)
        .unwrap_err();
    assert_eq!(error.kind, FilesystemErrorKind::InvalidGeometry);
}

#[test]
fn fat16_driver_uses_none_as_the_only_no_user_label_semantic() {
    let geometry = FilesystemGeometry::new(63, 20_417, 512);
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat16,
        volume_label: None,
        volume_serial: Some(7),
    };
    let plan = FAT16_DRIVER.build_format_plan(geometry, &request).unwrap();
    assert_eq!(plan.expected_metadata.volume_label, None);
    let mut reader = PlanReader {
        sector_count: geometry.sector_count,
        sectors: plan
            .writes
            .iter()
            .map(|write| (write.relative_lba, write.data))
            .collect(),
    };
    let metadata = FAT16_DRIVER.read_metadata(&mut reader).unwrap();
    assert_eq!(metadata.volume_label, None);
    FAT16_DRIVER
        .verify_format(&mut reader, geometry, &plan.expected_metadata)
        .unwrap();

    let invalid = FormatRequest {
        filesystem: FilesystemKind::Fat16,
        volume_label: Some(String::new()),
        volume_serial: Some(7),
    };
    assert_eq!(
        FAT16_DRIVER
            .validate_format_request(&invalid)
            .unwrap_err()
            .kind,
        FilesystemErrorKind::InvalidVolumeLabel
    );
}

#[test]
fn fat32_driver_owns_format_metadata_geometry_and_verification() {
    let geometry = FilesystemGeometry::new(2_048, 1_000_000, 512);
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat32,
        volume_label: Some("DATA".into()),
        volume_serial: Some(0x89ab_cdef),
    };
    let capabilities = FAT32_DRIVER.capabilities();
    assert!(capabilities.detect);
    assert!(capabilities.read_metadata);
    assert!(capabilities.format);
    assert!(capabilities.verify_format);
    assert!(capabilities.analyze);

    let plan = FAT32_DRIVER
        .build_format_plan(geometry, &request)
        .expect("FAT32 driver format plan");
    let writes = plan
        .writes
        .iter()
        .map(|write| (write.relative_lba, write.data))
        .collect::<BTreeMap<_, _>>();
    let legacy = edpcli::application::filesystem::build_empty_fat32(
        geometry.partition_offset,
        geometry.sector_count,
        0x89ab_cdef,
        "DATA",
    )
    .unwrap();
    assert_eq!(&writes, legacy.sectors());

    let mut reader = PlanReader {
        sector_count: geometry.sector_count,
        sectors: writes.clone(),
    };
    assert_eq!(
        FAT32_DRIVER.detect(&mut reader).unwrap().confidence,
        DetectionConfidence::Exact
    );
    assert!(FAT32_DRIVER
        .matches_geometry(&mut reader, geometry)
        .unwrap());
    let metadata = FAT32_DRIVER.read_metadata(&mut reader).unwrap();
    assert_eq!(metadata.kind, FilesystemKind::Fat32);
    assert_eq!(metadata.volume_label.as_deref(), Some("DATA"));
    assert_eq!(metadata.volume_serial, Some(0x89ab_cdef));
    let verified = FAT32_DRIVER
        .verify_format(&mut reader, geometry, &plan.expected_metadata)
        .unwrap();
    assert_eq!(verified.metadata, metadata);

    let boot = writes.get(&0).unwrap();
    assert_eq!(&boot[3..11], b"MSDOS5.0");
    assert_eq!(u16::from_le_bytes(boot[48..50].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(boot[50..52].try_into().unwrap()), 6);
    assert_eq!(&boot[82..90], b"FAT32   ");

    let mut stale_geometry = PlanReader {
        sector_count: geometry.sector_count,
        sectors: writes.clone(),
    };
    stale_geometry.sectors.get_mut(&0).unwrap()[28..32].copy_from_slice(&2_049u32.to_le_bytes());
    assert_eq!(
        FAT32_DRIVER
            .verify_format(&mut stale_geometry, geometry, &plan.expected_metadata)
            .unwrap_err()
            .kind,
        FilesystemErrorKind::InvalidGeometry
    );

    let mut corrupt_fsinfo = PlanReader {
        sector_count: geometry.sector_count,
        sectors: writes,
    };
    corrupt_fsinfo.sectors.get_mut(&1).unwrap()[0] ^= 1;
    assert_eq!(
        FAT32_DRIVER
            .verify_format(&mut corrupt_fsinfo, geometry, &plan.expected_metadata)
            .unwrap_err()
            .kind,
        FilesystemErrorKind::InvalidMetadata
    );
}

#[test]
fn exfat_driver_owns_format_metadata_detection_and_verification() {
    let geometry = FilesystemGeometry::new(2_048, 100_000, 512);
    let request = FormatRequest {
        filesystem: FilesystemKind::ExFat,
        volume_label: Some("DATA".into()),
        volume_serial: Some(0x8765_4321),
    };
    let plan = EXFAT_DRIVER
        .build_format_plan(geometry, &request)
        .expect("driver format plan");
    let legacy =
        edpcli::application::filesystem::build_empty_exfat(2_048, 100_000, 0x8765_4321, "DATA")
            .unwrap();
    let writes = plan
        .writes
        .iter()
        .map(|write| (write.relative_lba, write.data))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        &writes,
        legacy.sectors(),
        "迁移适配入口必须与 exFAT driver 字节一致"
    );

    let mut reader = PlanReader {
        sector_count: geometry.sector_count,
        sectors: writes.clone(),
    };
    let detected = EXFAT_DRIVER.detect(&mut reader).unwrap();
    assert_eq!(detected.confidence, DetectionConfidence::Exact);
    let metadata = EXFAT_DRIVER.read_metadata(&mut reader).unwrap();
    assert_eq!(metadata.kind, FilesystemKind::ExFat);
    assert_eq!(metadata.volume_label.as_deref(), Some("DATA"));
    assert_eq!(metadata.volume_serial, Some(0x8765_4321));
    let verified = EXFAT_DRIVER
        .verify_format(&mut reader, geometry, &plan.expected_metadata)
        .unwrap();
    assert_eq!(verified.metadata, metadata);

    let mut corrupted = PlanReader {
        sector_count: geometry.sector_count,
        sectors: writes,
    };
    corrupted.sectors.get_mut(&0).unwrap()[64..72].copy_from_slice(&2_049u64.to_le_bytes());
    let error = EXFAT_DRIVER
        .verify_format(&mut corrupted, geometry, &plan.expected_metadata)
        .unwrap_err();
    assert_eq!(error.kind, FilesystemErrorKind::InvalidGeometry);
}

#[test]
fn exfat_driver_uses_none_as_the_only_no_user_label_semantic() {
    let geometry = FilesystemGeometry::new(2_048, 100_000, 512);
    let request = FormatRequest {
        filesystem: FilesystemKind::ExFat,
        volume_label: None,
        volume_serial: Some(9),
    };
    let plan = EXFAT_DRIVER.build_format_plan(geometry, &request).unwrap();
    assert_eq!(plan.expected_metadata.volume_label, None);
    let mut reader = PlanReader {
        sector_count: geometry.sector_count,
        sectors: plan
            .writes
            .iter()
            .map(|write| (write.relative_lba, write.data))
            .collect(),
    };
    let metadata = EXFAT_DRIVER.read_metadata(&mut reader).unwrap();
    assert_eq!(metadata.volume_label, None);
    EXFAT_DRIVER
        .verify_format(&mut reader, geometry, &plan.expected_metadata)
        .unwrap();

    let invalid = FormatRequest {
        filesystem: FilesystemKind::ExFat,
        volume_label: Some(String::new()),
        volume_serial: Some(9),
    };
    assert_eq!(
        EXFAT_DRIVER
            .validate_format_request(&invalid)
            .unwrap_err()
            .kind,
        FilesystemErrorKind::InvalidVolumeLabel
    );
}

#[test]
fn fat16_512_native_plan_matches_legacy_write_bytes_exactly() {
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat16,
        volume_label: Some("DATA".into()),
        volume_serial: Some(0x1020_3040),
    };
    use sha2::{Digest, Sha256};
    // Independent, fixed 512B metadata golden digests derived from the
    // pre-P6 512B formatter, in LBA order with little-endian u64 LBA prefix.
    for (start, count, golden) in [
        (
            63,
            20_417,
            "ff92933ccf0142cf70392d6c9bf7bd99643ac2f3a06183d3fbc732da769885e5",
        ),
        (
            2_048,
            32_768,
            "450097e645b797fb91c3a119ea841db50eb1bd1c8ba31112f19379e31ca3b8b2",
        ),
        (
            63,
            65_535,
            "b4724c2d52a1b06975f6ca410f5a812df28745ecbe06bcea91d96c3a9059a50c",
        ),
    ] {
        let geometry = FilesystemGeometry::new(start, count, 512);
        let legacy = FAT16_DRIVER.build_format_plan(geometry, &request).unwrap();
        let native = FAT16_DRIVER
            .build_native_format_plan(geometry, &request)
            .unwrap();
        assert_eq!(legacy.expected_metadata, native.expected_metadata);
        assert_eq!(legacy.writes.len(), native.writes.len());
        let mut digest = Sha256::new();
        for (left, right) in legacy.writes.iter().zip(&native.writes) {
            assert_eq!(left.relative_lba, right.relative_lba);
            assert_eq!(&left.data[..], right.data.as_slice());
            digest.update(left.relative_lba.to_le_bytes());
            digest.update(left.data);
        }
        assert_eq!(
            digest
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            golden
        );
    }
}

#[test]
fn fat16_native_4kn_formats_virtual_file_and_independent_layout_parser() {
    use std::io::{Read, Seek, SeekFrom, Write};

    // The test writes only to an explicit regular file, never a block device.
    let bytes_per_sector = 4096u64;
    let native_lbas = 16_384u64;
    let geometry = FilesystemGeometry::new(63, native_lbas, bytes_per_sector as u32);
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat16,
        volume_label: Some("VIRTUAL".to_string()),
        volume_serial: Some(0x1234_5678),
    };
    let plan = FAT16_DRIVER
        .build_native_format_plan(geometry, &request)
        .unwrap();
    assert_eq!(plan.filesystem, FilesystemKind::Fat16);
    let saved_path = std::env::var_os("EDPCLI_NATIVE_FAT16_IMAGE_PATH");
    let path = if let Some(path) = saved_path.as_ref() {
        std::path::PathBuf::from(path)
    } else {
        std::env::temp_dir().join(format!(
            "edpcli-fat16-4kn-{}-{}.img",
            std::process::id(),
            std::thread::current().name().unwrap_or("test"),
        ))
    };
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let result = (|| -> std::io::Result<()> {
        file.set_len(native_lbas * bytes_per_sector)?;
        for sector in &plan.writes {
            assert!(sector.relative_lba < native_lbas);
            assert_eq!(sector.data.len(), bytes_per_sector as usize);
            file.seek(SeekFrom::Start(sector.relative_lba * bytes_per_sector))?;
            file.write_all(&sector.data)?;
        }
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        let mut boot = vec![0u8; bytes_per_sector as usize];
        file.read_exact(&mut boot)?;
        // Independent FAT BPB decoding; this test does NOT use edpcli's parser.
        let le16 = |at| u16::from_le_bytes(boot[at..at + 2].try_into().unwrap()) as u64;
        let le32 = |at| u32::from_le_bytes(boot[at..at + 4].try_into().unwrap()) as u64;
        assert_eq!(le16(11), bytes_per_sector);
        assert_eq!(boot[13], 1);
        assert_eq!(le16(14), 1);
        assert_eq!(boot[16], 2);
        assert_eq!(le16(17), 512);
        let count = if le16(19) != 0 { le16(19) } else { le32(32) };
        assert_eq!(count, native_lbas);
        assert_eq!(le32(28), 63);
        assert_eq!(&boot[510..512], [0x55, 0xaa]);
        assert!(boot[512..].iter().all(|byte| *byte == 0));
        let fat_sectors = le16(22);
        let root_sectors = (le16(17) * 32).div_ceil(bytes_per_sector);
        let root_start = le16(14) + u64::from(boot[16]) * fat_sectors;
        let clusters = (count - root_start - root_sectors) / u64::from(boot[13]);
        assert!((4_085..65_525).contains(&clusters));
        assert!((clusters + 2) * 2 <= fat_sectors * bytes_per_sector);
        assert_eq!(plan.writes.len() as u64, root_start + root_sectors);
        let mut first_fat = vec![0u8; bytes_per_sector as usize];
        let mut second_fat = first_fat.clone();
        file.seek(SeekFrom::Start(bytes_per_sector))?;
        file.read_exact(&mut first_fat)?;
        file.seek(SeekFrom::Start((1 + fat_sectors) * bytes_per_sector))?;
        file.read_exact(&mut second_fat)?;
        assert_eq!(first_fat, second_fat);
        assert_eq!(&first_fat[..4], [0xf8, 0xff, 0xff, 0xff]);
        let mut root = vec![0u8; bytes_per_sector as usize];
        file.seek(SeekFrom::Start(root_start * bytes_per_sector))?;
        file.read_exact(&mut root)?;
        assert_eq!(&root[..11], &boot[43..54]);
        assert_eq!(root[11], 0x08);
        file.seek(SeekFrom::Start((count - 1) * bytes_per_sector))?;
        let mut final_block = vec![0xffu8; bytes_per_sector as usize];
        file.read_exact(&mut final_block)?;
        assert!(final_block.iter().all(|byte| *byte == 0));
        Ok(())
    })();
    if saved_path.is_none() {
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }
    result.unwrap();
}

#[test]
fn fat16_native_writer_refuses_unvalidated_geometry_or_overflow() {
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat16,
        volume_label: None,
        volume_serial: Some(7),
    };
    for sector_size in [0, 1024, 2048, 8192] {
        let err = FAT16_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(63, 16384, sector_size), &request)
            .unwrap_err();
        assert_eq!(err.kind, FilesystemErrorKind::InvalidGeometry);
    }
    for (start, count) in [(u64::MAX, 16_384), (63, u64::MAX), (63, 10)] {
        assert!(FAT16_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(start, count, 4096), &request,)
            .is_err());
    }
    // Do not accidentally allow a 4096B plan into the old 512B write contract.
    assert!(FAT16_DRIVER
        .build_format_plan(FilesystemGeometry::new(63, 16384, 4096), &request,)
        .is_err());
}
