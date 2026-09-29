use super::{
    DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemKind, FilesystemMetadata, FilesystemReader,
};

pub struct NtfsDriver;
pub static NTFS_DRIVER: NtfsDriver = NtfsDriver;

fn u16le(raw: &[u8; 512], offset: usize) -> u16 {
    u16::from_le_bytes([raw[offset], raw[offset + 1]])
}

fn u64le(raw: &[u8; 512], offset: usize) -> u64 {
    u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap())
}

fn valid_boot_jump(boot: &[u8; 512]) -> bool {
    (boot[0] == 0xeb && boot[2] == 0x90) || boot[0] == 0xe9
}

fn valid_spc(spc: u32) -> bool {
    spc != 0 && spc.is_power_of_two() && spc <= 128
}

fn matches_ntfs(boot: &[u8; 512], partition_sectors: u64) -> bool {
    if boot[510..512] != [0x55, 0xaa]
        || !valid_boot_jump(boot)
        || boot.get(3..11) != Some(b"NTFS    ")
    {
        return false;
    }
    let bps = u16le(boot, 11) as u32;
    let spc = boot[13] as u32;
    let total = u64le(boot, 40);
    if bps != 512
        || !valid_spc(spc)
        || total == 0
        || total > partition_sectors
        || !boot[14..21].iter().all(|byte| *byte == 0)
        || boot[21] < 0xf0
    {
        return false;
    }
    let Some(clusters) = total.checked_div(u64::from(spc)) else {
        return false;
    };
    let mft = u64le(boot, 48);
    let mft_mirror = u64le(boot, 56);
    clusters > 0 && mft < clusters && mft_mirror < clusters
}

impl FilesystemDriver for NtfsDriver {
    fn kind(&self) -> FilesystemKind {
        FilesystemKind::Ntfs
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
            confidence: if matches_ntfs(&boot, source.sector_count()) {
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
                "NTFS 引导扇区校验失败",
            ));
        }
        Ok(FilesystemMetadata::new(self.kind()))
    }
}
