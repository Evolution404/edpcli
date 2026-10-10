use std::collections::{BTreeMap, BTreeSet};

use super::{
    DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver,
    FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemMetadata,
    FilesystemReader, FilesystemWrite, FormatPlan, FormatRequest, FormatVerification,
    NativeFilesystemWrite, NativeFormatPlan,
};

const BOOT_REGION_SECTORS: u64 = 24;
const FAT_OFFSET: u64 = BOOT_REGION_SECTORS;
const MAX_LABEL_SCAN_SECTORS: u64 = 8192;

pub struct ExFatDriver;
pub static EXFAT_DRIVER: ExFatDriver = ExFatDriver;

#[derive(Clone, Copy, Debug)]
struct ExFatGeometry {
    partition_offset: u64,
    volume_length: u64,
    fat_offset: u64,
    fat_length: u64,
    heap_offset: u64,
    cluster_count: u32,
    root_cluster: u32,
    volume_serial: u32,
    flags: u16,
    sectors_per_cluster: u64,
    fats: u64,
}

fn u16le(raw: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([raw[o], raw[o + 1]])
}
fn u32le(raw: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(raw[o..o + 4].try_into().unwrap())
}
fn u64le(raw: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(raw[o..o + 8].try_into().unwrap())
}
pub(crate) fn put_u16(dst: &mut [u8], o: usize, v: u16) {
    dst[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn put_u32(dst: &mut [u8], o: usize, v: u32) {
    dst[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn put_u64(dst: &mut [u8], o: usize, v: u64) {
    dst[o..o + 8].copy_from_slice(&v.to_le_bytes());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExFatAnalysisLayout {
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub fat_offset: u64,
    pub root_relative_lba: u64,
}

pub(crate) fn analysis_layout(boot: &[u8], partition_sectors: u64) -> Option<ExFatAnalysisLayout> {
    let boot: &[u8; 512] = boot.try_into().ok()?;
    let geometry = parse_geometry(boot, partition_sectors)?;
    Some(ExFatAnalysisLayout {
        bytes_per_sector: 512,
        sectors_per_cluster: u32::try_from(geometry.sectors_per_cluster).ok()?,
        fat_offset: geometry.fat_offset,
        root_relative_lba: geometry.heap_offset.checked_add(
            (u64::from(geometry.root_cluster) - 2).checked_mul(geometry.sectors_per_cluster)?,
        )?,
    })
}

fn parse_geometry(boot: &[u8; 512], partition_sectors: u64) -> Option<ExFatGeometry> {
    if boot[510..512] != [0x55, 0xaa]
        || !((boot[0] == 0xeb && boot[2] == 0x90) || boot[0] == 0xe9)
        || boot.get(3..11) != Some(b"EXFAT   ")
    {
        return None;
    }
    let partition_offset = u64le(boot, 64);
    let volume_length = u64le(boot, 72);
    let fat_offset = u32le(boot, 80) as u64;
    let fat_length = u32le(boot, 84) as u64;
    let heap_offset = u32le(boot, 88) as u64;
    let cluster_count = u32le(boot, 92);
    let root_cluster = u32le(boot, 96);
    let volume_serial = u32le(boot, 100);
    let flags = u16le(boot, 106);
    let bps_shift = boot[108];
    let spc_shift = boot[109];
    let fats = boot[110] as u64;
    let percent = boot[112];
    let sectors_per_cluster = 1u64.checked_shl(spc_shift as u32)?;
    let fat_end = fat_offset.checked_add(fat_length.checked_mul(fats)?)?;
    let heap_end =
        heap_offset.checked_add(u64::from(cluster_count).checked_mul(sectors_per_cluster)?)?;
    if !boot[11..64].iter().all(|b| *b == 0)
        || bps_shift != 9
        || spc_shift >= 26
        || !matches!(fats, 1 | 2)
        || !(percent <= 100 || percent == 0xff)
        || volume_length == 0
        || volume_length > partition_sectors
        || fat_offset < 24
        || fat_length == 0
        || heap_offset < fat_end
        || cluster_count == 0
        || root_cluster < 2
        || root_cluster >= cluster_count + 2
        || heap_end > volume_length
    {
        return None;
    }
    Some(ExFatGeometry {
        partition_offset,
        volume_length,
        fat_offset,
        fat_length,
        heap_offset,
        cluster_count,
        root_cluster,
        volume_serial,
        flags,
        sectors_per_cluster,
        fats,
    })
}

fn validate_label(label: Option<&str>) -> Result<Vec<u16>, FilesystemError> {
    let Some(label) = label else {
        return Ok(Vec::new());
    };
    if label.is_empty() {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::ExFat,
            FilesystemErrorKind::InvalidVolumeLabel,
            "exFAT 空卷标必须使用 None 表示",
        ));
    }
    let units = label.encode_utf16().collect::<Vec<_>>();
    if units.len() > 11 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::ExFat,
            FilesystemErrorKind::InvalidVolumeLabel,
            "exFAT 卷标超过 11 个 UTF-16 code unit",
        ));
    }
    Ok(units)
}

