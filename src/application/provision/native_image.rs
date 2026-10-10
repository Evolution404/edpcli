//! Offline-only native-sector Plain image authoring. No USB discovery, raw
//! device handle, mount, provision commit, or physical write authority.
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::{PlainPartitionRequest, PlainPartitionSize};
use crate::diskio::{execute_native_transaction, NativeBlockDevice};
use crate::filesystem::{
    FilesystemGeometry, FilesystemKind, FormatRequest, NativeFormatPlan, NativeVirtualDiskPlan,
    EXFAT_DRIVER, FAT12_DRIVER, FAT16_DRIVER, FAT32_DRIVER,
};

fn checked_size(size: PlainPartitionSize, sector_bytes: u32) -> Result<Option<u64>, String> {
    let bytes = u64::from(sector_bytes);
    match size {
        PlainPartitionSize::Sectors(n) => Ok(Some(n)),
        PlainPartitionSize::MiB(n) | PlainPartitionSize::GiB(n) => {
            let unit = if matches!(size, PlainPartitionSize::MiB(_)) {
                1024u64 * 1024
            } else {
                1024u64 * 1024 * 1024
            };
            n.checked_mul(unit)
                .ok_or_else(|| "容量换算溢出".to_string())
                .and_then(|v| {
                    (v % bytes == 0)
                        .then_some(Some(v / bytes))
                        .ok_or_else(|| "容量不是完整原生逻辑扇区的整数倍".to_string())
                })
        }
        PlainPartitionSize::Fill => Ok(None),
    }
}

pub fn plan_native_plain_image(
    total_sectors: u64,
    sector_bytes: u32,
    partitions: &[PlainPartitionRequest],
) -> Result<NativeVirtualDiskPlan, String> {
    // 512B and 4Kn are the currently independently verified filesystem sizes.
    // Geometry on a given device is immutable for its entire transaction.
    if !matches!(sector_bytes, 512 | 4096) || total_sectors <= 2048 {
        return Err("仅支持已验证的512B/4096B原生整盘几何".into());
    }
    let default = [PlainPartitionRequest {
        start_lba: 2048,
        size: PlainPartitionSize::Fill,
        filesystem: FilesystemKind::ExFat,
        volume_label: "EDP-PLAIN".into(),
    }];
    let partitions = if partitions.is_empty() {
        &default[..]
    } else {
        partitions
    };
    if partitions.len() > 4 {
        return Err("MBR最多只能包含4个主分区".into());
    }
    let mut plans: Vec<NativeFormatPlan> = Vec::with_capacity(partitions.len());
    for partition in partitions {
        let start = partition.start_lba;
        let remaining = total_sectors
            .checked_sub(start)
            .filter(|v| *v > 0)
            .ok_or("原生分区起点超出整盘范围")?;
        let count = checked_size(partition.size, sector_bytes)?.unwrap_or(remaining);
        if count == 0 || count > remaining {
            return Err("原生分区容量超出整盘边界".into());
        }
        let geometry = FilesystemGeometry::new(start, count, sector_bytes);
        let request = FormatRequest {
            filesystem: partition.filesystem,
            volume_label: Some(partition.volume_label.clone()),
            volume_serial: Some(0xED50_0000 + plans.len() as u32),
        };
        let plan = match partition.filesystem {
            FilesystemKind::Fat12 => FAT12_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::Fat16 => FAT16_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::Fat32 => FAT32_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::ExFat => EXFAT_DRIVER.build_native_format_plan(geometry, &request),
            _ => return Err("离线原生镜像暂不支持格式化此文件系统".into()),
        }
        .map_err(|error| {
            format!(
                "{}原生格式化计划失败: {error}",
                partition.filesystem.label()
            )
        })?;
        plans.push(plan);
    }
    NativeVirtualDiskPlan::assemble(total_sectors, sector_bytes, &plans)
        .map_err(|error| format!("原生整盘镜像规划失败: {error}"))
}

/// Creates a brand-new sparse *ordinary file* image, verifies each complete
/// native block, and commits MBR LBA0 last. Refuses existing paths, symlinks,
/// device paths, nonregular files and unsupported geometry. On error removes
/// only the newly created file. This API cannot provision an existing USB disk.
/// A regular file created by this call, with explicit complete native blocks.
/// No raw-device path or pre-existing image can enter this adapter.
struct NewImageBlockDevice<'a> {
    file: &'a mut fs::File,
    total_sectors: u64,
    sector_bytes: u32,
}

