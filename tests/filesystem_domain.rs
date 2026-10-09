use std::collections::BTreeMap;

use edpcli::application::filesystem::{
    DetectionConfidence, DetectionResult, DriverRegistry, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemReader,
    FormatRequest, EXFAT_DRIVER, FAT12_DRIVER, FAT16_DRIVER, FAT32_DRIVER,
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
            std::thread::current()
                .name()
                .unwrap_or("test")
                .replace(':', "_"),
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

#[test]
fn fat32_legacy_512_goldens_survive_shared_native_writer() {
    use sha2::{Digest, Sha256};
    // Fixed goldens captured from the original FAT32 formatter before the
    // native-block implementation; never recompute these from new output.
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat32,
        volume_label: Some("DATA".into()),
        volume_serial: Some(0x1020_3040),
    };
    for (offset, sectors, expected_sha) in [
        (
            63,
            70_000,
            "c0dbcc0ee274f109bd209fd5b0036b67291963281ea555df45e35456d1ea9db1",
        ),
        (
            2048,
            262_144,
            "0705ddabb9b6855442d4dd172ea5972569b9ebaf0d1150ea40cb0c5bc99d4c80",
        ),
        (
            63,
            1_048_576,
            "79f3c8de5049e59706801690ecbc41804542059c059b87af7fa2c80e64416c6d",
        ),
    ] {
        let geometry = FilesystemGeometry::new(offset, sectors, 512);
        let old = FAT32_DRIVER.build_format_plan(geometry, &request).unwrap();
        let native = FAT32_DRIVER
            .build_native_format_plan(geometry, &request)
            .unwrap();
        assert_eq!(old.expected_metadata, native.expected_metadata);
        assert_eq!(old.writes.len(), native.writes.len());
        let mut hash = Sha256::new();
        for (left, right) in old.writes.iter().zip(&native.writes) {
            assert_eq!(left.relative_lba, right.relative_lba);
            assert_eq!(&left.data[..], right.data.as_slice());
            hash.update(left.relative_lba.to_le_bytes());
            hash.update(left.data);
        }
        let digest = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            digest, expected_sha,
            "512B FAT32 baseline drift {offset}:{sectors}"
        );
    }
}