fn read_label(
    source: &mut dyn FilesystemReader,
    g: ExFatGeometry,
) -> Result<Option<String>, FilesystemError> {
    let active_fat = g.fat_offset
        + if g.fats == 2 && g.flags & 1 != 0 {
            g.fat_length
        } else {
            0
        };
    let mut cluster = g.root_cluster;
    let mut visited = BTreeSet::new();
    let mut read_count = 0u64;
    loop {
        if cluster < 2 || cluster >= g.cluster_count + 2 || !visited.insert(cluster) {
            return Err(FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::CorruptFilesystem,
                "exFAT 根目录簇链无效或存在环",
            ));
        }
        let rel = g
            .heap_offset
            .checked_add(
                (u64::from(cluster) - 2)
                    .checked_mul(g.sectors_per_cluster)
                    .ok_or_else(|| {
                        FilesystemError::for_filesystem(
                            FilesystemKind::ExFat,
                            FilesystemErrorKind::InvalidGeometry,
                            "exFAT 根目录簇偏移溢出",
                        )
                    })?,
            )
            .ok_or_else(|| {
                FilesystemError::for_filesystem(
                    FilesystemKind::ExFat,
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT 根目录 LBA 溢出",
                )
            })?;
        if rel
            .checked_add(g.sectors_per_cluster)
            .is_none_or(|end| end > g.volume_length)
        {
            return Err(FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::CorruptFilesystem,
                "exFAT 根目录超出卷范围",
            ));
        }
        for off in 0..g.sectors_per_cluster {
            read_count += 1;
            if read_count > MAX_LABEL_SCAN_SECTORS {
                return Err(FilesystemError::for_filesystem(
                    FilesystemKind::ExFat,
                    FilesystemErrorKind::ScanBudgetExceeded,
                    "exFAT 卷标扫描超过固定预算",
                ));
            }
            let sec = source.read_sector(rel + off)?;
            for entry in sec.as_chunks::<32>().0 {
                if entry[0] == 0x00 {
                    return Ok(None);
                }
                if entry[0] == 0x83 {
                    let count = entry[1] as usize;
                    if count > 11 {
                        return Err(FilesystemError::for_filesystem(
                            FilesystemKind::ExFat,
                            FilesystemErrorKind::InvalidMetadata,
                            "exFAT 卷标长度无效",
                        ));
                    }
                    if count == 0 {
                        return Ok(None);
                    }
                    let units = (0..count)
                        .map(|i| u16::from_le_bytes([entry[2 + i * 2], entry[3 + i * 2]]))
                        .collect::<Vec<_>>();
                    return String::from_utf16(&units).map(Some).map_err(|_| {
                        FilesystemError::for_filesystem(
                            FilesystemKind::ExFat,
                            FilesystemErrorKind::InvalidMetadata,
                            "exFAT 卷标 UTF-16 无效",
                        )
                    });
                }
            }
        }
        let byte_off = u64::from(cluster).checked_mul(4).ok_or_else(|| {
            FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::InvalidGeometry,
                "exFAT FAT entry 偏移溢出",
            )
        })?;
        let sec_idx = byte_off / 512;
        if sec_idx >= g.fat_length {
            return Err(FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::CorruptFilesystem,
                "exFAT 根目录 FAT entry 超出 FAT",
            ));
        }
        let fat = source.read_sector(active_fat + sec_idx)?;
        let o = (byte_off % 512) as usize;
        let next = u32::from_le_bytes(fat[o..o + 4].try_into().unwrap());
        if next >= 0xffff_fff8 {
            return Ok(None);
        }
        if next < 2 || next >= g.cluster_count + 2 || next >= 0xffff_fff0 {
            return Err(FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::CorruptFilesystem,
                "exFAT 根目录 FAT 链接无效",
            ));
        }
        cluster = next;
    }
}

