use std::collections::BTreeMap;

use encoding_rs::GBK;

use super::{
    DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemMetadata,
    FilesystemReader, FilesystemWrite, FormatPlan, FormatRequest, FormatVerification,
    NativeFilesystemWrite, NativeFormatPlan,
};

const SECTOR_SIZE: usize = 512;
const ROOT_ENTRIES: u16 = 512;
// Legacy 512B resource accounting; native writer derives this from BPB bytes/sector.
const ROOT_SECTORS: u64 = 32;
const RESERVED: u64 = 1;
const COPIES: u64 = 2;

pub struct Fat16Driver;

pub static FAT16_DRIVER: Fat16Driver = Fat16Driver;

fn u16le(raw: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([raw[offset], raw[offset + 1]])
}

fn u32le(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn valid_boot_jump(boot: &[u8; SECTOR_SIZE]) -> bool {
    (boot[0] == 0xeb && boot[2] == 0x90) || boot[0] == 0xe9
}

fn valid_spc(spc: u32) -> bool {
    spc != 0 && spc.is_power_of_two() && spc <= 128
}

fn fat16_geometry(boot: &[u8; SECTOR_SIZE], partition_sectors: u64) -> Option<(u64, u64)> {
    if boot[510..512] != [0x55, 0xaa] || !valid_boot_jump(boot) {
        return None;
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
    if bps != SECTOR_SIZE as u32
        || !valid_spc(spc)
        || reserved == 0
        || !matches!(fats, 1 | 2)
        || media < 0xf0
        || total == 0
        || total > partition_sectors
        || fat_size == 0
        || root_entries == 0
    {
        return None;
    }
    let root_dir_sectors = root_entries.checked_mul(32)?.div_ceil(SECTOR_SIZE as u64);
    let metadata_sectors = reserved
        .checked_add(fats.checked_mul(fat_size)?)?
        .checked_add(root_dir_sectors)?;
    let data_sectors = total.checked_sub(metadata_sectors)?;
    let cluster_count = data_sectors / spc as u64;
    if !(4_085..65_525).contains(&cluster_count) {
        return None;
    }
    let root_start = reserved.checked_add(fats.checked_mul(fat_size)?)?;
    Some((total, root_start))
}

pub(super) fn encode_label(label: Option<&str>) -> Result<[u8; 11], FilesystemError> {
    let Some(label) = label else {
        return Ok(*b"NO NAME    ");
    };
    if label.is_empty() {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat16,
            FilesystemErrorKind::InvalidVolumeLabel,
            "FAT16 空卷标必须使用 None 表示",
        ));
    }
    if label
        .chars()
        .any(|ch| ch.is_control() || "\"*/:<>?\\|".contains(ch))
    {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat16,
            FilesystemErrorKind::InvalidVolumeLabel,
            "FAT16 卷标包含禁止字符",
        ));
    }
    let uppercase = label.to_uppercase();
    let (encoded, _, had_errors) = GBK.encode(&uppercase);
    if had_errors || encoded.len() > 11 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat16,
            FilesystemErrorKind::InvalidVolumeLabel,
            "FAT16 卷标无法编码到 11 个 GBK 字节内",
        ));
    }
    let mut out = [b' '; 11];
    out[..encoded.len()].copy_from_slice(&encoded);
    Ok(out)
}

pub(super) fn decode_label(raw: &[u8]) -> Result<Option<String>, FilesystemError> {
    if raw.len() != 11 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat16,
            FilesystemErrorKind::InvalidMetadata,
            "FAT16 卷标字段长度错误",
        ));
    }
    if raw.iter().all(|byte| *byte == 0 || *byte == b' ') || raw == b"NO NAME    " {
        return Ok(None);
    }
    let (decoded, _, had_errors) = GBK.decode(raw);
    if had_errors {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat16,
            FilesystemErrorKind::InvalidMetadata,
            "FAT16 卷标无法按 GBK 解码",
        ));
    }
    let value = decoded
        .trim_end_matches(' ')
        .trim_end_matches(char::from(0))
        .to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn put_u16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

