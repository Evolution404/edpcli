use std::collections::BTreeMap;

use encoding_rs::GBK;

use super::{
    DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemMetadata,
    FilesystemReader, FilesystemWrite, FormatPlan, FormatRequest, FormatVerification,
    NativeFilesystemWrite, NativeFormatPlan,
};

const SECTOR_SIZE: usize = 512;
const RESERVED: u64 = 32;
const COPIES: u64 = 2;
const ROOT_CLUSTER: u32 = 2;
const FSINFO_SECTOR: u16 = 1;
const BACKUP_BOOT_SECTOR: u16 = 6;
const BACKUP_FSINFO_SECTOR: u16 = 7;
const MAX_VALIDATED_CLUSTERS: u64 = 4_194_304;

pub struct Fat32Driver;
pub static FAT32_DRIVER: Fat32Driver = Fat32Driver;

#[derive(Clone, Copy, Debug)]
struct Fat32Geometry {
    total: u64,
    sectors_per_cluster: u8,
    reserved: u64,
    copies: u64,
    fat_sectors: u64,
    cluster_count: u64,
    root_cluster: u32,
    data_start: u64,
    fsinfo_sector: u16,
    backup_boot_sector: u16,
}

fn u16le(raw: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([raw[offset], raw[offset + 1]])
}

fn u32le(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn put_u16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn valid_boot_jump(boot: &[u8; SECTOR_SIZE]) -> bool {
    (boot[0] == 0xeb && boot[2] == 0x90) || boot[0] == 0xe9
}

fn valid_spc(spc: u32) -> bool {
    spc != 0 && spc.is_power_of_two() && spc <= 128
}

fn fat32_geometry(boot: &[u8; SECTOR_SIZE], partition_sectors: u64) -> Option<Fat32Geometry> {
    if boot[510..512] != [0x55, 0xaa] || !valid_boot_jump(boot) {
        return None;
    }
    let bps = u16le(boot, 11) as u32;
    let spc = boot[13] as u32;
    let reserved = u16le(boot, 14) as u64;
    let copies = boot[16] as u64;
    let root_entries = u16le(boot, 17);
    let total16 = u16le(boot, 19);
    let media = boot[21];
    let fat16_size = u16le(boot, 22);
    let total = u32le(boot, 32) as u64;
    let fat_sectors = u32le(boot, 36) as u64;
    let flags = u16le(boot, 40);
    let version = u16le(boot, 42);
    let root_cluster = u32le(boot, 44);
    let fsinfo_sector = u16le(boot, 48);
    let backup_boot_sector = u16le(boot, 50);

    if bps != SECTOR_SIZE as u32
        || !valid_spc(spc)
        || reserved == 0
        || !matches!(copies, 1 | 2)
        || media < 0xf0
        || root_entries != 0
        || total16 != 0
        || fat16_size != 0
        || total == 0
        || total > partition_sectors
        || fat_sectors == 0
        || version != 0
        || (flags & 0x80 != 0 && u64::from(flags & 0x0f) >= copies)
    {
        return None;
    }
    if fsinfo_sector != u16::MAX && u64::from(fsinfo_sector) >= reserved {
        return None;
    }
    if !matches!(backup_boot_sector, 0 | u16::MAX) && u64::from(backup_boot_sector) >= reserved {
        return None;
    }

    let data_start = reserved.checked_add(copies.checked_mul(fat_sectors)?)?;
    let data_sectors = total.checked_sub(data_start)?;
    let cluster_count = data_sectors / u64::from(spc);
    if !(65_525..0x0fff_fff5).contains(&cluster_count) {
        return None;
    }
    if root_cluster < 2 || u64::from(root_cluster) >= cluster_count + 2 {
        return None;
    }
    let required_fat_bytes = (cluster_count + 2).checked_mul(4)?;
    if required_fat_bytes > fat_sectors.checked_mul(SECTOR_SIZE as u64)? {
        return None;
    }

    Some(Fat32Geometry {
        total,
        sectors_per_cluster: spc as u8,
        reserved,
        copies,
        fat_sectors,
        cluster_count,
        root_cluster,
        data_start,
        fsinfo_sector,
        backup_boot_sector,
    })
}

fn encode_label(label: Option<&str>) -> Result<[u8; 11], FilesystemError> {
    let Some(label) = label else {
        return Ok(*b"NO NAME    ");
    };
    if label.is_empty() {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat32,
            FilesystemErrorKind::InvalidVolumeLabel,
            "FAT32 空卷标必须使用 None 表示",
        ));
    }
    if label
        .chars()
        .any(|ch| ch.is_control() || "\"*/:<>?\\|".contains(ch))
    {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat32,
            FilesystemErrorKind::InvalidVolumeLabel,
            "FAT32 卷标包含禁止字符",
        ));
    }
    let uppercase = label.to_uppercase();
    let (encoded, _, had_errors) = GBK.encode(&uppercase);
    if had_errors || encoded.len() > 11 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat32,
            FilesystemErrorKind::InvalidVolumeLabel,
            "FAT32 卷标无法编码到 11 个 GBK 字节内",
        ));
    }
    let mut out = [b' '; 11];
    out[..encoded.len()].copy_from_slice(&encoded);
    Ok(out)
}