fn align_up(v: u64, a: u64) -> Option<u64> {
    v.checked_add(a.checked_sub(1)?)?
        .checked_div(a)?
        .checked_mul(a)
}
fn preferred_shift_native(sectors: u64, sector_bytes: u32) -> Option<u8> {
    let bytes = sectors.checked_mul(u64::from(sector_bytes))?;
    const G: u64 = 1024 * 1024 * 1024;
    const M: u64 = 1024 * 1024;
    Some(if bytes >= 64 * G {
        7
    } else if bytes >= G {
        6
    } else if bytes >= 64 * M {
        4
    } else {
        3
    })
}

fn layout_native(sectors: u64, shift: u8, sector_bytes: u32) -> Option<(u64, u64, u32)> {
    if sectors <= BOOT_REGION_SECTORS || sector_bytes == 0 {
        return None;
    }
    let spc = 1u64.checked_shl(shift.into())?;
    let mut count = (sectors - BOOT_REGION_SECTORS) / spc;
    for _ in 0..16 {
        if count == 0 || count > u32::MAX as u64 - 2 {
            return None;
        }
        let fat_len = ((count + 2).checked_mul(4)?).div_ceil(u64::from(sector_bytes));
        let heap = align_up(FAT_OFFSET.checked_add(fat_len)?, spc)?;
        if heap >= sectors {
            return None;
        }
        let next = (sectors - heap) / spc;
        if next == count {
            return Some((fat_len, heap, count as u32));
        }
        count = next;
    }
    None
}

fn layout(sectors: u64, shift: u8) -> Option<(u64, u64, u32)> {
    layout_native(sectors, shift, 512)
}

fn choose_shift_native(sectors: u64, sector_bytes: u32) -> Result<u8, FilesystemError> {
    let mut tried = Vec::new();
    let initial = preferred_shift_native(sectors, sector_bytes).ok_or_else(|| {
        FilesystemError::for_filesystem(
            FilesystemKind::ExFat,
            FilesystemErrorKind::InvalidGeometry,
            format!("无法为 {sectors} 扇区生成受支持的 exFAT 布局（容量字节溢出）"),
        )
    })?;
    for shift in initial..=crate::filesystem_capability::EXFAT_MAX_VALIDATED_CLUSTER_SHIFT {
        tried.push(shift);
        if let Some((_, _, count)) = layout_native(sectors, shift, sector_bytes) {
            if count <= crate::filesystem_capability::EXFAT_MAX_VALIDATED_CLUSTERS {
                return Ok(shift);
            }
        }
    }
    Err(FilesystemError::for_filesystem(
        FilesystemKind::ExFat,
        FilesystemErrorKind::InvalidGeometry,
        format!("无法为 {sectors} 扇区生成受支持的 exFAT 布局（已尝试 shift {tried:?}）"),
    ))
}
fn choose_shift(sectors: u64) -> Result<u8, FilesystemError> {
    choose_shift_native(sectors, 512)
}