#[test]
fn fat32_native_4kn_virtual_file_has_independent_consistent_metadata() {
    use std::io::{Read, Seek, SeekFrom, Write};
    const SIZE: u64 = 4096;
    const COUNT: u64 = 70_000;
    let geometry = FilesystemGeometry::new(63, COUNT, SIZE as u32);
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat32,
        volume_label: Some("FOURKN".into()),
        volume_serial: Some(0x45a3_917f),
    };
    let plan = FAT32_DRIVER
        .build_native_format_plan(geometry, &request)
        .unwrap();
    let external = std::env::var_os("EDPCLI_NATIVE_FAT32_IMAGE_PATH");
    let path = external
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("edpcli-fat32-4kn-{}.img", std::process::id()))
        });
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let outcome = (|| -> std::io::Result<()> {
        file.set_len(COUNT * SIZE)?;
        for write in &plan.writes {
            assert!(write.relative_lba < COUNT);
            assert_eq!(write.data.len(), SIZE as usize);
            file.seek(SeekFrom::Start(write.relative_lba * SIZE))?;
            file.write_all(&write.data)?;
        }
        file.flush()?;
        let block = |file: &mut std::fs::File, lba: u64| -> std::io::Result<Vec<u8>> {
            let mut bytes = vec![0u8; SIZE as usize];
            file.seek(SeekFrom::Start(lba * SIZE))?;
            file.read_exact(&mut bytes)?;
            Ok(bytes)
        };
        // This deliberately parses on-disk bytes without using edpcli's own FAT32 parser.
        let boot = block(&mut file, 0)?;
        let get16 = |raw: &[u8], offset: usize| {
            u16::from_le_bytes(raw[offset..offset + 2].try_into().unwrap()) as u64
        };
        let get32 = |raw: &[u8], offset: usize| {
            u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap()) as u64
        };
        let bps = get16(&boot, 11);
        let spc = u64::from(boot[13]);
        let reserved = get16(&boot, 14);
        let copies = u64::from(boot[16]);
        let fat_len = get32(&boot, 36);
        let root_cluster = get32(&boot, 44);
        let fsinfo_lba = get16(&boot, 48);
        let backup_lba = get16(&boot, 50);
        assert_eq!(bps, SIZE);
        assert_eq!(spc, 1);
        assert_eq!(reserved, 32);
        assert_eq!(copies, 2);
        assert_eq!(root_cluster, 2);
        assert_eq!(fsinfo_lba, 1);
        assert_eq!(backup_lba, 6);
        assert_eq!(get32(&boot, 28), 63);
        assert_eq!(get32(&boot, 32), COUNT);
        assert_eq!(&boot[510..512], &[0x55, 0xaa]);
        assert!(boot[512..].iter().all(|b| *b == 0));
        let data_start = reserved + copies * fat_len;
        let clusters = (COUNT - data_start) / spc;
        assert!((65_525..=4_194_304).contains(&clusters));
        assert!((clusters + 2) * 4 <= fat_len * bps);
        let fsinfo = block(&mut file, fsinfo_lba)?;
        assert_eq!(get32(&fsinfo, 0), 0x4161_5252);
        assert_eq!(get32(&fsinfo, 484), 0x6141_7272);
        assert_eq!(get32(&fsinfo, 488), clusters - 1);
        assert_eq!(get32(&fsinfo, 492), 3);
        assert_eq!(get32(&fsinfo, 508), 0xaa55_0000);
        assert_eq!(block(&mut file, backup_lba)?, boot);
        assert_eq!(block(&mut file, backup_lba + 1)?, fsinfo);
        let first_fat = block(&mut file, reserved)?;
        assert_eq!(get32(&first_fat, 0), 0x0fff_fff8);
        assert_eq!(get32(&first_fat, 4), 0x0fff_ffff);
        assert_eq!(get32(&first_fat, 8), 0x0fff_ffff);
        assert_eq!(block(&mut file, reserved + fat_len)?, first_fat);
        let root = block(&mut file, data_start)?;
        assert_eq!(&root[..11], &boot[71..82]);
        assert_eq!(root[11], 0x08);
        assert_eq!(plan.writes.len() as u64, 4 + 2 * fat_len + spc);
        assert!(block(&mut file, COUNT - 1)?.iter().all(|b| *b == 0));
        Ok(())
    })();
    if external.is_none() {
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }
    outcome.unwrap();
}

#[test]
fn fat32_native_formatter_rejects_noncertified_sizes_and_bounds() {
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat32,
        volume_label: None,
        volume_serial: Some(7),
    };
    for bytes in [0, 256, 1024, 2048, 8192] {
        assert!(FAT32_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(63, 70_000, bytes), &request)
            .is_err());
    }
    for (start, count) in [(u64::MAX, 70_000), (63, u64::MAX), (63, 5)] {
        assert!(FAT32_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(start, count, 4096), &request)
            .is_err());
    }
    assert!(FAT32_DRIVER
        .build_format_plan(FilesystemGeometry::new(63, 70_000, 4096), &request)
        .is_err());
    assert!(FAT32_DRIVER
        .build_native_format_plan(FilesystemGeometry::new(63, 80_000_000, 4096), &request)
        .is_err());
}

#[test]
fn exfat_legacy_512_goldens_survive_native_formatter() {
    use sha2::{Digest, Sha256};
    // Captured from pre-P6 exFAT writer: LBA little-endian u64 followed by full 512B.
    let request = FormatRequest {
        filesystem: FilesystemKind::ExFat,
        volume_label: Some("DATA".into()),
        volume_serial: Some(0x1020_3040),
    };
    for (start, count, golden) in [
        (
            63,
            32_768,
            "21760f0aad2e6b8fb7dcffb26c95a9eb1dd7067a43ea5140da12064f8345ef73",
        ),
        (
            2048,
            262_144,
            "858556bd69486ca9a0dbec722cd73ca8fca02571e2836e929e54e78b176986ec",
        ),
        (
            63,
            1_048_576,
            "5a422ccbdb60c5c8d4df8c0a9df0f83e3be4647b6a92f03436bb2379570dcc08",
        ),
    ] {
        let geometry = FilesystemGeometry::new(start, count, 512);
        let legacy = EXFAT_DRIVER.build_format_plan(geometry, &request).unwrap();
        let native = EXFAT_DRIVER
            .build_native_format_plan(geometry, &request)
            .unwrap();
        assert_eq!(legacy.expected_metadata, native.expected_metadata);
        assert_eq!(legacy.writes.len(), native.writes.len());
        let mut hash = Sha256::new();
        for (old, new) in legacy.writes.iter().zip(&native.writes) {
            assert_eq!(old.relative_lba, new.relative_lba);
            assert_eq!(old.data.as_slice(), new.data.as_slice());
            hash.update(old.relative_lba.to_le_bytes());
            hash.update(old.data);
        }
        let digest = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            digest, golden,
            "exFAT 512B metadata changed at {start}:{count}"
        );
    }
}