fn decode_label(raw: &[u8]) -> Result<Option<String>, FilesystemError> {
    if raw.len() != 11 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat32,
            FilesystemErrorKind::InvalidMetadata,
            "FAT32 卷标字段长度错误",
        ));
    }
    if raw.iter().all(|byte| *byte == 0 || *byte == b' ') || raw == b"NO NAME    " {
        return Ok(None);
    }
    let (decoded, _, had_errors) = GBK.decode(raw);
    if had_errors {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat32,
            FilesystemErrorKind::InvalidMetadata,
            "FAT32 卷标无法按 GBK 解码",
        ));
    }
    let value = decoded
        .trim_end_matches(' ')
        .trim_end_matches(char::from(0))
        .to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn root_start(geometry: Fat32Geometry) -> Option<u64> {
    let cluster = u64::from(geometry.root_cluster).checked_sub(2)?;
    geometry
        .data_start
        .checked_add(cluster.checked_mul(u64::from(geometry.sectors_per_cluster))?)
}

fn root_volume_label(
    source: &mut dyn FilesystemReader,
    geometry: Fat32Geometry,
) -> Result<Option<String>, FilesystemError> {
    let Some(start) = root_start(geometry) else {
        return Ok(None);
    };
    for relative in 0..u64::from(geometry.sectors_per_cluster) {
        let sector = source.read_sector(start + relative)?;
        for entry in sector.chunks(32) {
            match entry[0] {
                0x00 => return Ok(None),
                0xe5 => continue,
                _ if entry[11] == 0x0f => continue,
                _ if entry[11] & 0x08 != 0 && entry[11] & 0x10 == 0 => {
                    return decode_label(&entry[..11]);
                }
                _ => {}
            }
        }
    }
    Ok(None)
}

/// Compute FAT32 geometry in source-device native LBAs.
fn choose_format_geometry_for_sector_bytes(
    total: u64,
    sector_bytes: u32,
) -> Option<(u8, u32, u64)> {
    for spc in [1u64, 2, 4, 8, 16, 32, 64, 128] {
        // Windows FAT implementations typically cap a cluster at 64KiB.
        if spc.checked_mul(u64::from(sector_bytes))? > 65_536 {
            break;
        }
        let mut fat_sectors = 1u64;
        for _ in 0..32 {
            let overhead = RESERVED.checked_add(COPIES.checked_mul(fat_sectors)?)?;
            let data = total.checked_sub(overhead)?;
            let clusters = data / spc;
            let next = (clusters + 2)
                .checked_mul(4)?
                .div_ceil(u64::from(sector_bytes));
            if next <= fat_sectors {
                if (65_525..=MAX_VALIDATED_CLUSTERS).contains(&clusters)
                    && fat_sectors <= u32::MAX as u64
                {
                    return Some((spc as u8, fat_sectors as u32, clusters));
                }
                break;
            }
            fat_sectors = next;
        }
    }
    None
}

fn choose_format_geometry(total: u64) -> Option<(u8, u32, u64)> {
    choose_format_geometry_for_sector_bytes(total, SECTOR_SIZE as u32)
}

fn build_fsinfo(cluster_count: u64) -> [u8; SECTOR_SIZE] {
    let mut fsinfo = [0u8; SECTOR_SIZE];
    put_u32(&mut fsinfo, 0, 0x4161_5252);
    put_u32(&mut fsinfo, 484, 0x6141_7272);
    let free = cluster_count.saturating_sub(1).min(u32::MAX as u64) as u32;
    put_u32(&mut fsinfo, 488, free);
    put_u32(&mut fsinfo, 492, if free > 0 { 3 } else { 0xffff_ffff });
    put_u32(&mut fsinfo, 508, 0xaa55_0000);
    fsinfo
}

fn valid_fsinfo(raw: &[u8; SECTOR_SIZE], cluster_count: u64) -> bool {
    if u32le(raw, 0) != 0x4161_5252
        || u32le(raw, 484) != 0x6141_7272
        || u32le(raw, 508) != 0xaa55_0000
    {
        return false;
    }
    let free = u32le(raw, 488);
    let next = u32le(raw, 492);
    (free == u32::MAX || u64::from(free) <= cluster_count)
        && (next == u32::MAX || (2..cluster_count.saturating_add(2)).contains(&u64::from(next)))
}