/// exFAT checksums cover eleven *complete native* blocks. The volume flags
/// and percent-in-use fields are excluded by the on-disk specification.
fn exfat_boot_checksum_native(sectors: &[Vec<u8>]) -> u32 {
    let mut sum = 0u32;
    for (si, sec) in sectors.iter().take(11).enumerate() {
        for (o, b) in sec.iter().enumerate() {
            if si == 0 && matches!(o, 106 | 107 | 112) {
                continue;
            }
            sum = sum.rotate_right(1).wrapping_add(u32::from(*b));
        }
    }
    sum
}
pub(crate) fn upcase_mapping(c: u16) -> u16 {
    if (b'a' as u16..=b'z' as u16).contains(&c) {
        c - 0x20
    } else {
        c
    }
}
pub(crate) fn exfat_upcase_table() -> Vec<u8> {
    let mut words = Vec::new();
    let mut code = 0u32;
    while code <= u16::MAX as u32 {
        let cur = code as u16;
        if upcase_mapping(cur) == cur {
            let start = code;
            while code <= u16::MAX as u32
                && upcase_mapping(code as u16) == code as u16
                && code - start < u16::MAX as u32
            {
                code += 1;
            }
            words.push(0xffff);
            words.push((code - start) as u16);
        } else {
            words.push(upcase_mapping(cur));
            code += 1;
        }
    }
    words.into_iter().flat_map(u16::to_le_bytes).collect()
}
fn checksum32(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0, |s, b| s.rotate_right(1).wrapping_add(*b as u32))
}
pub(crate) fn fat_chain(fat: &mut [u8], first: u32, count: u32) -> Result<(), FilesystemError> {
    if count == 0 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::ExFat,
            FilesystemErrorKind::InvalidGeometry,
            "exFAT 元数据流簇数量为 0",
        ));
    }
    for off in 0..count {
        let cluster = first.checked_add(off).ok_or_else(|| {
            FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::InvalidGeometry,
                "exFAT 元数据簇编号溢出",
            )
        })?;
        let next = if off + 1 == count {
            0xffff_ffff
        } else {
            cluster + 1
        };
        let at = cluster as usize * 4;
        if at + 4 > fat.len() {
            return Err(FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::InvalidGeometry,
                "exFAT 元数据链超出 FAT",
            ));
        }
        put_u32(fat, at, next);
    }
    Ok(())
}
fn put_native_stream(
    sectors: &mut BTreeMap<u64, Vec<u8>>,
    heap: u64,
    spc: u64,
    first: u32,
    count: u32,
    bytes: &[u8],
    sector_bytes: u32,
) -> Result<(), FilesystemError> {
    let sector_bytes = u64::from(sector_bytes);
    let capacity = u64::from(count)
        .checked_mul(spc)
        .and_then(|v| v.checked_mul(sector_bytes))
        .ok_or_else(|| {
            FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::InvalidGeometry,
                "exFAT 元数据流容量溢出",
            )
        })?;
    if bytes.len() as u64 > capacity || first < 2 {
        return Err(FilesystemError::for_filesystem(
            FilesystemKind::ExFat,
            FilesystemErrorKind::InvalidGeometry,
            "exFAT 元数据流超过已分配簇容量或簇编号无效",
        ));
    }
    let first_lba = heap
        .checked_add((u64::from(first) - 2).checked_mul(spc).ok_or_else(|| {
            FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::InvalidGeometry,
                "exFAT 元数据流相对簇偏移溢出",
            )
        })?)
        .ok_or_else(|| {
            FilesystemError::for_filesystem(
                FilesystemKind::ExFat,
                FilesystemErrorKind::InvalidGeometry,
                "exFAT 元数据 LBA 溢出",
            )
        })?;
    for i in 0..u64::from(count) * spc {
        let mut sec = vec![0u8; sector_bytes as usize];
        let start = (i * sector_bytes) as usize;
        if start < bytes.len() {
            let end = (start + sector_bytes as usize).min(bytes.len());
            sec[..end - start].copy_from_slice(&bytes[start..end]);
        }
        sectors.insert(first_lba + i, sec);
    }
    Ok(())
}