impl Fat16Driver {
    /// Build full-native-block FAT16 metadata for the shared provision plan.
    /// This pure generator cannot grant disk write access or bypass the native
    /// application's identity pin, write lease, WAL, or verification.
    pub fn build_native_format_plan(
        &self,
        geometry: FilesystemGeometry,
        request: &FormatRequest,
    ) -> Result<NativeFormatPlan, FilesystemError> {
        self.validate_format_request(request)?;
        if !super::format::native_fat_sector_bytes_supported(geometry.sector_size)
            || geometry.sector_count == 0
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "仅允许已认证的 512B/4096B FAT16 虚拟格式化几何",
            ));
        }
        let sector_bytes = u64::from(geometry.sector_size);
        let sector_len = geometry.sector_size as usize;
        let total = u32::try_from(geometry.sector_count).map_err(|_| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT16 分区扇区数超过 u32",
            )
        })?;
        let hidden = u32::try_from(geometry.partition_offset).map_err(|_| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT16 hidden sectors 超过 u32",
            )
        })?;
        let volume_serial = request.volume_serial.ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT16 格式化需要卷序列号",
            )
        })?;
        let label = encode_label(request.volume_label.as_deref())?;
        let (spc, fat_sectors) =
            choose_format_geometry_for_sector_size(geometry.sector_count, geometry.sector_size)?;
        let root_sectors = (u64::from(ROOT_ENTRIES) * 32).div_ceil(sector_bytes);
        let root_start = RESERVED + COPIES * u64::from(fat_sectors);
        let metadata_sectors = root_start.checked_add(root_sectors).ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT16 元数据边界溢出",
            )
        })?;
        if metadata_sectors > geometry.sector_count {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT16 元数据超过分区末尾",
            ));
        }
        super::FormatResourceBudget::default().check(
            super::FormatResourceEstimate::from_native_sectors(
                metadata_sectors,
                geometry.sector_size,
            )?,
        )?;

        let mut boot = vec![0u8; sector_len];
        boot[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
        // Preserve first-party FAT16 OEM field in either geometry.
        boot[3..11].copy_from_slice(b"MSDOS5.0");
        put_u16(&mut boot, 11, geometry.sector_size as u16);
        boot[13] = spc;
        put_u16(&mut boot, 14, RESERVED as u16);
        boot[16] = COPIES as u8;
        put_u16(&mut boot, 17, ROOT_ENTRIES);
        if total <= u16::MAX as u32 {
            put_u16(&mut boot, 19, total as u16);
        }
        boot[21] = 0xf8;
        put_u16(&mut boot, 22, fat_sectors);
        put_u16(&mut boot, 24, 63);
        put_u16(&mut boot, 26, 255);
        put_u32(&mut boot, 28, hidden);
        if total > u16::MAX as u32 {
            put_u32(&mut boot, 32, total);
        }
        boot[36] = 0x80;
        boot[38] = 0x29;
        put_u32(&mut boot, 39, volume_serial);
        boot[43..54].copy_from_slice(&label);
        boot[54..62].copy_from_slice(b"FAT16   ");
        boot[510..512].copy_from_slice(&[0x55, 0xaa]);

        let mut sectors = BTreeMap::new();
        sectors.insert(0, boot);
        for copy in 0..COPIES {
            for offset in 0..u64::from(fat_sectors) {
                let mut fat = vec![0u8; sector_len];
                if offset == 0 {
                    fat[..4].copy_from_slice(&[0xf8, 0xff, 0xff, 0xff]);
                }
                sectors.insert(RESERVED + copy * u64::from(fat_sectors) + offset, fat);
            }
        }
        let mut root = vec![0u8; sector_len];
        if request.volume_label.is_some() {
            root[..11].copy_from_slice(&label);
            root[11] = 0x08;
        }
        sectors.insert(root_start, root);
        for offset in 1..root_sectors {
            sectors.insert(root_start + offset, vec![0u8; sector_len]);
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
                volume_label: decode_label(&label)?,
                volume_serial: Some(volume_serial),
            },
        })
    }
}

impl FilesystemDriver for Fat16Driver {
    fn kind(&self) -> FilesystemKind {
        FilesystemKind::Fat16
    }

    fn capabilities(&self) -> FilesystemCapabilities {
        FilesystemCapabilities {
            detect: true,
            read_metadata: true,
            format: true,
            verify_format: true,
            analyze: true,
        }
    }