impl NewImageBlockDevice<'_> {
    fn byte_offset(&self, lba: u64) -> std::io::Result<u64> {
        if lba >= self.total_sectors {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "LBA超出新镜像边界",
            ));
        }
        lba.checked_mul(u64::from(self.sector_bytes))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "LBA字节偏移溢出"))
    }
}

impl NativeBlockDevice for NewImageBlockDevice<'_> {
    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
    fn sector_bytes(&self) -> u32 {
        self.sector_bytes
    }
    fn read_block(&mut self, lba: u64) -> std::io::Result<Vec<u8>> {
        let offset = self.byte_offset(lba)?;
        let mut block = vec![0u8; self.sector_bytes as usize];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut block)?;
        Ok(block)
    }
    fn write_block(&mut self, lba: u64, full_block: &[u8]) -> std::io::Result<()> {
        if full_block.len() != self.sector_bytes as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "非完整原生块写入",
            ));
        }
        let offset = self.byte_offset(lba)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(full_block)
    }
    fn sync_blocks(&mut self) -> std::io::Result<()> {
        self.file.sync_all()
    }
}

/// Write an already planned virtual image using full native blocks, with
/// preflight snapshots, MBR-last commit and independent double readback.
/// Only a caller-owned ordinary file may enter this adapter, not a device node.
pub(crate) fn write_native_virtual_plan(
    file: &mut fs::File,
    plan: &NativeVirtualDiskPlan,
) -> Result<(), String> {
    let byte_len = plan
        .total_sectors
        .checked_mul(u64::from(plan.sector_bytes))
        .ok_or("虚拟整盘字节长度溢出")?;
    if !matches!(plan.sector_bytes, 512 | 4096) || byte_len == 0 {
        return Err("不支持的原生镜像逻辑扇区几何".into());
    }
    if !file.metadata().is_ok_and(|m| m.file_type().is_file()) {
        return Err("目标不是普通文件".into());
    }
    file.set_len(byte_len)
        .map_err(|e| format!("无法设置镜像容量: {e}"))?;
    let mut device = NewImageBlockDevice {
        file,
        total_sectors: plan.total_sectors,
        sector_bytes: plan.sector_bytes,
    };
    execute_native_transaction(&mut device, plan).map_err(|e| format!("镜像原生事务失败: {e}"))
}

/// Build a genuine offline 512B EDP target from the same protocol producer
/// and filesystem/key-domain formatter as the prepared physical workflow.
///
/// The caller supplies a *synthetic* verified identity, per-partition FileKeys,
/// serials and a 512B official layout. No USB probe, mount or raw-device handle
/// is used. The returned plan can only be exported to an ordinary image file;
/// it is NOT manufacturer certification for 4Kn or physical writes.
pub fn plan_native_edp_512_image(
    spec: &crate::provision::ProvisionSpec,
    entropy: &crate::provision::ProvisionEntropy,
    plan: &crate::provision::OfficialProvisionPlan,
    options: &super::FormatOptions,
    serials: &[u32],
    file_keys: &[[u8; 16]],
) -> Result<NativeVirtualDiskPlan, String> {
    let protocol = crate::provision::build_official_provision_protocol_image(spec, entropy, plan)?;
    let format_choices =
        super::format_plan::plan_format_targets_with_keys(plan, options, serials, file_keys)
            .map_err(|error| error.to_string())?;
    super::export::assemble_official_virtual_plan(&protocol, &format_choices)
        .map_err(|error| error.msg)
}

/// Author the independently produced EDP layout to a *new* ordinary file.
/// The native transaction performs a sync, MBR-last commit and double readback.
pub fn export_native_edp_512_image(
    path: &Path,
    spec: &crate::provision::ProvisionSpec,
    entropy: &crate::provision::ProvisionEntropy,
    plan: &crate::provision::OfficialProvisionPlan,
    options: &super::FormatOptions,
    serials: &[u32],
    file_keys: &[[u8; 16]],
) -> Result<(), String> {
    let native = plan_native_edp_512_image(spec, entropy, plan, options, serials, file_keys)?;
    export_native_plain_image(path, &native)
}