impl ExFatDriver {
    /// Produce complete native-block exFAT metadata for regular-file virtual disks.
    /// Legacy physical formatting still consumes 512-byte FormatPlan exclusively.
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
                "仅允许已认证512B和4096B的exFAT虚拟格式化",
            ));
        }
        let sector_size = geometry.sector_size;
        if geometry
            .partition_offset
            .checked_mul(u64::from(sector_size))
            .is_none()
            || geometry
                .partition_offset
                .checked_add(geometry.sector_count)
                .is_none()
        {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "exFAT原生分区起点或末端字节偏移溢出",
            ));
        }
        let sector_len = sector_size as usize;
        let serial = request.volume_serial.ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "exFAT格式化需要卷序列号",
            )
        })?;
        let label = validate_label(request.volume_label.as_deref())?;
        let shift = choose_shift_native(geometry.sector_count, sector_size)?;
        let spc = 1u64 << shift;
        let (fat_len, heap, cluster_count) =
            layout_native(geometry.sector_count, shift, sector_size).ok_or_else(|| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT原生几何计算失败",
                )
            })?;
        let cluster_bytes = spc.checked_mul(u64::from(sector_size)).ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "exFAT簇字节长度溢出",
            )
        })?;
        let bitmap_len = u64::from(cluster_count).div_ceil(8);
        let bitmap_clusters = u32::try_from(bitmap_len.div_ceil(cluster_bytes)).map_err(|_| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "exFAT分配位图簇数量溢出",
            )
        })?;
        let upcase = exfat_upcase_table();
        let upcase_clusters = u32::try_from((upcase.len() as u64).div_ceil(cluster_bytes))
            .map_err(|_| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT Upcase簇数量溢出",
                )
            })?;
        let root = 2u32;
        let bitmap = 3u32;
        let upcase_cluster = bitmap.checked_add(bitmap_clusters).ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "exFAT Upcase起始簇溢出",
            )
        })?;
        let allocated = 1u32
            .checked_add(bitmap_clusters)
            .and_then(|v| v.checked_add(upcase_clusters))
            .ok_or_else(|| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT已分配簇数量溢出",
                )
            })?;
        if allocated > cluster_count {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "exFAT空间不足以保存元数据",
            ));
        }

        // Preflight materialization budget on full native block payloads.
        let metadata_sectors = BOOT_REGION_SECTORS
            .checked_add(fat_len)
            .and_then(|n| n.checked_add(u64::from(allocated).checked_mul(spc)?))
            .ok_or_else(|| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT元数据块数量溢出",
                )
            })?;
        super::FormatResourceBudget::default().check(
            super::FormatResourceEstimate::from_native_sectors(metadata_sectors, sector_size)?,
        )?;

        let mut sectors = BTreeMap::new();
        let mut main_boot = vec![vec![0u8; sector_len]; 12];
        let b = &mut main_boot[0];
        b[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
        b[3..11].copy_from_slice(b"EXFAT   ");
        put_u64(b, 64, geometry.partition_offset);
        put_u64(b, 72, geometry.sector_count);
        put_u32(b, 80, FAT_OFFSET as u32);
        put_u32(
            b,
            84,
            u32::try_from(fat_len).map_err(|_| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT FAT原生扇区数量超出u32",
                )
            })?,
        );
        put_u32(
            b,
            88,
            u32::try_from(heap).map_err(|_| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT数据堆起点超出u32",
                )
            })?,
        );
        put_u32(b, 92, cluster_count);
        put_u32(b, 96, root);
        put_u32(b, 100, serial);
        put_u16(b, 104, 0x0100);
        put_u16(b, 106, 0);
        b[108] = sector_size.trailing_zeros() as u8;
        b[109] = shift;
        b[110] = 1;
        b[111] = 0x80;
        b[112] = u8::try_from((u64::from(allocated) * 100).div_ceil(u64::from(cluster_count)))
            .unwrap_or(100)
            .min(100);
        b[510..512].copy_from_slice(&[0x55, 0xaa]);
        // The Extended Boot Signature is the last DWORD of every extended
        // native sector, not always offset 508 on 4Kn.
        for sec in &mut main_boot[1..=8] {
            sec[sector_len - 2..].copy_from_slice(&[0x55, 0xaa]);
        }
        let sum = exfat_boot_checksum_native(&main_boot);
        for chunk in main_boot[11].as_chunks_mut::<4>().0 {
            chunk.copy_from_slice(&sum.to_le_bytes());
        }
        for (i, sec) in main_boot.iter().enumerate() {
            sectors.insert(i as u64, sec.clone());
            sectors.insert(i as u64 + 12, sec.clone());
        }

        let fat_bytes_len =
            usize::try_from(fat_len.checked_mul(u64::from(sector_size)).ok_or_else(|| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT FAT字节长度溢出",
                )
            })?)
            .map_err(|_| {
                FilesystemError::for_filesystem(
                    self.kind(),
                    FilesystemErrorKind::InvalidGeometry,
                    "exFAT FAT过大",
                )
            })?;
        let mut fat = vec![0u8; fat_bytes_len];
        put_u32(&mut fat, 0, 0xffff_fff8);
        put_u32(&mut fat, 4, 0xffff_ffff);
        fat_chain(&mut fat, root, 1)?;
        fat_chain(&mut fat, bitmap, bitmap_clusters)?;
        fat_chain(&mut fat, upcase_cluster, upcase_clusters)?;
        for (i, chunk) in fat.chunks_exact(sector_len).enumerate() {
            sectors.insert(FAT_OFFSET + i as u64, chunk.to_vec());
        }

        let mut bitmap_bytes = vec![0u8; bitmap_len as usize];
        for bit in 0..allocated as usize {
            bitmap_bytes[bit / 8] |= 1 << (bit % 8);
        }
        put_native_stream(
            &mut sectors,
            heap,
            spc,
            bitmap,
            bitmap_clusters,
            &bitmap_bytes,
            sector_size,
        )?;
        put_native_stream(
            &mut sectors,
            heap,
            spc,
            upcase_cluster,
            upcase_clusters,
            &upcase,
            sector_size,
        )?;
        let mut root_bytes = vec![0u8; cluster_bytes as usize];
        root_bytes[0] = 0x81;
        root_bytes[1] = 0;
        put_u32(&mut root_bytes, 20, bitmap);
        put_u64(&mut root_bytes, 24, bitmap_len);
        root_bytes[32] = 0x82;
        put_u32(&mut root_bytes, 36, checksum32(&upcase));
        put_u32(&mut root_bytes, 52, upcase_cluster);
        put_u64(&mut root_bytes, 56, upcase.len() as u64);
        if !label.is_empty() {
            root_bytes[64] = 0x83;
            root_bytes[65] = label.len() as u8;
            for (i, v) in label.iter().enumerate() {
                put_u16(&mut root_bytes, 66 + i * 2, *v);
            }
        }
        put_native_stream(&mut sectors, heap, spc, root, 1, &root_bytes, sector_size)?;

        Ok(NativeFormatPlan {
            filesystem: self.kind(),
            geometry,
            writes: sectors
                .into_iter()
                .map(|(relative_lba, data)| NativeFilesystemWrite { relative_lba, data })
                .collect(),
            expected_metadata: FilesystemMetadata {
                kind: self.kind(),
                volume_label: request.volume_label.clone(),
                volume_serial: Some(serial),
            },
        })
    }
}