    fn detect(
        &self,
        source: &mut dyn FilesystemReader,
    ) -> Result<DetectionResult, FilesystemError> {
        if source.sector_size() != SECTOR_SIZE as u32 {
            return Ok(DetectionResult::no_match(self.kind()));
        }
        let boot = source.read_sector(0)?;
        let confidence = if fat16_geometry(&boot, source.sector_count()).is_some() {
            DetectionConfidence::Exact
        } else {
            DetectionConfidence::NoMatch
        };
        Ok(DetectionResult {
            kind: self.kind(),
            confidence,
        })
    }

    fn matches_geometry(
        &self,
        source: &mut dyn FilesystemReader,
        geometry: FilesystemGeometry,
    ) -> Result<bool, FilesystemError> {
        if geometry.sector_size != SECTOR_SIZE as u32
            || source.sector_size() != SECTOR_SIZE as u32
            || source.sector_count() != geometry.sector_count
        {
            return Ok(false);
        }
        let boot = source.read_sector(0)?;
        let Some((total, _)) = fat16_geometry(&boot, geometry.sector_count) else {
            return Ok(false);
        };
        Ok(total == geometry.sector_count && u32le(&boot, 28) as u64 == geometry.partition_offset)
    }

    fn read_metadata(
        &self,
        source: &mut dyn FilesystemReader,
    ) -> Result<FilesystemMetadata, FilesystemError> {
        let boot = source.read_sector(0)?;
        if fat16_geometry(&boot, source.sector_count()).is_none() {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidBootSector,
                "FAT16 引导扇区校验失败",
            ));
        }
        Ok(FilesystemMetadata {
            kind: self.kind(),
            volume_label: decode_label(&boot[43..54])?,
            volume_serial: Some(u32le(&boot, 39)),
        })
    }

    fn validate_format_request(&self, request: &FormatRequest) -> Result<(), FilesystemError> {
        if request.filesystem != self.kind() {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT16 驱动收到其他文件系统的格式化请求",
            ));
        }
        encode_label(request.volume_label.as_deref()).map(|_| ())
    }

    fn expected_format_metadata(
        &self,
        request: &FormatRequest,
    ) -> Result<FilesystemMetadata, FilesystemError> {
        self.validate_format_request(request)?;
        let volume_serial = request.volume_serial.ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT16 格式化需要卷序列号",
            )
        })?;
        let label = encode_label(request.volume_label.as_deref())?;
        Ok(FilesystemMetadata {
            kind: self.kind(),
            volume_label: decode_label(&label)?,
            volume_serial: Some(volume_serial),
        })
    }

    fn build_format_plan(
        &self,
        geometry: FilesystemGeometry,
        request: &FormatRequest,
    ) -> Result<FormatPlan, FilesystemError> {
        // Preserve the legacy 512-byte write contract and all existing callers.
        // Native 4Kn plans are intentionally NOT routed to the physical writer.
        if geometry.sector_size != SECTOR_SIZE as u32 {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "旧 FAT16 写入器仅支持 512B 逻辑扇区",
            ));
        }
        let native = self.build_native_format_plan(geometry, request)?;
        let writes = native
            .writes
            .into_iter()
            .map(|write| {
                let data = write.data.try_into().map_err(|_| {
                    FilesystemError::for_filesystem(
                        FilesystemKind::Fat16,
                        FilesystemErrorKind::InvalidGeometry,
                        "FAT16 512B 格式化块长度不一致",
                    )
                })?;
                Ok(FilesystemWrite {
                    relative_lba: write.relative_lba,
                    data,
                })
            })
            .collect::<Result<Vec<_>, FilesystemError>>()?;
        Ok(FormatPlan {
            filesystem: native.filesystem,
            geometry,
            writes,
            expected_metadata: native.expected_metadata,
        })
    }

    fn verify_format(
        &self,
        source: &mut dyn FilesystemReader,
        geometry: FilesystemGeometry,
        expected: &FilesystemMetadata,
    ) -> Result<FormatVerification, FilesystemError> {
        if geometry.sector_size != SECTOR_SIZE as u32
            || source.sector_size() != SECTOR_SIZE as u32
            || source.sector_count() != geometry.sector_count
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT16 读回分区几何不一致",
            ));
        }
        let boot = source.read_sector(0)?;
        let Some((total, root_start)) = fat16_geometry(&boot, source.sector_count()) else {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidBootSector,
                "FAT16 读回引导扇区校验失败",
            ));
        };
        if u32le(&boot, 28) as u64 != geometry.partition_offset || total != geometry.sector_count {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT16 hidden sectors 或总扇区数读回不一致",
            ));
        }
        if expected
            .volume_serial
            .is_some_and(|serial| u32le(&boot, 39) != serial)
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT16 卷序列号读回不一致",
            ));
        }

        let root = source.read_sector(root_start)?;
        let actual_label = decode_label(&boot[43..54])?;
        match expected.volume_label.as_deref() {
            None => {
                if boot[43..54] != *b"NO NAME    " || root[11] == 0x08 {
                    return Err(FilesystemError::for_filesystem(
                        self.kind(),
                        FilesystemErrorKind::InvalidVolumeLabel,
                        "FAT16 空卷标读回不一致",
                    ));
                }
            }
            Some(label) => {
                if root[11] != 0x08 || boot[43..54] != root[..11] {
                    return Err(FilesystemError::for_filesystem(
                        self.kind(),
                        FilesystemErrorKind::InvalidVolumeLabel,
                        "FAT16 BPB 与根目录卷标项读回不一致",
                    ));
                }
                if actual_label.as_deref() != Some(label) {
                    return Err(FilesystemError::for_filesystem(
                        self.kind(),
                        FilesystemErrorKind::InvalidVolumeLabel,
                        "FAT16 卷标读回不一致",
                    ));
                }
            }
        }

        Ok(FormatVerification {
            metadata: FilesystemMetadata {
                kind: self.kind(),
                volume_label: actual_label,
                volume_serial: Some(u32le(&boot, 39)),
            },
        })
    }
}

