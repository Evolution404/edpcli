use std::collections::BTreeMap;

use super::{
    DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemMetadata,
    FilesystemReader, FormatRequest, NativeFilesystemWrite, NativeFormatPlan,
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

const ROOT_ENTRIES: u16 = 512;
const RESERVED: u64 = 1;
const FAT_COPIES: u64 = 2;
const MAX_FAT12_CLUSTERS: u64 = 4_084;

fn invalid_geometry(message: &str) -> FilesystemError {
    FilesystemError::for_filesystem(
        FilesystemKind::Fat12,
        FilesystemErrorKind::InvalidGeometry,
        message,
    )
}

fn put16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Native FAT12 geometry solver. The on-disk FAT uses packed 12-bit entries,
/// not FAT16's 16-bit indexing, and root directory entries are fixed at 32B.
fn choose_native_fat12_geometry(total: u64, bps: u32) -> Option<(u8, u16, u64)> {
    let root_sectors = (u64::from(ROOT_ENTRIES) * 32).div_ceil(u64::from(bps));
    for spc in [1u64, 2, 4, 8, 16, 32, 64, 128] {
        if spc.checked_mul(u64::from(bps))? > 65_536 {
            break;
        }
        let mut fat_sectors = 1u64;
        for _ in 0..32 {
            let overhead = RESERVED
                .checked_add(FAT_COPIES.checked_mul(fat_sectors)?)?
                .checked_add(root_sectors)?;
            let data = total.checked_sub(overhead)?;
            let clusters = data / spc;
            if clusters == 0 {
                break;
            }
            let required_bytes = (clusters + 2).checked_mul(3)?.div_ceil(2);
            let next = required_bytes.div_ceil(u64::from(bps));
            if next <= fat_sectors {
                if clusters <= MAX_FAT12_CLUSTERS && fat_sectors <= u16::MAX as u64 {
                    return Some((spc as u8, fat_sectors as u16, clusters));
                }
                break;
            }
            fat_sectors = next;
        }
    }
    None
}

impl Fat12Driver {
    /// Virtual-only native FAT12 metadata generator. Does not enable the
    /// legacy 512B provision writer, whose FAT12 capability remains disabled.
    pub fn build_native_format_plan(
        &self,
        geometry: FilesystemGeometry,
        request: &FormatRequest,
    ) -> Result<NativeFormatPlan, FilesystemError> {
        if request.filesystem != FilesystemKind::Fat12 {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT12 虚拟格式化收到错误文件系统类型",
            ));
        }
        if !matches!(geometry.sector_size, 512 | 4096) || geometry.sector_count == 0 {
            return Err(invalid_geometry(
                "FAT12仅允许认证的512B/4096B原生虚拟格式化",
            ));
        }
        let sector_bytes = u64::from(geometry.sector_size);
        // Reject illegal geometry before any allocation or multiplication.
        if geometry
            .partition_offset
            .checked_add(geometry.sector_count)
            .is_none()
            || geometry
                .partition_offset
                .checked_mul(sector_bytes)
                .is_none()
            || geometry.sector_count.checked_mul(sector_bytes).is_none()
        {
            return Err(invalid_geometry("FAT12 分区原生字节边界溢出"));
        }
        let total = u32::try_from(geometry.sector_count)
            .map_err(|_| invalid_geometry("FAT12 原生分区扇区数量超出 BPB u32"))?;
        let hidden = u32::try_from(geometry.partition_offset)
            .map_err(|_| invalid_geometry("FAT12 hidden sectors 超出 u32"))?;
        let serial = request.volume_serial.ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT12 格式化需要卷序列号",
            )
        })?;
        // Reuse the existing FAT volume label codec. Do not fork GBK
        // encoding rules or introduce a different 512B label representation.
        let label =
            super::fat16::encode_label(request.volume_label.as_deref()).map_err(|error| {
                FilesystemError::for_filesystem(self.kind(), error.kind, error.message)
            })?;
        let (spc, fat_sectors, clusters) =
            choose_native_fat12_geometry(geometry.sector_count, geometry.sector_size)
                .ok_or_else(|| invalid_geometry("此分区无法用已认证的 FAT12 簇数/簇大小表示"))?;
        let root_sectors = (u64::from(ROOT_ENTRIES) * 32).div_ceil(sector_bytes);
        let root_start = RESERVED + FAT_COPIES * u64::from(fat_sectors);
        let metadata_count = root_start
            .checked_add(root_sectors)
            .ok_or_else(|| invalid_geometry("FAT12 元数据扇区边界溢出"))?;
        if metadata_count >= geometry.sector_count || clusters > MAX_FAT12_CLUSTERS {
            return Err(invalid_geometry("FAT12 元数据超出卷尾或簇类型不匹配"));
        }
        super::FormatResourceBudget::default().check(
            super::FormatResourceEstimate::from_native_sectors(
                metadata_count,
                geometry.sector_size,
            )?,
        )?;

        let sector_len = geometry.sector_size as usize;
        let mut boot = vec![0u8; sector_len];
        boot[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
        boot[3..11].copy_from_slice(b"MSDOS5.0");
        put16(&mut boot, 11, geometry.sector_size as u16);
        boot[13] = spc;
        put16(&mut boot, 14, RESERVED as u16);
        boot[16] = FAT_COPIES as u8;
        put16(&mut boot, 17, ROOT_ENTRIES);
        if total <= u16::MAX as u32 {
            put16(&mut boot, 19, total as u16);
        } else {
            put32(&mut boot, 32, total);
        }
        boot[21] = 0xf8;
        put16(&mut boot, 22, fat_sectors);
        put16(&mut boot, 24, 63);
        put16(&mut boot, 26, 255);
        put32(&mut boot, 28, hidden);
        boot[36] = 0x80;
        boot[38] = 0x29;
        put32(&mut boot, 39, serial);
        boot[43..54].copy_from_slice(&label);
        boot[54..62].copy_from_slice(b"FAT12   ");
        // FAT family boot signature remains at 510 even on a native 4Kn block.
        boot[510..512].copy_from_slice(&[0x55, 0xaa]);

        let mut sectors = BTreeMap::new();
        sectors.insert(0, boot);
        for copy in 0..FAT_COPIES {
            for offset in 0..u64::from(fat_sectors) {
                let mut fat = vec![0u8; sector_len];
                if offset == 0 {
                    // FAT[0] media descriptor and FAT[1] reserved, both packed
                    // as 12-bit entries: F8 FF FF, not a FAT16 4-byte prefix.
                    fat[0..3].copy_from_slice(&[0xf8, 0xff, 0xff]);
                }
                sectors.insert(RESERVED + copy * u64::from(fat_sectors) + offset, fat);
            }
        }
        for offset in 0..root_sectors {
            let mut root = vec![0u8; sector_len];
            if offset == 0 && request.volume_label.is_some() {
                root[0..11].copy_from_slice(&label);
                root[11] = 0x08;
            }
            sectors.insert(root_start + offset, root);
        }
        Ok(NativeFormatPlan {
            filesystem: self.kind(),
            geometry,
            writes: sectors
                .into_iter()
                .map(|(relative_lba, data)| NativeFilesystemWrite { relative_lba, data })
                .collect(),
            expected_metadata: FilesystemMetadata {
                kind: self.kind(),
                volume_label: super::fat16::decode_label(&label).map_err(|error| {
                    FilesystemError::for_filesystem(self.kind(), error.kind, error.message)
                })?,
                volume_serial: Some(serial),
            },
        })
    }
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
            analyze: true,
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