#[test]
fn exfat_native_4kn_virtual_metadata_and_checksum_independent() {
    use std::io::{Read, Seek, SeekFrom, Write};
    const SIZE: u64 = 4096;
    const COUNT: u64 = 32_768;
    let geometry = FilesystemGeometry::new(63, COUNT, SIZE as u32);
    let request = FormatRequest {
        filesystem: FilesystemKind::ExFat,
        volume_label: Some("FOURKN".into()),
        volume_serial: Some(0x5643_2190),
    };
    let plan = EXFAT_DRIVER
        .build_native_format_plan(geometry, &request)
        .unwrap();
    let external = std::env::var_os("EDPCLI_NATIVE_EXFAT_IMAGE_PATH");
    let path = external
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("edpcli-exfat-4kn-{}.img", std::process::id()))
        });
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let outcome = (|| -> std::io::Result<()> {
        file.set_len(COUNT * SIZE)?;
        for write in &plan.writes {
            assert!(write.relative_lba < COUNT);
            assert_eq!(write.data.len(), SIZE as usize);
            file.seek(SeekFrom::Start(write.relative_lba * SIZE))?;
            file.write_all(&write.data)?;
        }
        file.flush()?;
        let block = |file: &mut std::fs::File, lba: u64| -> std::io::Result<Vec<u8>> {
            let mut bytes = vec![0u8; SIZE as usize];
            file.seek(SeekFrom::Start(lba * SIZE))?;
            file.read_exact(&mut bytes)?;
            Ok(bytes)
        };
        let read16 =
            |v: &[u8], i: usize| u16::from_le_bytes(v[i..i + 2].try_into().unwrap()) as u64;
        let read32 =
            |v: &[u8], i: usize| u32::from_le_bytes(v[i..i + 4].try_into().unwrap()) as u64;
        let read64 = |v: &[u8], i: usize| u64::from_le_bytes(v[i..i + 8].try_into().unwrap());
        let boot = block(&mut file, 0)?;
        assert_eq!(&boot[3..11], b"EXFAT   ");
        assert_eq!(read64(&boot, 64), 63);
        assert_eq!(read64(&boot, 72), COUNT);
        assert_eq!(boot[108], 12);
        assert_eq!(&boot[510..512], &[0x55, 0xaa]);
        assert!(boot[512..].iter().all(|b| *b == 0));
        let fat_start = read32(&boot, 80);
        let fat_len = read32(&boot, 84);
        let heap_start = read32(&boot, 88);
        let clusters = read32(&boot, 92);
        let spc = 1u64 << boot[109];
        assert_eq!(fat_start, 24);
        assert_eq!(read32(&boot, 96), 2);
        assert_eq!(read16(&boot, 104), 0x0100);
        assert_eq!(boot[110], 1);
        assert!(fat_len * SIZE >= (clusters + 2) * 4);
        assert!(heap_start >= fat_start + fat_len);
        assert!(heap_start + clusters * spc <= COUNT);
        let mut checksum = 0u32;
        for lba in 0..11 {
            let bytes = block(&mut file, lba)?;
            if (1..=8).contains(&lba) {
                assert_eq!(&bytes[SIZE as usize - 4..], &[0, 0, 0x55, 0xaa]);
            }
            assert_eq!(block(&mut file, lba + 12)?, bytes);
            for (i, byte) in bytes.iter().enumerate() {
                if lba == 0 && matches!(i, 106 | 107 | 112) {
                    continue;
                }
                checksum = checksum.rotate_right(1).wrapping_add(u32::from(*byte));
            }
        }
        let boot_sum = block(&mut file, 11)?;
        assert!(boot_sum
            .as_chunks::<4>()
            .0
            .iter()
            .all(|chunk| u32::from_le_bytes(*chunk) == checksum));
        assert_eq!(block(&mut file, 23)?, boot_sum);
        let fat = block(&mut file, fat_start)?;
        assert_eq!(read32(&fat, 0), 0xffff_fff8);
        assert_eq!(read32(&fat, 4), 0xffff_ffff);
        assert_eq!(read32(&fat, 8), 0xffff_ffff); // root cluster 2
        let root = block(&mut file, heap_start)?;
        assert_eq!(root[0], 0x81); // bitmap entry
        assert_eq!(root[32], 0x82); // upcase table entry
        assert_eq!(root[64], 0x83); // UTF-16 volume label entry
        assert_eq!(root[65], 6);
        let bitmap_cluster = read32(&root, 20);
        let bitmap_bytes = read64(&root, 24);
        let upcase_cluster = read32(&root, 52);
        let upcase_bytes = read64(&root, 56);
        assert_eq!(bitmap_cluster, 3);
        assert_eq!(bitmap_bytes, clusters.div_ceil(8));
        assert!(upcase_cluster >= 4);
        assert!(upcase_bytes > 0);
        let bitmap_lba = heap_start + (bitmap_cluster - 2) * spc;
        let bitmap = block(&mut file, bitmap_lba)?;
        assert_eq!(bitmap[0] & 0b0000_0111, 0b0000_0111); // root, bitmap, upcase
        let upcase_lba = heap_start + (upcase_cluster - 2) * spc;
        let mut upcase = Vec::new();
        for lba in 0..upcase_bytes.div_ceil(SIZE) {
            upcase.extend_from_slice(&block(&mut file, upcase_lba + lba)?);
        }
        upcase.truncate(upcase_bytes as usize);
        let upcase_sum = upcase.iter().fold(0u32, |sum, byte| {
            sum.rotate_right(1).wrapping_add(u32::from(*byte))
        });
        assert_eq!(read32(&root, 36), u64::from(upcase_sum));
        assert_eq!(
            plan.writes.len() as u64,
            24 + fat_len
                + ((bitmap_bytes.div_ceil(spc * SIZE) + upcase_bytes.div_ceil(spc * SIZE) + 1)
                    * spc)
        );
        assert!(block(&mut file, COUNT - 1)?.iter().all(|b| *b == 0));
        Ok(())
    })();
    if external.is_none() {
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }
    outcome.unwrap();
}

