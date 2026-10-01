use super::{
    BootSectorReader, DetectionConfidence, DetectionResult, FilesystemDriver, FilesystemError,
    FilesystemErrorKind, FilesystemKind, FilesystemReader,
};

static DEFAULT_DRIVERS: [&'static dyn FilesystemDriver; 5] = [
    &super::EXFAT_DRIVER,
    &super::NTFS_DRIVER,
    &super::FAT32_DRIVER,
    &super::FAT16_DRIVER,
    &super::FAT12_DRIVER,
];

pub fn default_registry() -> DriverRegistry<'static> {
    DriverRegistry::new(&DEFAULT_DRIVERS)
}

pub fn detect_boot_sector(
    sector_count: u64,
    boot: &[u8],
) -> Result<Option<FilesystemKind>, FilesystemError> {
    if boot.len() != 512 {
        return Err(FilesystemError::new(
            FilesystemErrorKind::ReadFailure,
            "文件系统首扇区长度不是 512B",
        ));
    }
    let mut reader = BootSectorReader::new(boot, sector_count);
    default_registry()
        .detect(&mut reader)
        .map(|detected| detected.map(|value| value.kind()))
}

pub fn detect_boot_sector_with_geometry(
    partition_offset: u64,
    sector_count: u64,
    boot: &[u8],
) -> Result<Option<FilesystemKind>, FilesystemError> {
    if boot.len() != 512 {
        return Err(FilesystemError::new(
            FilesystemErrorKind::ReadFailure,
            "文件系统首扇区长度不是 512B",
        ));
    }
    let registry = default_registry();
    let mut detection_reader = BootSectorReader::new(boot, sector_count);
    let Some(detected) = registry.detect(&mut detection_reader)? else {
        return Ok(None);
    };
    let kind = detected.kind();
    let geometry = super::FilesystemGeometry::new(partition_offset, sector_count, 512);
    let mut geometry_reader = BootSectorReader::new(boot, sector_count);
    match detected.driver.matches_geometry(&mut geometry_reader, geometry) {
        Ok(true) => Ok(Some(kind)),
        Ok(false) => Ok(None),
        Err(error) if error.kind == FilesystemErrorKind::Unsupported => Ok(Some(kind)),
        Err(error) => Err(error),
    }
}

pub struct DetectedFilesystem<'a> {
    pub driver: &'a dyn FilesystemDriver,
    pub result: DetectionResult,
}

impl DetectedFilesystem<'_> {
    pub const fn kind(&self) -> FilesystemKind {
        self.result.kind
    }
}

pub struct DriverRegistry<'a> {
    drivers: &'a [&'a dyn FilesystemDriver],
}

impl<'a> DriverRegistry<'a> {
    pub const fn new(drivers: &'a [&'a dyn FilesystemDriver]) -> Self {
        Self { drivers }
    }

    pub fn driver(&self, kind: FilesystemKind) -> Option<&'a dyn FilesystemDriver> {
        self.drivers
            .iter()
            .copied()
            .find(|driver| driver.kind() == kind)
    }

    pub fn detect(
        &self,
        source: &mut dyn FilesystemReader,
    ) -> Result<Option<DetectedFilesystem<'a>>, FilesystemError> {
        let mut best: Option<DetectedFilesystem<'a>> = None;

        for driver in self.drivers.iter().copied() {
            if !driver.capabilities().detect {
                continue;
            }
            let result = driver.detect(source)?;
            if result.confidence == DetectionConfidence::NoMatch {
                continue;
            }
            match &best {
                None => best = Some(DetectedFilesystem { driver, result }),
                Some(current) if result.confidence > current.result.confidence => {
                    best = Some(DetectedFilesystem { driver, result });
                }
                Some(current)
                    if result.confidence == current.result.confidence
                        && result.kind != current.result.kind =>
                {
                    return Err(FilesystemError::new(
                        FilesystemErrorKind::AmbiguousDetection,
                        format!(
                            "文件系统识别出现同级歧义：{} 与 {}",
                            current.result.kind.display_name(),
                            result.kind.display_name()
                        ),
                    ));
                }
                _ => {}
            }
        }

        Ok(best)
    }
}
