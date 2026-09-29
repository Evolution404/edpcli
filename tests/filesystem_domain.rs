use edpcli::filesystem::{
    DetectionConfidence, DetectionResult, DriverRegistry, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemKind, FilesystemReader, FormatRequest,
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
fn migration_adapters_preserve_legacy_filesystem_kinds() {
    use edpcli::inspect_target::FilesystemBootKind;
    use edpcli::provision::OfficialFilesystemFormat;

    for (legacy, canonical) in [
        (FilesystemBootKind::Fat12, FilesystemKind::Fat12),
        (FilesystemBootKind::Fat16, FilesystemKind::Fat16),
        (FilesystemBootKind::Fat32, FilesystemKind::Fat32),
        (FilesystemBootKind::Exfat, FilesystemKind::ExFat),
        (FilesystemBootKind::Ntfs, FilesystemKind::Ntfs),
    ] {
        assert_eq!(FilesystemKind::from(legacy), canonical);
        assert_eq!(FilesystemBootKind::from(canonical), legacy);
    }

    for (legacy, canonical) in [
        (OfficialFilesystemFormat::Fat16, FilesystemKind::Fat16),
        (OfficialFilesystemFormat::ExFat, FilesystemKind::ExFat),
        (OfficialFilesystemFormat::Fat32, FilesystemKind::Fat32),
        (OfficialFilesystemFormat::Ntfs, FilesystemKind::Ntfs),
    ] {
        assert_eq!(FilesystemKind::from(legacy), canonical);
        assert_eq!(
            OfficialFilesystemFormat::try_from(canonical).unwrap(),
            legacy
        );
    }

    let error = OfficialFilesystemFormat::try_from(FilesystemKind::Fat12).unwrap_err();
    assert_eq!(error.kind, FilesystemErrorKind::FormatUnsupported);
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