/// Compose a NEW native 4Kn EDP disk with all 13 protocol blocks, native LCE,
/// selected FAT/exFAT filesystem metadata and FileKeyCRC-checked ciphertext.
/// This is an OFFLINE virtual candidate; it does not certify OEM 4Kn tails or
/// grant permission to write USB hardware. For legacy 512B use the existing
/// exact gold-backed producer above rather than a separate implementation.
pub fn plan_native_edp_4kn_image(
    spec: &crate::provision::ProvisionSpec,
    entropy: &crate::provision::ProvisionEntropy,
    plan: &crate::provision::OfficialProvisionPlan,
    options: &super::FormatOptions,
    serials: &[u32],
    file_keys: &[[u8; 16]],
) -> Result<NativeVirtualDiskPlan, String> {
    use crate::filesystem::NativeFilesystemWrite;
    use crate::partition_transform::{
        transform_native_sector_offline, NativeCipherDirection, NativePartitionDataCipher,
    };
    use crate::protocol::crypto::crc32_bare;
    use crate::provision::{NativeEdpLayoutPlan, TargetPartitionGeometry};
    use std::collections::BTreeMap;

    const NATIVE: u32 = 4096;
    let total = spec.target().total_sectors();
    let targets = plan.format_targets_native(NATIVE)?;
    if targets.len() != serials.len() || targets.len() != file_keys.len() {
        return Err("原生4Kn制盘格式化目标、serial和FileKey数量不一致".into());
    }
    if plan.filesystems != options.filesystems() {
        return Err("4Kn格式化选项与目标分区文件系统配置不一致".into());
    }
    for (requested, role) in [
        (options.boot, crate::provision::PartitionRole::Boot),
        (options.share, crate::provision::PartitionRole::Share),
        (options.encrypt, crate::provision::PartitionRole::Encrypt),
    ] {
        if requested
            && !targets.iter().any(|part| {
                part.format_capable
                    && (part.role == role
                        || (role == crate::provision::PartitionRole::Share
                            && part.role == crate::provision::PartitionRole::BootShareCombined))
            })
        {
            return Err(format!("4Kn目标模式不包含请求格式化的{}分区", role.label()));
        }
    }
    let mut resources = crate::filesystem::FormatResourceEstimate::from_sectors(0)
        .map_err(|error| error.to_string())?;
    for part in targets.iter().filter(|part| options.choice(part.role).0) {
        let filesystem = part.filesystem.ok_or("4Kn格式化分区缺少文件系统")?;
        let estimated =
            crate::filesystem::estimate_format_resources(filesystem, part.geometry.sector_count())
                .map_err(|error| error.to_string())?;
        resources = resources
            .checked_add(estimated)
            .map_err(|error| error.to_string())?;
    }
    crate::filesystem::FormatResourceBudget::default()
        .check(resources)
        .map_err(|error| error.to_string())?;

    let geometries = targets
        .iter()
        .map(|target| TargetPartitionGeometry {
            role: target.role,
            partition_type: target.geometry.partition_type,
            start_lba: target.geometry.start_sector,
            sector_count: target.geometry.sector_count(),
            physically_encrypted: target.physically_encrypted,
            filesystem: target.filesystem,
        })
        .collect::<Vec<_>>();
    // The same native geometry validator gates fresh 4Kn authoring, source
    // replay, mode semantics and LCE collision/overflow prevention.
    let layout = NativeEdpLayoutPlan::from_confirmed_geometry(
        plan.mode,
        total,
        NATIVE,
        &geometries,
        plan.lba7_compatibility_extent.start_lba,
        plan.lba7_compatibility_extent.size_sectors,
    )?;
    if layout.visible_mbr_type
        != crate::provision::visible_mbr_partition_type(
            plan.mode,
            targets[0].filesystem.unwrap_or(FilesystemKind::Fat16),
        )
    {
        return Err("4Kn可见分区类型不一致".into());
    }
    let native = crate::provision::generate_official_native_image(spec, entropy, plan, NATIVE)?;
    let parsed = crate::provision::parse_existing_provision_native(
        &native,
        spec.target().device_id(),
        total,
    )?
    .ok_or("4Kn新协议无法通过统一EDPF解码器")?;
    if parsed.profile.source_mode != plan.mode
        || parsed.profile.partitions.len() != targets.len()
        || parsed
            .profile
            .partitions
            .iter()
            .zip(&geometries)
            .any(|(actual, wanted)| {
                actual.start_lba != wanted.start_lba
                    || actual.sector_count != wanted.sector_count
                    || actual.partition_type != wanted.partition_type
            })
    {
        return Err("4Kn新盘协议回解析模式或分区原生几何不一致".into());
    }

    let mut writes = BTreeMap::<u64, Vec<u8>>::new();
    for lba in 0..13 {
        writes.insert(
            lba,
            native
                .block(lba as usize)
                .ok_or("缺少完整原生协议块")?
                .to_vec(),
        );
    }
    let lce =
        crate::provision::build_native_lce_ciphertext(plan.lba7_compatibility_extent, NATIVE)?;
    if writes.insert(layout.lce.start_lba, lce).is_some() {
        return Err("LCE与协议冲突".into());
    }
    for (index, target) in targets.iter().enumerate() {
        let (selected, label) = options.choice(target.role);
        if !selected {
            continue;
        }
        if !target.format_capable {
            return Err("兼容预留区域禁止格式化".into());
        }
        let filesystem = target.filesystem.ok_or("格式化目标未指定文件系统")?;
        crate::filesystem::validate_writable_filesystem(filesystem)
            .map_err(|error| error.to_string())?;
        let geometry = FilesystemGeometry::new(
            target.geometry.start_sector,
            target.geometry.sector_count(),
            NATIVE,
        );
        let request = FormatRequest {
            filesystem,
            volume_label: Some(label.to_string()),
            volume_serial: Some(serials[index]),
        };
        let format = match filesystem {
            FilesystemKind::Fat12 => FAT12_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::Fat16 => FAT16_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::Fat32 => FAT32_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::ExFat => EXFAT_DRIVER.build_native_format_plan(geometry, &request),
            _ => return Err("4Kn离线EDP格式化不支持该文件系统".into()),
        }
        .map_err(|error| format!("4Kn {}格式化失败: {error}", target.role.label()))?;
        let cipher = if target.physically_encrypted {
            let material = plan.partition_lba12_material[index].unwrap_or(plan.lba12_key_material);
            if crc32_bare(&file_keys[index]) != material.file_key_crc {
                return Err(format!("4Kn slot{index} FileKeyCRC不匹配"));
            }
            Some(NativePartitionDataCipher::from_encrypt_mode(
                material.encrypt_mode.raw(),
            )?)
        } else {
            None
        };
        for write in &format.writes {
            let lba = target
                .geometry
                .start_sector
                .checked_add(write.relative_lba)
                .ok_or("4Kn格式化目标LBA溢出")?;
            if write.relative_lba >= target.geometry.sector_count()
                || write.data.len() != NATIVE as usize
            {
                return Err("4Kn文件系统元数据未对齐完整原生块".into());
            }
            let data = match cipher {
                Some(cipher) => transform_native_sector_offline(
                    cipher,
                    NativeCipherDirection::Encrypt,
                    &write.data,
                    &file_keys[index],
                    lba,
                    NATIVE,
                )?,
                None => write.data.clone(),
            };
            if writes.insert(lba, data).is_some() {
                return Err(format!(
                    "4Kn格式化元数据与协议、LCE或其他分区重叠于LBA{lba}"
                ));
            }
        }
    }
    let mbr = writes.remove(&0).ok_or("4Kn虚拟盘没有MBR")?;
    let mut block_writes = writes
        .into_iter()
        .map(|(relative_lba, data)| NativeFilesystemWrite { relative_lba, data })
        .collect::<Vec<_>>();
    block_writes.push(NativeFilesystemWrite {
        relative_lba: 0,
        data: mbr,
    });
    Ok(NativeVirtualDiskPlan {
        total_sectors: total,
        sector_bytes: NATIVE,
        writes: block_writes,
    })
}