impl Fat32Driver {
    /// Generate native-block FAT32 metadata on a regular-file virtual target.
    /// No 4Kn physical writer or restore path consumes this plan.
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
                "FAT32仅允许标准512B/1024B/2048B/4096B原生扇区格式化几何",
            ));
        }
        let sector_len = geometry.sector_size as usize;
        let total = u32::try_from(geometry.sector_count).map_err(|_| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT32分区的原生LBA数超出BPB u32范围",
            )
        })?;
        let hidden = u32::try_from(geometry.partition_offset).map_err(|_| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT32 hidden sectors超出u32范围",
            )
        })?;
        let volume_serial = request.volume_serial.ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT32格式化需要卷序列号",
            )
        })?;
        let label = encode_label(request.volume_label.as_deref())?;
        let (spc, fat_sectors, cluster_count) =
            choose_format_geometry_for_sector_bytes(geometry.sector_count, geometry.sector_size)
                .ok_or_else(|| {
                    FilesystemError::for_filesystem(
                        self.kind(),
                        FilesystemErrorKind::InvalidGeometry,
                        "当前分区的原生LBA与FAT32簇数范围不兼容",
                    )
                })?;
        let data_start = RESERVED
            .checked_add(COPIES * u64::from(fat_sectors))
            .ok_or_else(|| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "FAT32数据区起点溢出",
                )
            })?;
        let metadata_count = data_start
            .checked_add(u64::from(spc))
            .and_then(|sum| sum.checked_add(4))
            .ok_or_else(|| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "FAT32元数据长度溢出",
                )
            })?;
        if data_start + u64::from(spc) > geometry.sector_count {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT32根簇越界",
            ));
        }
        super::FormatResourceBudget::default().check(
            super::FormatResourceEstimate::from_native_sectors(
                metadata_count,
                geometry.sector_size,
            )?,
        )?;

        let mut boot = vec![0u8; sector_len];
        boot[0..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
        boot[3..11].copy_from_slice(b"MSDOS5.0");
        put_u16(&mut boot, 11, geometry.sector_size as u16);
        boot[13] = spc;
        put_u16(&mut boot, 14, RESERVED as u16);
        boot[16] = COPIES as u8;
        put_u16(&mut boot, 17, 0);
        put_u16(&mut boot, 19, 0);
        boot[21] = 0xf8;
        put_u16(&mut boot, 22, 0);
        put_u16(&mut boot, 24, 63);
        put_u16(&mut boot, 26, 255);
        put_u32(&mut boot, 28, hidden);
        put_u32(&mut boot, 32, total);
        put_u32(&mut boot, 36, fat_sectors);
        put_u16(&mut boot, 40, 0);
        put_u16(&mut boot, 42, 0);
        put_u32(&mut boot, 44, ROOT_CLUSTER);
        put_u16(&mut boot, 48, FSINFO_SECTOR);
        put_u16(&mut boot, 50, BACKUP_BOOT_SECTOR);
        boot[64] = 0x80;
        boot[66] = 0x29;
        put_u32(&mut boot, 67, volume_serial);
        boot[71..82].copy_from_slice(&label);
        boot[82..90].copy_from_slice(b"FAT32   ");
        boot[510..512].copy_from_slice(&[0x55, 0xaa]);

        let mut fsinfo = vec![0u8; sector_len];
        fsinfo[..SECTOR_SIZE].copy_from_slice(&build_fsinfo(cluster_count));
        let mut sectors = BTreeMap::new();
        sectors.insert(0, boot.clone());
        sectors.insert(u64::from(FSINFO_SECTOR), fsinfo.clone());
        sectors.insert(u64::from(BACKUP_BOOT_SECTOR), boot);
        sectors.insert(u64::from(BACKUP_FSINFO_SECTOR), fsinfo);

        for copy in 0..COPIES {
            for offset in 0..u64::from(fat_sectors) {
                let mut fat = vec![0u8; sector_len];
                if offset == 0 {
                    put_u32(&mut fat, 0, 0x0fff_fff8);
                    put_u32(&mut fat, 4, 0x0fff_ffff);
                    put_u32(&mut fat, 8, 0x0fff_ffff);
                }
                sectors.insert(RESERVED + copy * u64::from(fat_sectors) + offset, fat);
            }
        }

        for offset in 0..u64::from(spc) {
            let mut root = vec![0u8; sector_len];
            if offset == 0 && request.volume_label.is_some() {
                root[..11].copy_from_slice(&label);
                root[11] = 0x08;
            }
            sectors.insert(data_start + offset, root);
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

impl FilesystemDriver for Fat32Driver {
    fn kind(&self) -> FilesystemKind {
        FilesystemKind::Fat32
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
        Ok(DetectionResult {
            kind: self.kind(),
            confidence: if fat32_geometry(&boot, source.sector_count()).is_some() {
                DetectionConfidence::Exact
            } else {
                DetectionConfidence::NoMatch
            },
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
        let Some(parsed) = fat32_geometry(&boot, geometry.sector_count) else {
            return Ok(false);
        };
        Ok(parsed.total == geometry.sector_count
            && u32le(&boot, 28) as u64 == geometry.partition_offset)
    }

    fn read_metadata(
        &self,
        source: &mut dyn FilesystemReader,
    ) -> Result<FilesystemMetadata, FilesystemError> {
        let boot = source.read_sector(0)?;
        let geometry = fat32_geometry(&boot, source.sector_count()).ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidBootSector,
                "FAT32 引导扇区校验失败",
            )
        })?;
        let boot_label = decode_label(&boot[71..82])?;
        let volume_label = if boot_label.is_some() {
            boot_label
        } else {
            root_volume_label(source, geometry)?
        };
        Ok(FilesystemMetadata {
            kind: self.kind(),
            volume_label,
            volume_serial: Some(u32le(&boot, 67)),
        })
    }

    fn validate_format_request(&self, request: &FormatRequest) -> Result<(), FilesystemError> {
        if request.filesystem != self.kind() {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT32 驱动收到其他文件系统的格式化请求",
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
                "FAT32 格式化需要卷序列号",
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
        // All physical provision writers still accept only 512B full blocks.
        if geometry.sector_size != SECTOR_SIZE as u32 {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT32旧写入器仅支持512B逻辑扇区",
            ));
        }
        let native = self.build_native_format_plan(geometry, request)?;
        let writes = native
            .writes
            .into_iter()
            .map(|write| {
                let data = write.data.try_into().map_err(|_| {
                    FilesystemError::for_filesystem(
                        self.kind(),
                        FilesystemErrorKind::InvalidGeometry,
                        "FAT32 512B格式化块长度不一致",
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
                "FAT32 读回分区几何不一致",
            ));
        }

        let boot = source.read_sector(0)?;
        let parsed = fat32_geometry(&boot, source.sector_count()).ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidBootSector,
                "FAT32 读回引导扇区校验失败",
            )
        })?;
        if parsed.total != geometry.sector_count
            || u32le(&boot, 28) as u64 != geometry.partition_offset
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT32 hidden sectors 或总扇区数读回不一致",
            ));
        }
        if parsed.fsinfo_sector != FSINFO_SECTOR
            || parsed.backup_boot_sector != BACKUP_BOOT_SECTOR
            || parsed.reserved != RESERVED
            || parsed.copies != COPIES
            || parsed.root_cluster != ROOT_CLUSTER
            || parsed.cluster_count > MAX_VALIDATED_CLUSTERS
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "FAT32 保留区、FSInfo、备份引导或根目录几何与格式化契约不一致",
            ));
        }

        let fsinfo = source.read_sector(u64::from(FSINFO_SECTOR))?;
        let backup = source.read_sector(u64::from(BACKUP_BOOT_SECTOR))?;
        let backup_fsinfo = source.read_sector(u64::from(BACKUP_FSINFO_SECTOR))?;
        if backup != boot || backup_fsinfo != fsinfo || !valid_fsinfo(&fsinfo, parsed.cluster_count)
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT32 FSInfo 或备份引导扇区读回不一致",
            ));
        }

        let first_fat = source.read_sector(parsed.reserved)?;
        if u32le(&first_fat, 0) & 0x0fff_ffff != 0x0fff_fff8
            || u32le(&first_fat, 4) & 0x0fff_ffff != 0x0fff_ffff
            || u32le(&first_fat, 8) & 0x0fff_ffff != 0x0fff_ffff
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "FAT32 FAT 保留项或根目录簇链无效",
            ));
        }
        if parsed.copies == 2 {
            let second_fat = source.read_sector(parsed.reserved + parsed.fat_sectors)?;
            if second_fat != first_fat {
                return Err(FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidMetadata,
                    "FAT32 两份 FAT 首扇区不一致",
                ));
            }
        }

        let metadata = self.read_metadata(source)?;
        if metadata != *expected {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                format!("FAT32 元数据读回不一致：expected={expected:?}, actual={metadata:?}"),
            ));
        }

        Ok(FormatVerification { metadata })
    }
}

pub(super) fn format_sector_count(volume_sectors: u64) -> Result<u64, FilesystemError> {
    if volume_sectors > u32::MAX as u64 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::Fat32,
            FilesystemErrorKind::InvalidGeometry,
            "FAT32 分区扇区数超过 u32",
        ));
    }
    let (spc, fat, _) = choose_format_geometry(volume_sectors).ok_or_else(|| {
        FilesystemError::for_filesystem(
            FilesystemKind::Fat32,
            FilesystemErrorKind::InvalidGeometry,
            "无法表示为当前已验证范围内的 FAT32",
        )
    })?;
    Ok(4 + COPIES * u64::from(fat) + u64::from(spc))
}