impl FilesystemDriver for ExFatDriver {
    fn kind(&self) -> FilesystemKind {
        FilesystemKind::ExFat
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
        if source.sector_size() != 512 {
            return Ok(DetectionResult::no_match(self.kind()));
        }
        let boot = source.read_sector(0)?;
        Ok(DetectionResult {
            kind: self.kind(),
            confidence: if parse_geometry(&boot, source.sector_count()).is_some() {
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
        if geometry.sector_size != 512
            || source.sector_size() != 512
            || source.sector_count() != geometry.sector_count
        {
            return Ok(false);
        }
        let boot = source.read_sector(0)?;
        let Some(g) = parse_geometry(&boot, geometry.sector_count) else {
            return Ok(false);
        };
        Ok(g.partition_offset == geometry.partition_offset
            && g.volume_length == geometry.sector_count)
    }
    fn read_metadata(
        &self,
        source: &mut dyn FilesystemReader,
    ) -> Result<FilesystemMetadata, FilesystemError> {
        let boot = source.read_sector(0)?;
        let g = parse_geometry(&boot, source.sector_count()).ok_or_else(|| {
            FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidBootSector,
                "exFAT 引导扇区校验失败",
            )
        })?;
        Ok(FilesystemMetadata {
            kind: self.kind(),
            volume_label: read_label(source, g)?,
            volume_serial: Some(g.volume_serial),
        })
    }
    fn validate_format_request(&self, request: &FormatRequest) -> Result<(), FilesystemError> {
        if request.filesystem != self.kind() {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "exFAT 驱动收到其他文件系统格式化请求",
            ));
        }
        validate_label(request.volume_label.as_deref()).map(|_| ())
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
                "exFAT 格式化需要卷序列号",
            )
        })?;
        Ok(FilesystemMetadata {
            kind: self.kind(),
            volume_label: request.volume_label.clone(),
            volume_serial: Some(volume_serial),
        })
    }
    fn build_format_plan(
        &self,
        geometry: FilesystemGeometry,
        request: &FormatRequest,
    ) -> Result<FormatPlan, FilesystemError> {
        // The physical provision writer is still a strict [u8;512] consumer.
        if geometry.sector_size != 512 {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "exFAT旧格式化写入器仅支持512B逻辑扇区",
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
                        "exFAT旧格式化块长度不符合512B",
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
        if !self.matches_geometry(source, geometry)? {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidGeometry,
                "exFAT 分区偏移或卷长度读回不一致",
            ));
        }
        let metadata = self.read_metadata(source)?;
        if expected.volume_serial.is_some() && metadata.volume_serial != expected.volume_serial {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidMetadata,
                "exFAT 卷序列号读回不一致",
            ));
        }
        if metadata.volume_label != expected.volume_label {
            return Err(FilesystemError::for_filesystem(
                self.kind(),
                FilesystemErrorKind::InvalidVolumeLabel,
                "exFAT 卷标读回不一致",
            ));
        }
        Ok(FormatVerification { metadata })
    }
}