/// Like 512B export, the 4Kn candidate can only create a *new ordinary file*.
/// It always uses full-block transaction verification and MBR-last commit.
pub fn export_native_edp_4kn_image(
    path: &Path,
    spec: &crate::provision::ProvisionSpec,
    entropy: &crate::provision::ProvisionEntropy,
    plan: &crate::provision::OfficialProvisionPlan,
    options: &super::FormatOptions,
    serials: &[u32],
    file_keys: &[[u8; 16]],
) -> Result<(), String> {
    let native = plan_native_edp_4kn_image(spec, entropy, plan, options, serials, file_keys)?;
    export_native_plain_image(path, &native)
}

// Bridge a caller-owned read-only native-sector callback into the application's
// EvidenceSource contract. An independent native protocol snapshot is required
// at every public replay call, so the source cannot silently change between
// preview and LCE acquisition.
struct OfflineNativeSourceReader<F> {
    native_sector_bytes: u32,
    read_native: F,
}

impl<F> crate::application::evidence::SectorReader for OfflineNativeSourceReader<F>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    fn logical_sector_bytes(&self) -> u32 {
        self.native_sector_bytes
    }

    fn read_native_sector(&mut self, lba: u64) -> std::io::Result<Vec<u8>> {
        (self.read_native)(lba).map_err(std::io::Error::other)
    }

    fn read_sector(&mut self, lba: u64) -> std::io::Result<Vec<u8>> {
        let block = self.read_native_sector(lba)?;
        if block.len() < 512 {
            return Err(std::io::Error::other("离线来源原生扇区不足512B"));
        }
        Ok(block[..512].to_vec())
    }
}