/// Shared FAT16 layout solver for 512B and 4Kn. Both paths use native LBAs.
pub(super) fn choose_format_geometry_for_sector_size(
    volume_sectors: u64,
    sector_bytes: u32,
) -> Result<(u8, u16), FilesystemError> {
    let mut chosen = None;
    let root_sectors = (u64::from(ROOT_ENTRIES) * 32).div_ceil(u64::from(sector_bytes));
    for spc in [1u64, 2, 4, 8, 16, 32, 64, 128] {
        // FAT16 cluster size must remain representable by common OS readers.
        if spc * u64::from(sector_bytes) > 65_536 {
            break;
        }
        let mut fat_sectors = 1u64;
        for _ in 0..16 {
            let overhead = RESERVED + COPIES * fat_sectors + root_sectors;
            if volume_sectors <= overhead {
                break;
            }
            let clusters = (volume_sectors - overhead) / spc;
            let next = ((clusters + 2) * 2).div_ceil(u64::from(sector_bytes));
            if next == fat_sectors {
                if (4_085..65_525).contains(&clusters) && fat_sectors <= u16::MAX as u64 {
                    chosen = Some((spc as u8, fat_sectors as u16));
                }
                break;
            }
            fat_sectors = next;
        }
        if chosen.is_some() {
            break;
        }
    }
    chosen.ok_or_else(|| {
        FilesystemError::for_filesystem(
            FilesystemKind::Fat16,
            FilesystemErrorKind::InvalidGeometry,
            "该分区大小无法表示为 FAT16",
        )
    })
}

fn choose_format_geometry(volume_sectors: u64) -> Result<(u8, u16), FilesystemError> {
    choose_format_geometry_for_sector_size(volume_sectors, SECTOR_SIZE as u32)
}

pub(super) fn format_sector_count(volume_sectors: u64) -> Result<u64, FilesystemError> {
    if volume_sectors > u32::MAX as u64 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat16,
            FilesystemErrorKind::InvalidGeometry,
            "FAT16 分区扇区数超过 u32",
        ));
    }
    let (_, fat) = choose_format_geometry(volume_sectors)?;
    Ok(1 + COPIES * u64::from(fat) + ROOT_SECTORS)
}
