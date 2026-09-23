//! Pure filesystem construction for first-party provisioning profiles.
//!
//! The current first-party Windows writer reads the GLOBAL/fType setting from
//! usbtoolCfg.ini, defaults it to exfat, normalizes ntfs/exfat/fat32, and
//! passes that format name to fmifs FormatEx. This module models that axis
//! separately from EDP partition type and provides the portable exFAT builder
//! used by the default profile.

use std::collections::BTreeMap;

use crate::{
    backup_deep::keys::sm4_encrypt_block, crypto::crc32_bare, protocol::edpf::EdpPartitionType,
};

use super::{
    layout::{OfficialPartitionGeometry, OfficialProvisionPlan},
    FileKeyWrapMode, OfficialPartitionMode,
};

const SECTOR_SIZE: usize = 512;
const BOOT_REGION_SECTORS: u64 = 24;
const FAT_OFFSET: u64 = BOOT_REGION_SECTORS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OfficialFilesystemFormat {
    ExFat,
    Ntfs,
    Fat32,
}

impl OfficialFilesystemFormat {
    pub const fn first_party_default() -> Self {
        Self::ExFat
    }

    pub fn from_first_party_config(value: Option<&str>) -> Self {
        match value
            .unwrap_or("exfat")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "ntfs" => Self::Ntfs,
            "fat32" => Self::Fat32,
            "exfat" => Self::ExFat,
            _ => Self::ExFat,
        }
    }

    pub const fn config_token(self) -> &'static str {
        match self {
            Self::ExFat => "exfat",
            Self::Ntfs => "ntfs",
            Self::Fat32 => "fat32",
        }
    }

    pub const fn windows_format_name(self) -> &'static str {
        match self {
            Self::ExFat => "exFat",
            Self::Ntfs => "NTFS",
            Self::Fat32 => "fat32",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseFilesystemImage {
    volume_sectors: u64,
    sectors: BTreeMap<u64, [u8; SECTOR_SIZE]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionFilesystemImage {
    pub geometry: OfficialPartitionGeometry,
    pub physically_encrypted: bool,
    pub image: SparseFilesystemImage,
}

impl SparseFilesystemImage {
    pub fn volume_sectors(&self) -> u64 {
        self.volume_sectors
    }

    pub fn sectors(&self) -> &BTreeMap<u64, [u8; SECTOR_SIZE]> {
        &self.sectors
    }

    pub fn sector_or_zero(&self, relative_lba: u64) -> Option<[u8; SECTOR_SIZE]> {
        if relative_lba >= self.volume_sectors {
            return None;
        }
        Some(
            self.sectors
                .get(&relative_lba)
                .copied()
                .unwrap_or([0; SECTOR_SIZE]),
        )
    }

    pub fn metadata_bytes(&self) -> usize {
        self.sectors.len() * SECTOR_SIZE
    }
}

fn put_u16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    if alignment == 0 {
        return None;
    }
    value
        .checked_add(alignment - 1)
        .map(|v| v / alignment * alignment)
}

fn choose_cluster_shift(volume_sectors: u64) -> u8 {
    const GIB_SECTORS: u64 = 1024 * 1024 * 1024 / 512;
    const MIB_SECTORS: u64 = 1024 * 1024 / 512;
    if volume_sectors >= 64 * GIB_SECTORS {
        7
    } else if volume_sectors >= GIB_SECTORS {
        6
    } else if volume_sectors >= 64 * MIB_SECTORS {
        4
    } else {
        3
    }
}

fn exfat_geometry(volume_sectors: u64, cluster_shift: u8) -> Result<(u64, u64, u32), String> {
    if volume_sectors <= BOOT_REGION_SECTORS {
        return Err("exFAT volume is too small for its boot region".into());
    }
    let sectors_per_cluster = 1u64
        .checked_shl(cluster_shift.into())
        .ok_or("invalid exFAT cluster shift")?;
    let mut cluster_count = (volume_sectors - BOOT_REGION_SECTORS) / sectors_per_cluster;
    for _ in 0..16 {
        if cluster_count == 0 || cluster_count > u32::MAX as u64 - 2 {
            return Err("exFAT cluster count is out of range".into());
        }
        let fat_bytes = (cluster_count + 2)
            .checked_mul(4)
            .ok_or("exFAT FAT size overflow")?;
        let fat_length = fat_bytes.div_ceil(SECTOR_SIZE as u64);
        let heap_offset = align_up(
            FAT_OFFSET
                .checked_add(fat_length)
                .ok_or("exFAT heap offset overflow")?,
            sectors_per_cluster,
        )
        .ok_or("exFAT heap alignment overflow")?;
        if heap_offset >= volume_sectors {
            return Err("exFAT volume is too small for FAT and cluster heap".into());
        }
        let next = (volume_sectors - heap_offset) / sectors_per_cluster;
        if next == cluster_count {
            return Ok((fat_length, heap_offset, cluster_count as u32));
        }
        cluster_count = next;
    }
    Err("exFAT geometry did not converge".into())
}

fn exfat_boot_checksum(sectors: &[[u8; SECTOR_SIZE]]) -> u32 {
    let mut checksum = 0u32;
    for (sector_index, sector) in sectors.iter().take(11).enumerate() {
        for (offset, &byte) in sector.iter().enumerate() {
            if sector_index == 0 && matches!(offset, 106 | 107 | 112) {
                continue;
            }
            checksum = checksum.rotate_right(1).wrapping_add(byte as u32);
        }
    }
    checksum
}

fn upcase_mapping(code: u16) -> u16 {
    if (b'a' as u16..=b'z' as u16).contains(&code) {
        code - 0x20
    } else {
        code
    }
}

fn exfat_upcase_table() -> Vec<u8> {
    let mut words = Vec::<u16>::new();
    let mut code = 0u32;
    while code <= u16::MAX as u32 {
        let current = code as u16;
        if upcase_mapping(current) == current {
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
            words.push(upcase_mapping(current));
            code += 1;
        }
    }
    words
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

fn checksum32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |sum, &byte| {
        sum.rotate_right(1).wrapping_add(byte as u32)
    })
}