/// Replay source-authenticated native EDP protocol and LCE blocks into a
/// sparse *virtual* write plan. Both 512B and 4Kn inputs retain all complete
/// native block bytes, including unknown protocol tails and LCE suffixes.
///
/// This is deliberately source replay, NOT new 4Kn OEM protocol generation.
/// A separately captured native protocol snapshot must match every read.
/// One read-only callback supplies both metadata and LCE, but no physical
/// write permission is ever provided.
pub fn plan_native_edp_source_replay_image<F>(
    layout: &crate::provision::NativeEdpLayoutPlan,
    source_protocol: &crate::protocol::image::NativeProtocolImage,
    device_id: &str,
    source_total_sectors: u64,
    read_native: F,
) -> Result<NativeVirtualDiskPlan, String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    let mut source_reader = OfflineNativeSourceReader {
        native_sector_bytes: layout.logical_sector_bytes,
        read_native,
    };
    let writes = crate::application::evidence::verified_native_source_replay(
        &mut source_reader,
        layout,
        source_protocol,
        device_id,
        source_total_sectors,
    )?;
    if !matches!(layout.logical_sector_bytes, 512 | 4096)
        || writes.last().is_none_or(|block| block.relative_lba != 0)
        || writes.len() != 13 + layout.lce.sector_count as usize
    {
        return Err("来源EDP重放不是完整原生块、完整LCE或MBR最后提交".into());
    }
    Ok(NativeVirtualDiskPlan {
        total_sectors: layout.total_sectors,
        sector_bytes: layout.logical_sector_bytes,
        writes,
    })
}

/// Export a verified source replay into a *new regular file* with the shared
/// native rollback/readback transaction. The result contains only original
/// protocol/LCE blocks; no source partition user data or mountability is
/// claimed. Physical media writers and device paths remain unavailable.
pub fn export_native_edp_source_replay_image<F>(
    path: &Path,
    layout: &crate::provision::NativeEdpLayoutPlan,
    source_protocol: &crate::protocol::image::NativeProtocolImage,
    device_id: &str,
    source_total_sectors: u64,
    read_native: F,
) -> Result<(), String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    let plan = plan_native_edp_source_replay_image(
        layout,
        source_protocol,
        device_id,
        source_total_sectors,
        read_native,
    )?;
    export_native_plain_image(path, &plan)
}

