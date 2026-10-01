use super::{
    DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemKind, FilesystemMetadata, FilesystemReader,
};

pub struct Fat12Driver;
pub static FAT12_DRIVER: Fat12Driver = Fat12Driver;

fn u16le(raw: &[u8; 512], offset: usize) -> u16 {
    u16::from_le_bytes([raw[offset], raw[offset + 1]])
}

fn u32le(raw: &[u8; 512], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn valid_boot_jump(boot: &[u8; 512]) -> bool {
    (boot[0] == 0xeb && boot[2] == 0x90) || boot[0] == 0xe9
}

fn valid_spc(spc: u32) -> bool {
    spc != 0 && spc.is_power_of_two() && spc <= 128
}

fn matches_fat12(boot: &[u8; 512], partition_sectors: u64) -> bool {
    if boot[510..512] != [0x55, 0xaa] || !valid_boot_jump(boot) {
        return false;
    }
    let bps = u16le(boot, 11) as u32;
    let spc = boot[13] as u32;
    let reserved = u16le(boot, 14) as u64;
    let fats = boot[16] as u64;
    let root_entries = u16le(boot, 17) as u64;
    let total16 = u16le(boot, 19) as u64;
    let media = boot[21];
    let fat_size = u16le(boot, 22) as u64;
    let total32 = u32le(boot, 32) as u64;
    let total = if total16 != 0 { total16 } else { total32 };
    if bps != 512
        || !valid_spc(spc)
        || reserved == 0
        || !matches!(fats, 1 | 2)
        || media < 0xf0
        || total == 0
        || total > partition_sectors
        || fat_size == 0
        || root_entries == 0
    {
        return false;
    }
    let root_dir_sectors = root_entries.saturating_mul(32).div_ceil(512);
    let metadata = reserved
        .saturating_add(fats.saturating_mul(fat_size))
        .saturating_add(root_dir_sectors);
    let Some(data) = total.checked_sub(metadata) else {
        return false;
    };
    data / u64::from(spc) < 4_085
}

impl FilesystemDriver for Fat12Driver {
    fn kind(&self) -> FilesystemKind {
        FilesystemKind::Fat12
    }

    fn capabilities(&self) -> FilesystemCapabilities {
        FilesystemCapabilities {
            detect: true,
            read_metadata: true,
            format: false,
            verify_format: false,
            analyze: false,
        }
    }

    fn detect(
        &self,
        source: &mut dyn FilesystemReader,
    ) -> Result<DetectionResult, FilesystemError> {
        if source.sector_size() != 512 {
            return Ok(DetectionResult::no_match(self.kind()));
        }
        let boot = source.read_sector(0)?;
        Ok(DetectionResult {
            kind: self.kind(),
            confidence: if matches_fat12(&boot, source.sector_count()) {
                DetectionConfidence::Exact
            } else {
                DetectionConfidence::NoMatch
            },
        })
    }

    fn read_metadata(
        &self,
        source: &mut dyn FilesystemReader,
    ) -> Result<FilesystemMetadata, FilesystemError> {
        if self.detect(source)?.confidence != DetectionConfidence::Exact {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidBootSector,
                "FAT12 引导扇区校验失败",
            ));
        }
        Ok(FilesystemMetadata::new(self.kind()))
    }
}