pub(super) fn format_sector_count(volume_sectors: u64) -> Result<u64, FilesystemError> {
    let shift = choose_shift(volume_sectors)?;
    let spc = 1u64 << shift;
    let (fat, _, clusters) = layout(volume_sectors, shift).ok_or_else(|| {
        FilesystemError::for_filesystem(
            FilesystemKind::ExFat,
            FilesystemErrorKind::InvalidGeometry,
            "exFAT 几何计算失败",
        )
    })?;
    let bitmap = u64::from(clusters).div_ceil(8).div_ceil(spc * 512);
    let upcase = (exfat_upcase_table().len() as u64).div_ceil(spc * 512);
    Ok(24 + fat + (1 + bitmap + upcase) * spc)
}

#[cfg(test)]
mod geometry_tests {
    use super::{choose_shift, layout};
    use crate::filesystem::FilesystemErrorKind;

    fn first_volume_with_cluster_count(target: u32, shift: u8) -> u64 {
        let mut low = target as u64 * (1u64 << shift);
        let mut high = (target as u64 + 100_000) * (1u64 << shift);
        while low < high {
            let mid = low + (high - low) / 2;
            if layout(mid, shift).unwrap().2 < target {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low
    }

    #[test]
    fn validated_cluster_budget_accepts_limit_and_upgrades_limit_plus_one() {
        const LIMIT: u32 = crate::filesystem_capability::EXFAT_MAX_VALIDATED_CLUSTERS;
        let at_limit = first_volume_with_cluster_count(LIMIT, 7);
        let over_limit = first_volume_with_cluster_count(LIMIT + 1, 7);
        assert_eq!(layout(at_limit, 7).unwrap().2, LIMIT);
        assert_eq!(layout(over_limit, 7).unwrap().2, LIMIT + 1);
        assert_eq!(choose_shift(at_limit).unwrap(), 7);
        assert_eq!(choose_shift(over_limit).unwrap(), 8);
    }

    #[test]
    fn deterministic_geometry_sweep_stays_within_parser_capability() {
        const GIB: u64 = 2_097_152;
        let mut volumes = (1..=1024)
            .step_by(7)
            .map(|gib| gib * GIB)
            .collect::<Vec<_>>();
        for threshold in [64 * GIB, 256 * GIB, 512 * GIB] {
            volumes.extend([threshold - 1, threshold, threshold + 1]);
        }
        for volume in volumes {
            let shift = choose_shift(volume).unwrap();
            let (_, heap_offset, count) = layout(volume, shift).unwrap();
            assert!(count <= crate::filesystem_capability::EXFAT_MAX_VALIDATED_CLUSTERS);
            assert!(heap_offset + count as u64 * (1u64 << shift) <= volume);
            assert!(shift <= crate::filesystem_capability::EXFAT_MAX_VALIDATED_CLUSTER_SHIFT);
        }
    }

    #[test]
    fn unsupported_geometry_is_typed() {
        let error = choose_shift(u64::MAX).unwrap_err();
        assert_eq!(error.kind, FilesystemErrorKind::InvalidGeometry);
        assert!(error.message.contains("无法为"));
    }
}