#[test]
fn exfat_native_4kn_geometry_and_legacy_write_guards() {
    let request = FormatRequest {
        filesystem: FilesystemKind::ExFat,
        volume_label: Some("DATA".into()),
        volume_serial: Some(7),
    };
    for bytes in [0, 256, 1024, 2048, 8192] {
        assert!(EXFAT_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(63, 32_768, bytes), &request)
            .is_err());
    }
    assert!(EXFAT_DRIVER
        .build_format_plan(FilesystemGeometry::new(63, 32_768, 4096), &request)
        .is_err());
    for (start, count) in [(63, 0), (u64::MAX, 32_768), (63, u64::MAX), (63, 8)] {
        assert!(EXFAT_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(start, count, 4096), &request)
            .is_err());
    }
}

#[test]
fn fat12_legacy_512_detection_remains_read_only_with_new_virtual_writer() {
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat12,
        volume_label: Some("BOOT".to_string()),
        volume_serial: Some(0x0102_0304),
    };
    let geometry = FilesystemGeometry::new(63, 2497, 512);
    let plan = FAT12_DRIVER
        .build_native_format_plan(geometry, &request)
        .unwrap();
    assert_eq!(plan.expected_metadata.kind, FilesystemKind::Fat12);
    assert!(!FAT12_DRIVER.capabilities().format);
    assert!(!FAT12_DRIVER.capabilities().verify_format);
    assert_eq!(
        FAT12_DRIVER
            .build_format_plan(geometry, &request)
            .unwrap_err()
            .kind,
        FilesystemErrorKind::FormatUnsupported
    );
    let sectors = plan
        .writes
        .into_iter()
        .map(|write| {
            (
                write.relative_lba,
                <[u8; 512]>::try_from(write.data).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut reader = PlanReader {
        sector_count: geometry.sector_count,
        sectors,
    };
    assert_eq!(
        FAT12_DRIVER.detect(&mut reader).unwrap().confidence,
        DetectionConfidence::Exact
    );
    assert_eq!(
        FAT12_DRIVER.read_metadata(&mut reader).unwrap().kind,
        FilesystemKind::Fat12
    );
}

#[test]
fn fat12_u391_native_virtual_4kn_image_independently_checks_fat_and_root() {
    use std::io::{Read, Seek, SeekFrom, Write};
    const SIZE: u64 = 4096;
    const COUNT: u64 = 2497; // Actual U391 FAT12 boot partition native sector count
    let geometry = FilesystemGeometry::new(63, COUNT, SIZE as u32);
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat12,
        volume_label: Some("BOOT".to_string()),
        volume_serial: Some(0x1234_5678),
    };
    let plan = FAT12_DRIVER
        .build_native_format_plan(geometry, &request)
        .unwrap();
    assert_eq!(plan.geometry, geometry);
    let external = std::env::var_os("EDPCLI_NATIVE_FAT12_IMAGE_PATH");
    let path = external
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("edpcli-fat12-4kn-{}.img", std::process::id()))
        });
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let result = (|| -> std::io::Result<()> {
        file.set_len(SIZE * COUNT)?;
        for sector in &plan.writes {
            assert!(sector.relative_lba < COUNT);
            assert_eq!(sector.data.len(), SIZE as usize);
            file.seek(SeekFrom::Start(sector.relative_lba * SIZE))?;
            file.write_all(&sector.data)?;
        }
        file.flush()?;

        // Independent FAT12 parser: reads the regular-file bytes directly,
        // without invoking any edpcli boot-sector detector/reader.
        let block = |file: &mut std::fs::File, lba: u64| -> std::io::Result<Vec<u8>> {
            let mut bytes = vec![0u8; SIZE as usize];
            file.seek(SeekFrom::Start(lba * SIZE))?;
            file.read_exact(&mut bytes)?;
            Ok(bytes)
        };
        let boot = block(&mut file, 0)?;
        let le16 = |off: usize| u16::from_le_bytes(boot[off..off + 2].try_into().unwrap()) as u64;
        let le32 = |off: usize| u32::from_le_bytes(boot[off..off + 4].try_into().unwrap()) as u64;
        assert_eq!(&boot[3..11], b"MSDOS5.0");
        assert_eq!(le16(11), SIZE);
        assert_eq!(boot[13], 1); // U391 boot partition SPC=1
        assert_eq!(le16(14), 1);
        assert_eq!(boot[16], 2);
        assert_eq!(le16(17), 512);
        assert_eq!(le16(19), COUNT);
        assert_eq!(le32(32), 0);
        assert_eq!(le16(22), 1); // U391 sample has one 4Kn block per FAT copy
        assert_eq!(le32(28), 63);
        assert_eq!(&boot[54..62], b"FAT12   ");
        assert_eq!(&boot[510..512], &[0x55, 0xaa]);
        assert!(boot[512..].iter().all(|byte| *byte == 0));
        let root_sectors = (le16(17) * 32).div_ceil(SIZE);
        let fat_len = le16(22);
        let fat_start = le16(14);
        let root_start = fat_start + fat_len * u64::from(boot[16]);
        let data_start = root_start + root_sectors;
        let cluster_count = (COUNT - data_start) / u64::from(boot[13]);
        assert_eq!(root_sectors, 4);
        assert_eq!(root_start, 3);
        assert_eq!(cluster_count, 2490);
        assert!(cluster_count < 4085);
        assert!((cluster_count + 2) * 3 <= fat_len * SIZE * 2);
        assert_eq!(plan.writes.len() as u64, data_start);
        let fat1 = block(&mut file, fat_start)?;
        let fat2 = block(&mut file, fat_start + fat_len)?;
        assert_eq!(fat1, fat2);
        // Decode packed 12-bit FAT[0] and FAT[1] independently.
        let entry0 = u16::from(fat1[0]) | (u16::from(fat1[1] & 0x0f) << 8);
        let entry1 = (u16::from(fat1[1]) >> 4) | (u16::from(fat1[2]) << 4);
        assert_eq!(entry0, 0x0ff8);
        assert_eq!(entry1, 0x0fff);
        assert!(fat1[3..].iter().all(|byte| *byte == 0));
        let root = block(&mut file, root_start)?;
        assert_eq!(&root[0..11], b"BOOT       ");
        assert_eq!(root[11], 0x08);
        assert!(root[32..].iter().all(|byte| *byte == 0));
        for index in 1..root_sectors {
            assert!(block(&mut file, root_start + index)?
                .iter()
                .all(|byte| *byte == 0));
        }
        assert!(block(&mut file, COUNT - 1)?.iter().all(|byte| *byte == 0));
        Ok(())
    })();
    if external.is_none() {
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }
    result.unwrap();
}