fn fat_chain(fat: &mut [u8], first_cluster: u32, count: u32) -> Result<(), String> {
    if count == 0 {
        return Err("exFAT metadata stream has zero clusters".into());
    }
    for offset in 0..count {
        let cluster = first_cluster
            .checked_add(offset)
            .ok_or("exFAT metadata cluster overflow")?;
        let next = if offset + 1 == count {
            0xffff_ffff
        } else {
            cluster + 1
        };
        let at = cluster as usize * 4;
        if at + 4 > fat.len() {
            return Err("exFAT metadata chain exceeds FAT".into());
        }
        put_u32(fat, at, next);
    }
    Ok(())
}

fn put_stream(
    sectors: &mut BTreeMap<u64, [u8; SECTOR_SIZE]>,
    heap_offset: u64,
    sectors_per_cluster: u64,
    first_cluster: u32,
    cluster_count: u32,
    bytes: &[u8],
) -> Result<(), String> {
    let capacity = cluster_count as u64 * sectors_per_cluster * SECTOR_SIZE as u64;
    if bytes.len() as u64 > capacity {
        return Err("exFAT metadata stream exceeds allocated clusters".into());
    }
    let first_lba = heap_offset
        .checked_add((first_cluster as u64 - 2) * sectors_per_cluster)
        .ok_or("exFAT metadata LBA overflow")?;
    for sector_index in 0..cluster_count as u64 * sectors_per_cluster {
        let mut sector = [0u8; SECTOR_SIZE];
        let start = sector_index as usize * SECTOR_SIZE;
        if start < bytes.len() {
            let end = (start + SECTOR_SIZE).min(bytes.len());
            sector[..end - start].copy_from_slice(&bytes[start..end]);
        }
        sectors.insert(first_lba + sector_index, sector);
    }
    Ok(())
}

