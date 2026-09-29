use std::collections::BTreeMap;

use edpcli::filesystem::{
    DetectionConfidence, DetectionResult, DriverRegistry, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemReader,
    FormatRequest, EXFAT_DRIVER, FAT16_DRIVER,
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
    let legacy = edpcli::provision::build_empty_fat16(63, 20_417, 0x1234_5678, "BOOT").unwrap();
    let writes = plan
        .writes
        .iter()
        .map(|write| (write.relative_lba, write.data))
        .collect::<BTreeMap<_, _>>();
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
    let legacy = edpcli::provision::build_empty_exfat(2_048, 100_000, 0x8765_4321, "DATA").unwrap();
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