/// Explicitly rebuild LCE in an independently verified 4Kn SOURCE-backed
/// *ordinary-file virtual image* using the candidate zero8+A7F0 producer.
///
/// This differs intentionally from lossless source replay: the entire source
/// 4096B LCE is replaced with freshly generated ciphertext, including the
/// final 1024B encrypted-zero tail. The immutable native protocol LBA0..12
/// remains source-preserved. This is NOT a validated OEM 4Kn write image or a
/// path to a physical disk: fresh 4Kn EDP metadata is not synthesized.
pub fn plan_native_edp_regenerated_lce_image<F>(
    layout: &crate::provision::NativeEdpLayoutPlan,
    source_protocol: &crate::protocol::image::NativeProtocolImage,
    device_id: &str,
    source_total_sectors: u64,
    read_native: F,
) -> Result<NativeVirtualDiskPlan, String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    if layout.logical_sector_bytes != 4096 || layout.lce.sector_count != 1 {
        return Err("重新生成LCE仅支持完整原生4096B虚拟盘".into());
    }
    let mut plan = plan_native_edp_source_replay_image(
        layout,
        source_protocol,
        device_id,
        source_total_sectors,
        read_native,
    )?;
    let start_byte_offset = layout
        .lce
        .start_lba
        .checked_mul(4096)
        .ok_or("新LCE原生字节偏移溢出")?;
    let chs_bytes = start_byte_offset
        .checked_add(crate::protocol::lba7_compat::LBA7_COMPAT_CHS_TAIL_DISTANCE_BYTES)
        .ok_or("新LCE CHS位置溢出")?;
    let lce_layout = crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
        chs_bytes,
        start_byte_offset,
        start_lba: layout.lce.start_lba,
        size_bytes: 4096,
        size_sectors: 1,
    };
    let ciphertext = crate::provision::build_native_lce_ciphertext(lce_layout, 4096)?;
    let block = plan
        .writes
        .iter_mut()
        .find(|write| write.relative_lba == layout.lce.start_lba)
        .ok_or("已验证来源重放缺少原生LCE块")?;
    block.data = ciphertext;
    Ok(plan)
}

/// Export the explicitly regenerated 4Kn LCE candidate to a freshly created
/// regular-file image, using the common MBR-last native transaction/readback.
pub fn export_native_edp_regenerated_lce_image<F>(
    path: &Path,
    layout: &crate::provision::NativeEdpLayoutPlan,
    source_protocol: &crate::protocol::image::NativeProtocolImage,
    device_id: &str,
    source_total_sectors: u64,
    read_native: F,
) -> Result<(), String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    let plan = plan_native_edp_regenerated_lce_image(
        layout,
        source_protocol,
        device_id,
        source_total_sectors,
        read_native,
    )?;
    export_native_plain_image(path, &plan)
}

/// One optional *offline-only* filesystem initialization over a confirmed
/// source partition. The password is used only to authenticate the original
/// wrapped FileKey (including CRC); it is never stored in the write plan.
pub struct NativeEdpReplayFormat<'a> {
    pub partition_index: usize,
    pub filesystem: &'a NativeFormatPlan,
    pub source_password: Option<&'a [u8]>,
}