pub fn build_empty_exfat(
    partition_offset_lba: u64,
    volume_sectors: u64,
    volume_serial: u32,
    label: &str,
) -> Result<SparseFilesystemImage, String> {
    let label_utf16 = label.encode_utf16().collect::<Vec<_>>();
    if label_utf16.len() > 11 {
        return Err("exFAT volume label exceeds 11 UTF-16 code units".into());
    }
    let cluster_shift = choose_cluster_shift(volume_sectors);
    let sectors_per_cluster = 1u64 << cluster_shift;
    let (fat_length, heap_offset, cluster_count) = exfat_geometry(volume_sectors, cluster_shift)?;
    if cluster_count > 4_194_304 {
        return Err("exFAT cluster count exceeds edpcli validated parser range".into());
    }

    let cluster_bytes = sectors_per_cluster * SECTOR_SIZE as u64;
    let bitmap_len = (cluster_count as u64).div_ceil(8);
    let bitmap_clusters = bitmap_len.div_ceil(cluster_bytes) as u32;
    let upcase = exfat_upcase_table();
    let upcase_clusters = (upcase.len() as u64).div_ceil(cluster_bytes) as u32;
    let root_cluster = 2u32;
    let bitmap_cluster = 3u32;
    let upcase_cluster = bitmap_cluster
        .checked_add(bitmap_clusters)
        .ok_or("exFAT metadata cluster overflow")?;
    let allocated_clusters = 1u32
        .checked_add(bitmap_clusters)
        .and_then(|v| v.checked_add(upcase_clusters))
        .ok_or("exFAT allocated-cluster count overflow")?;
    if allocated_clusters > cluster_count {
        return Err("exFAT volume is too small for system metadata".into());
    }

    let mut sectors = BTreeMap::<u64, [u8; SECTOR_SIZE]>::new();
    let mut main_boot = [[0u8; SECTOR_SIZE]; 12];
    let boot = &mut main_boot[0];
    boot[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    put_u64(boot, 64, partition_offset_lba);
    put_u64(boot, 72, volume_sectors);
    put_u32(boot, 80, FAT_OFFSET as u32);
    put_u32(
        boot,
        84,
        u32::try_from(fat_length).map_err(|_| "exFAT FAT length exceeds u32")?,
    );
    put_u32(
        boot,
        88,
        u32::try_from(heap_offset).map_err(|_| "exFAT heap offset exceeds u32")?,
    );
    put_u32(boot, 92, cluster_count);
    put_u32(boot, 96, root_cluster);
    put_u32(boot, 100, volume_serial);
    put_u16(boot, 104, 0x0100);
    put_u16(boot, 106, 0);
    boot[108] = 9;
    boot[109] = cluster_shift;
    boot[110] = 1;
    boot[111] = 0x80;
    boot[112] = u8::try_from((allocated_clusters as u64 * 100).div_ceil(cluster_count as u64))
        .unwrap_or(100)
        .min(100);
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    for sector in &mut main_boot[1..=8] {
        sector[510..512].copy_from_slice(&[0x55, 0xaa]);
    }
    let boot_checksum = exfat_boot_checksum(&main_boot);
    for chunk in main_boot[11].as_chunks_mut::<4>().0 {
        chunk.copy_from_slice(&boot_checksum.to_le_bytes());
    }
    for (index, sector) in main_boot.iter().enumerate() {
        sectors.insert(index as u64, *sector);
        sectors.insert(index as u64 + 12, *sector);
    }

    let fat_bytes_len = usize::try_from(
        fat_length
            .checked_mul(SECTOR_SIZE as u64)
            .ok_or("exFAT FAT byte length overflow")?,
    )
    .map_err(|_| "exFAT FAT is too large for this process")?;
    let mut fat = vec![0u8; fat_bytes_len];
    put_u32(&mut fat, 0, 0xffff_fff8);
    put_u32(&mut fat, 4, 0xffff_ffff);
    fat_chain(&mut fat, root_cluster, 1)?;
    fat_chain(&mut fat, bitmap_cluster, bitmap_clusters)?;
    fat_chain(&mut fat, upcase_cluster, upcase_clusters)?;
    for (index, chunk) in fat.as_chunks::<SECTOR_SIZE>().0.iter().enumerate() {
        let mut sector = [0u8; SECTOR_SIZE];
        sector.copy_from_slice(chunk);
        sectors.insert(FAT_OFFSET + index as u64, sector);
    }

    let mut bitmap = vec![0u8; bitmap_len as usize];
    for bit in 0..allocated_clusters as usize {
        bitmap[bit / 8] |= 1 << (bit % 8);
    }
    put_stream(
        &mut sectors,
        heap_offset,
        sectors_per_cluster,
        bitmap_cluster,
        bitmap_clusters,
        &bitmap,
    )?;
    put_stream(
        &mut sectors,
        heap_offset,
        sectors_per_cluster,
        upcase_cluster,
        upcase_clusters,
        &upcase,
    )?;

    let mut root = vec![0u8; cluster_bytes as usize];
    root[0] = 0x81;
    root[1] = 0;
    put_u32(&mut root, 20, bitmap_cluster);
    put_u64(&mut root, 24, bitmap_len);
    root[32] = 0x82;
    put_u32(&mut root, 36, checksum32(&upcase));
    put_u32(&mut root, 52, upcase_cluster);
    put_u64(&mut root, 56, upcase.len() as u64);
    if !label_utf16.is_empty() {
        root[64] = 0x83;
        root[65] = label_utf16.len() as u8;
        for (index, value) in label_utf16.iter().enumerate() {
            put_u16(&mut root, 66 + index * 2, *value);
        }
    }
    put_stream(
        &mut sectors,
        heap_offset,
        sectors_per_cluster,
        root_cluster,
        1,
        &root,
    )?;

    Ok(SparseFilesystemImage {
        volume_sectors,
        sectors,
    })
}

pub fn encrypt_sparse_mode2(
    image: &SparseFilesystemImage,
    file_key: &[u8; 16],
) -> SparseFilesystemImage {
    let sectors = image
        .sectors
        .iter()
        .map(|(&lba, sector)| {
            let mut encrypted = [0u8; SECTOR_SIZE];
            for (source, target) in sector
                .as_chunks::<16>()
                .0
                .iter()
                .zip(encrypted.as_chunks_mut::<16>().0.iter_mut())
            {
                target.copy_from_slice(&sm4_encrypt_block(source, file_key));
            }
            (lba, encrypted)
        })
        .collect();
    SparseFilesystemImage {
        volume_sectors: image.volume_sectors,
        sectors,
    }
}

fn physical_partition_encryption(
    mode: OfficialPartitionMode,
    index: usize,
    partition_type: EdpPartitionType,
) -> bool {
    match partition_type {
        EdpPartitionType::Boot => false,
        EdpPartitionType::Share
            if mode == OfficialPartitionMode::BootShareCombined && index == 0 =>
        {
            false
        }
        EdpPartitionType::Share | EdpPartitionType::Encrypt => true,
    }
}

fn is_whole_disk_compatibility_reserve(
    mode: OfficialPartitionMode,
    partition: OfficialPartitionGeometry,
) -> bool {
    mode == OfficialPartitionMode::WholeDiskEncrypted
        && partition.partition_type == EdpPartitionType::Boot
        && partition.size_bytes == super::WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES
}

/// Construct the default first-party exFAT filesystem stage for all logical
/// partitions that carry a filesystem.
///
/// The whole-disk-encrypted mode's 0x7E00 type1 compatibility entry is not a
/// filesystem. The mode1 front type2 is physically plaintext because it is
/// exposed directly by the MBR; all other type2/type4 filesystem images use
/// the verified current mode2 SM4 sector transform.
pub fn build_official_exfat_partitions(
    plan: &OfficialProvisionPlan,
    file_key: &[u8; 16],
    volume_label: &str,
    volume_serials: &[u32],
) -> Result<Vec<PartitionFilesystemImage>, String> {
    if plan.filesystem_format != OfficialFilesystemFormat::ExFat {
        return Err(format!(
            "portable filesystem writer does not yet implement {}",
            plan.filesystem_format.config_token()
        ));
    }
    if crc32_bare(file_key) != plan.lba12_key_material.file_key_crc {
        return Err("filesystem file key does not match LBA12 FileKeyCRC".into());
    }
    let logical = plan.logical_partitions(SECTOR_SIZE as u64)?;
    if volume_serials.len() != logical.len() {
        return Err(format!(
            "filesystem volume serial count mismatch: got {}, need {}",
            volume_serials.len(),
            logical.len()
        ));
    }
    if logical.iter().enumerate().any(|(index, partition)| {
        !is_whole_disk_compatibility_reserve(plan.mode, *partition)
            && physical_partition_encryption(plan.mode, index, partition.partition_type)
    }) && plan.lba12_key_material.encrypt_mode != FileKeyWrapMode::Sm4
    {
        return Err(
            "portable encrypted filesystem writer is validated only for current mode2 SM4".into(),
        );
    }

    let mut out = Vec::with_capacity(logical.len());
    for (index, partition) in logical.into_iter().enumerate() {
        if is_whole_disk_compatibility_reserve(plan.mode, partition) {
            continue;
        }
        let plain = build_empty_exfat(
            partition.start_sector,
            partition.sector_count(),
            volume_serials[index],
            volume_label,
        )?;
        let physically_encrypted =
            physical_partition_encryption(plan.mode, index, partition.partition_type);
        let image = if physically_encrypted {
            encrypt_sparse_mode2(&plain, file_key)
        } else {
            plain
        };
        out.push(PartitionFilesystemImage {
            geometry: partition,
            physically_encrypted,
            image,
        });
    }
    Ok(out)
}