#[test]
fn fat12_native_virtual_rejects_uncertified_sizes_and_overflow() {
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat12,
        volume_label: None,
        volume_serial: Some(7),
    };
    for bytes in [0, 256, 1024, 2048, 8192] {
        assert!(FAT12_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(63, 2497, bytes), &request)
            .is_err());
    }
    for (start, count) in [
        (63, 0),
        (u64::MAX, 2497),
        (63, u64::MAX),
        (u32::MAX as u64 + 1, 2497),
        (63, 1),
    ] {
        assert!(FAT12_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(start, count, 4096), &request)
            .is_err());
    }
    assert!(FAT12_DRIVER
        .build_native_format_plan(
            FilesystemGeometry::new(63, 2497, 4096),
            &FormatRequest {
                filesystem: FilesystemKind::Fat16,
                ..request.clone()
            },
        )
        .is_err());
    assert!(FAT12_DRIVER
        .build_native_format_plan(
            FilesystemGeometry::new(63, 2497, 4096),
            &FormatRequest {
                volume_serial: None,
                ..request
            },
        )
        .is_err());
}

#[test]
fn fat12_native_geometry_changes_only_for_source_device() {
    let request = FormatRequest {
        filesystem: FilesystemKind::Fat12,
        volume_label: None,
        volume_serial: Some(0x1234_5678),
    };
    for count in [2497, 20_417, 32_768] {
        let legacy = FAT12_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(63, count, 512), &request)
            .unwrap();
        let native = FAT12_DRIVER
            .build_native_format_plan(FilesystemGeometry::new(63, count, 4096), &request)
            .unwrap();
        let boot_512 = &legacy.writes[0].data;
        let boot_4kn = &native.writes[0].data;
        let le16 =
            |raw: &[u8], off| u16::from_le_bytes(raw[off..off + 2].try_into().unwrap()) as u64;
        assert_eq!(le16(boot_512, 11), 512);
        assert_eq!(le16(boot_4kn, 11), 4096);
        assert_eq!(legacy.geometry.sector_count, native.geometry.sector_count);
        for plan in [&legacy, &native] {
            let bps = u64::from(plan.geometry.sector_size);
            let boot = &plan.writes[0].data;
            let spc = u64::from(boot[13]);
            let root = le16(boot, 17) * 32 / bps;
            let fat_sectors = le16(boot, 22);
            let overhead = le16(boot, 14) + u64::from(boot[16]) * fat_sectors + root;
            let clusters = (count - overhead) / spc;
            assert!((1..4085).contains(&clusters));
            assert!((clusters + 2) * 3 <= fat_sectors * bps * 2);
            assert_eq!(plan.writes.len() as u64, overhead);
            assert_eq!(plan.writes[0].relative_lba, 0);
            assert_eq!(plan.writes.last().unwrap().relative_lba, overhead - 1);
            assert!(plan
                .writes
                .iter()
                .all(|write| write.data.len() == bps as usize));
        }
        assert_eq!(le16(&legacy.writes[0].data, 11), 512); // no global mutation
    }

    // 65535 x 4Kn cannot fit FAT12's <=4084 clusters when clusters
    // are limited to 64KiB; it must be rejected, not silently become FAT16.
    assert!(FAT12_DRIVER
        .build_native_format_plan(FilesystemGeometry::new(63, 65_535, 4096), &request,)
        .is_err());
    let invalid_label = FAT12_DRIVER
        .build_native_format_plan(
            FilesystemGeometry::new(63, 2497, 4096),
            &FormatRequest {
                volume_label: Some("BAD/NAME".into()),
                ..request
            },
        )
        .unwrap_err();
    assert_eq!(invalid_label.filesystem, Some(FilesystemKind::Fat12));
    assert_eq!(invalid_label.kind, FilesystemErrorKind::InvalidVolumeLabel);
}