/// Combine exact source-owned EDP 512B/4Kn protocol + LCE with independently
/// produced native FAT/exFAT metadata for selected existing partitions.
///
/// This is a disposable sparse-image format experiment. All original user
/// data is ABSENT, even for unselected partitions. It is NOT a lossless disk
/// clone, not a new OEM label producer, and never authorizes hardware writes.
/// Encrypted partition metadata requires CRC-verified original password/key.
pub fn plan_native_edp_source_replay_with_formats<F>(
    layout: &crate::provision::NativeEdpLayoutPlan,
    source_protocol: &crate::protocol::image::NativeProtocolImage,
    device_id: &str,
    source_total_sectors: u64,
    read_native: F,
    formats: &[NativeEdpReplayFormat<'_>],
) -> Result<NativeVirtualDiskPlan, String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    use crate::partition_transform::{
        transform_native_sector_offline, NativeCipherDirection, NativePartitionDataCipher,
    };
    use std::collections::{BTreeMap, BTreeSet};

    let mut base = plan_native_edp_source_replay_image(
        layout,
        source_protocol,
        device_id,
        source_total_sectors,
        read_native,
    )?;
    let native_bytes = layout.logical_sector_bytes as usize;
    let mut native_protocol = vec![0u8; 13 * native_bytes];
    for block in base.writes.iter().filter(|write| write.relative_lba < 13) {
        let offset = block.relative_lba as usize * native_bytes;
        native_protocol[offset..offset + native_bytes].copy_from_slice(&block.data);
    }
    let source = crate::protocol::image::NativeProtocolImage::from_native_bytes(
        layout.logical_sector_bytes,
        native_protocol,
    )
    .map_err(|error| error.to_string())?;
    let parsed = crate::provision::parse_existing_provision_native(
        &source,
        device_id,
        layout.total_sectors,
    )?
    .ok_or("无法复核来源EDPF分区密钥记录")?;

    let mut occupied = base
        .writes
        .iter()
        .map(|write| (write.relative_lba, write.data.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut seen_partitions = BTreeSet::new();
    for request in formats {
        if !seen_partitions.insert(request.partition_index) {
            return Err("重复初始化同一来源分区".into());
        }
        let partition = layout
            .partitions
            .get(request.partition_index)
            .ok_or("来源格式化分区索引越界")?;
        let format = request.filesystem;
        let geometry = partition.geometry;
        if partition.semantics.role == crate::provision::PartitionRole::CompatibilityReserve
            || geometry.filesystem != Some(format.filesystem)
            || format.expected_metadata.kind != format.filesystem
            || format.geometry.partition_offset != geometry.start_lba
            || format.geometry.sector_count != geometry.sector_count
            || format.geometry.sector_size != layout.logical_sector_bytes
        {
            return Err("来源分区、文件系统类型或原生格式化几何不匹配".into());
        }
        crate::filesystem::validate_writable_filesystem(format.filesystem)
            .map_err(|error| error.to_string())?;
        let boot = format
            .writes
            .iter()
            .find(|write| write.relative_lba == 0)
            .ok_or("原生格式化缺少启动扇区")?;
        if crate::filesystem::detect_native_boot_sector(
            &boot.data,
            geometry.sector_count,
            layout.logical_sector_bytes,
        )
        .map_err(|error| error.to_string())?
            != Some(format.filesystem)
        {
            return Err("原生格式化启动扇区与声明的文件系统不一致".into());
        }
        let cipher = if geometry.physically_encrypted {
            let record = parsed
                .records
                .get(request.partition_index)
                .copied()
                .ok_or("来源密码域记录不存在")?;
            if record.lba12.need_encrypt == 0 {
                return Err("来源物理加密分区没有加密密码域".into());
            }
            let password = request
                .source_password
                .filter(|password| !password.is_empty())
                .ok_or("离线加密格式化需要来源密码与CRC认证FileKey")?;
            let key = record
                .verified_file_key(Some(password))
                .map_err(|error| format!("来源FileKey验证失败: {error}"))?;
            let algorithm =
                NativePartitionDataCipher::from_encrypt_mode(record.lba12.encrypt_mode)?;
            Some((algorithm, key))
        } else {
            None
        };
        let mut relative = BTreeSet::new();
        for write in &format.writes {
            if write.relative_lba >= geometry.sector_count
                || write.data.len() != native_bytes
                || !relative.insert(write.relative_lba)
            {
                return Err("原生格式化包含重复、截断或越界分区扇区".into());
            }
            let absolute = geometry
                .start_lba
                .checked_add(write.relative_lba)
                .ok_or("格式化绝对LBA溢出")?;
            let data = if let Some((algorithm, key)) = &cipher {
                transform_native_sector_offline(
                    *algorithm,
                    NativeCipherDirection::Encrypt,
                    &write.data,
                    key,
                    absolute,
                    layout.logical_sector_bytes,
                )?
            } else {
                write.data.clone()
            };
            if occupied.insert(absolute, data).is_some() {
                return Err(format!(
                    "文件系统与来源协议、LCE或其他分区重叠LBA{absolute}"
                ));
            }
        }
    }
    let mbr = occupied.remove(&0).ok_or("原生重放缺少MBR")?;
    base.writes = occupied
        .into_iter()
        .map(|(relative_lba, data)| crate::filesystem::NativeFilesystemWrite { relative_lba, data })
        .collect();
    base.writes.push(crate::filesystem::NativeFilesystemWrite {
        relative_lba: 0,
        data: mbr,
    });
    Ok(base)
}

/// Independently reopen and verify every authored native block from the
/// *published* disposable virtual image. No physical I/O or device handles:
/// the output must be a regular file, not a symlink or raw disk. We do not
/// claim that sparse, unowned user-data blocks have been initialized.
pub fn verify_native_virtual_image(
    path: &Path,
    plan: &NativeVirtualDiskPlan,
) -> Result<(), String> {
    if path.starts_with("/dev") || crate::platform::is_raw_device_path(&path.to_string_lossy()) {
        return Err("禁止从物理设备路径执行虚拟镜像回读认证".into());
    }
    let byte_len = validate_virtual_image_plan(plan)?;
    if !fs::symlink_metadata(path)
        .map_err(|e| format!("镜像路径元数据读取失败: {e}"))?
        .file_type()
        .is_file()
    {
        return Err("虚拟镜像回读只接受普通文件，拒绝符号链接或设备".into());
    }
    let mut fresh = fs::File::open(path).map_err(|e| format!("无法独立重新打开虚拟镜像: {e}"))?;
    if !fresh
        .metadata()
        .is_ok_and(|meta| meta.file_type().is_file() && meta.len() == byte_len)
    {
        return Err("重新打开后的虚拟镜像不是完整预期容量的普通文件".into());
    }
    let mut block = vec![0u8; plan.sector_bytes as usize];
    for write in &plan.writes {
        let offset = write
            .relative_lba
            .checked_mul(u64::from(plan.sector_bytes))
            .ok_or("虚拟镜像回读LBA偏移溢出")?;
        fresh
            .seek(SeekFrom::Start(offset))
            .map_err(|e| format!("回读LBA{}定位失败: {e}", write.relative_lba))?;
        fresh
            .read_exact(&mut block)
            .map_err(|e| format!("重新打开后回读LBA{}失败: {e}", write.relative_lba))?;
        if block != write.data {
            return Err(format!(
                "重新打开后LBA{}完整原生块验证失败",
                write.relative_lba
            ));
        }
    }
    Ok(())
}

fn validate_virtual_image_plan(plan: &NativeVirtualDiskPlan) -> Result<u64, String> {
    let byte_len = plan
        .total_sectors
        .checked_mul(u64::from(plan.sector_bytes))
        .ok_or("虚拟整盘字节长度溢出")?;
    if !matches!(plan.sector_bytes, 512 | 4096) || byte_len == 0 {
        return Err("不支持的原生镜像逻辑扇区几何".into());
    }
    if plan.writes.is_empty() || plan.writes.last().is_none_or(|w| w.relative_lba != 0) {
        return Err("原生镜像缺少最后提交的MBR".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for write in &plan.writes {
        if write.relative_lba >= plan.total_sectors
            || write.data.len() != plan.sector_bytes as usize
            || !seen.insert(write.relative_lba)
        {
            return Err("原生镜像写计划含越界、截断或重复的LBA".into());
        }
    }
    Ok(byte_len)
}

/// Only unlink a file still referring to our exact create_new() inode.
/// An attacker or another process could replace the destination during the
/// transaction; never unlink that unrelated file on our failure path.
#[cfg(unix)]
fn is_same_created_file(path: &Path, original: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    fs::symlink_metadata(path).is_ok_and(|current| {
        current.file_type().is_file()
            && current.dev() == original.dev()
            && current.ino() == original.ino()
    })
}
#[cfg(not(unix))]
fn is_same_created_file(_path: &Path, _original: &fs::Metadata) -> bool {
    // No portable stable file-ID check: preserve an ambiguous path.
    false
}

pub fn export_native_plain_image(path: &Path, plan: &NativeVirtualDiskPlan) -> Result<(), String> {
    validate_virtual_image_plan(plan)?;
    if path.starts_with("/dev") || crate::platform::is_raw_device_path(&path.to_string_lossy()) {
        return Err("拒绝将虚拟镜像输出到设备路径".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("无法排他创建新的普通文件镜像: {error}"))?;
    let original = file
        .metadata()
        .map_err(|e| format!("新建镜像身份获取失败: {e}"))?;
    let result = write_native_virtual_plan(&mut file, plan).and_then(|()| {
        file.sync_all()
            .map_err(|e| format!("重新打开前强制落盘失败: {e}"))?;
        let current =
            fs::symlink_metadata(path).map_err(|e| format!("复核镜像路径身份失败: {e}"))?;
        #[cfg(unix)]
        if !is_same_created_file(path, &original) {
            return Err("虚拟镜像文件身份发生替换，禁止信任回读".into());
        }
        if !current.file_type().is_file() {
            return Err("虚拟镜像路径在写入期间成为特殊文件".into());
        }
        verify_native_virtual_image(path, plan)
    });
    drop(file);
    if result.is_err() && is_same_created_file(path, &original) {
        let _ = fs::remove_file(path);
    }
    result
}
