//! Read-only inspect application service shared by CLI/TUI frontends.

use std::path::Path;

use crate::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use crate::diskio::{self, FileDev, SectorReadCache};
use crate::identify::identify;
use crate::inspect::{self, InspectMeta, SectorView};
use crate::sysinfo::{self, CmdRunner};

#[derive(Debug, Clone)]
pub struct InspectWorkspace {
    pub source: String,
    pub meta: InspectMeta,
    pub views: Vec<SectorView>,
}

fn analyze_image(
    source: String,
    data: &[u8],
    meta: InspectMeta,
) -> Result<InspectWorkspace, String> {
    if data.len() != METADATA_IMAGE_LEN {
        return Err(format!(
            "错误: inspect 镜像长度 {}B，预期 {}B (LBA0-12)",
            data.len(),
            METADATA_IMAGE_LEN
        ));
    }
    let views = (0..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            let start = lba as usize * SECTOR;
            inspect::analyze_sector_with_context(
                lba,
                &data[start..start + SECTOR],
                &meta,
                Some(data),
            )
        })
        .collect();
    Ok(InspectWorkspace {
        source,
        meta,
        views,
    })
}

pub fn load_backup_inspect(path: &Path) -> Result<InspectWorkspace, String> {
    let verified = crate::edpb::verify_file(path)
        .map_err(|error| format!("错误: EDPB 校验失败 {}: {error}", path.display()))?;
    let data = crate::edpb::read_raw_protocol(path)
        .map_err(|error| format!("错误: 读取 EDPB LBA0-12 失败 {}: {error}", path.display()))?;
    let manifest = &verified.manifest;
    let meta = InspectMeta {
        device_id: Some(manifest.device.device_id.clone()),
        vid: Some(manifest.device.vid.clone()),
        pid: Some(manifest.device.pid.clone()),
        size_bytes: manifest.geometry.capacity_bytes,
        onlyid: manifest.device.onlyid.clone(),
    };
    analyze_image(path.display().to_string(), &data, meta)
}

pub fn load_disk_inspect(runner: &dyn CmdRunner, disk: u32) -> Result<InspectWorkspace, String> {
    crate::application::write::guard_usb_disk(runner, disk).map_err(|error| error.msg)?;
    let path = diskio::raw_path(disk);
    let mut dev = FileDev::open_rdonly(&path)
        .map_err(|error| format!("错误: 无法只读打开 disk{disk}: {error}"))?;
    let mut reader = SectorReadCache::new(&mut dev);
    let mut sectors = Vec::with_capacity(METADATA_IMAGE_LEN);
    for lba in 0..METADATA_SECTOR_COUNT as u32 {
        let raw = reader
            .read_sector(lba)
            .map_err(|error| format!("错误: 读取 disk{disk} LBA{lba} 失败: {error}"))?;
        if raw.len() != SECTOR {
            return Err(format!(
                "错误: disk{disk} LBA{lba} 长度 {}B，预期 {SECTOR}B",
                raw.len()
            ));
        }
        sectors.extend_from_slice(&raw);
    }

    let raw7 = &sectors[7 * SECTOR..8 * SECTOR];
    let id = identify(runner, disk, raw7).device_id;
    let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
    let size_bytes = sysinfo::disk_total_sectors(runner, disk)
        .and_then(|value| value.checked_mul(SECTOR as u64));
    let onlyid = diskio::lba4_label_id_from(&sectors[4 * SECTOR..5 * SECTOR]);
    let meta = InspectMeta {
        device_id: id,
        vid: (vid != "xxxx").then_some(vid),
        pid: (pid != "xxxx").then_some(pid),
        size_bytes,
        onlyid,
    };
    analyze_image(format!("物理盘 disk{disk} ({path})"), &sectors, meta)
}

pub const MAX_ADVANCED_INSPECT_SECTORS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectMode {
    Raw,
    Decode,
    Meta,
}

impl AdvancedInspectMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Decode => "decode",
            Self::Meta => "meta",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Raw => Self::Decode,
            Self::Decode => Self::Meta,
            Self::Meta => Self::Raw,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Raw => Self::Meta,
            Self::Decode => Self::Raw,
            Self::Meta => Self::Decode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectRequest {
    pub mode: AdvancedInspectMode,
    pub lbas: Vec<u64>,
    pub export_dir: Option<std::path::PathBuf>,
    pub device_id_override: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectItem {
    pub lba: u64,
    pub regions: Vec<String>,
    pub raw: Vec<u8>,
    pub raw_sha256: String,
    pub raw_nonzero: usize,
    pub decoded: Option<Vec<u8>>,
    pub decoded_sha256: Option<String>,
    pub method: Option<String>,
    pub meta_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectWorkspace {
    pub source: String,
    pub meta: InspectMeta,
    pub mode: AdvancedInspectMode,
    pub items: Vec<AdvancedInspectItem>,
    pub export_dir: Option<std::path::PathBuf>,
}

fn parse_u64_decimal(value: &str, label: &str) -> Result<u64, String> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{label} 必须为非负十进制整数"));
    }
    value
        .parse::<u64>()
        .map_err(|_| format!("{label} 超出 u64 范围"))
}

pub fn parse_advanced_lbas(spec: &str, count: &str) -> Result<Vec<u64>, String> {
    use std::collections::HashSet;

    let spec = spec.trim();
    let count = count.trim();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if spec.is_empty() {
        if !count.is_empty() {
            return Err("填写 count 时必须先填写单个起始 LBA".into());
        }
        return Ok((0..METADATA_SECTOR_COUNT as u64).collect());
    }

    for token in spec.split(',').map(str::trim) {
        if token.is_empty() {
            return Err("LBA 列表包含空项".into());
        }
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_u64_decimal(start, "LBA 范围起点")?;
            let end = parse_u64_decimal(end, "LBA 范围终点")?;
            if start > end {
                return Err(format!("LBA 范围起点大于终点: {token}"));
            }
            let span = end
                .checked_sub(start)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| format!("LBA 范围溢出: {token}"))?;
            if span > MAX_ADVANCED_INSPECT_SECTORS as u64 {
                return Err(format!(
                    "单个 LBA 范围最多包含 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                ));
            }
            for lba in start..=end {
                if seen.insert(lba) {
                    if out.len() >= MAX_ADVANCED_INSPECT_SECTORS {
                        return Err(format!(
                            "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                        ));
                    }
                    out.push(lba);
                }
            }
        } else {
            let lba = parse_u64_decimal(token, "LBA")?;
            if seen.insert(lba) {
                if out.len() >= MAX_ADVANCED_INSPECT_SECTORS {
                    return Err(format!(
                        "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                    ));
                }
                out.push(lba);
            }
        }
    }

    if !count.is_empty() {
        if out.len() != 1 {
            return Err("count 只能与单个起始 LBA 同时使用".into());
        }
        let count = parse_u64_decimal(count, "count")?;
        if count == 0 || count > MAX_ADVANCED_INSPECT_SECTORS as u64 {
            return Err(format!("count 必须为 1..={MAX_ADVANCED_INSPECT_SECTORS}"));
        }
        let start = out[0];
        out.clear();
        for offset in 0..count {
            out.push(
                start
                    .checked_add(offset)
                    .ok_or_else(|| "count 产生的 LBA 范围溢出".to_string())?,
            );
        }
    }

    Ok(out)
}

fn advanced_plain_hex(data: &[u8]) -> String {
    let mut out = String::new();
    for (line_no, line) in data.chunks(16).enumerate() {
        let base = line_no * 16;
        out.push_str(&format!("+0x{base:03X}: "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(&byte) = line.get(i) {
                out.push_str(&format!("{byte:02X} "));
            } else {
                out.push_str("   ");
            }
        }
        out.push(' ');
        for &byte in line {
            out.push(if (0x20..=0x7e).contains(&byte) {
                byte as char
            } else {
                '.'
            });
        }
        out.push('\n');
    }
    out
}

fn export_advanced_bytes(dir: &Path, lba: u64, suffix: &str, data: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("创建 Inspect 导出目录 {} 失败: {error}", dir.display()))?;
    let base = format!("LBA{lba}_{suffix}");
    std::fs::write(dir.join(format!("{base}.bin")), data)
        .map_err(|error| format!("导出 {base}.bin 失败: {error}"))?;
    std::fs::write(dir.join(format!("{base}.hex")), advanced_plain_hex(data))
        .map_err(|error| format!("导出 {base}.hex 失败: {error}"))?;
    Ok(())
}

fn export_advanced_meta(dir: &Path, lba: u64, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("创建 Inspect 导出目录 {} 失败: {error}", dir.display()))?;
    std::fs::write(dir.join(format!("LBA{lba}_meta.txt")), text)
        .map_err(|error| format!("导出 LBA{lba}_meta.txt 失败: {error}"))
}

fn advanced_meta_text(
    context: &crate::inspect_target::InspectDiskContext,
    meta: &InspectMeta,
    lba: u64,
    raw: &[u8],
    partition_boot_raw: Option<&[u8]>,
    partition_boot_issue: Option<&str>,
) -> Result<String, String> {
    use crate::inspect_target::PhysicalDataState;

    let offset = lba
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| "LBA 字节偏移溢出".to_string())?;
    let mut out = format!(
        "LBA: {lba}\n物理字节偏移: {offset} (0x{offset:X})\nRAW SHA-256: {}\nRAW 非零字节: {}/512\n",
        crate::sha256::sha256_hex(raw),
        raw.iter().filter(|&&byte| byte != 0).count()
    );
    let regions = context.regions(lba);
    out.push_str("区域:\n");
    for region in &regions {
        out.push_str(&format!("  - {}\n", region.label()));
    }
    if let Some(primary) = regions.first() {
        out.push_str(&format!("主区域: {}\n", primary.label()));
    }
    if regions.len() > 1 {
        out.push_str(&format!(
            "重叠区域: {}\n",
            regions[1..]
                .iter()
                .map(crate::inspect_target::SectorRegion::label)
                .collect::<Vec<_>>()
                .join("；")
        ));
    } else {
        out.push_str("重叠区域: 无\n");
    }

    if lba <= u64::from(crate::common::METADATA_LAST_LBA) {
        let lba32 = u32::try_from(lba).map_err(|_| format!("LBA{lba} 超出协议解析器范围"))?;
        let view =
            inspect::analyze_sector_with_context(lba32, raw, meta, Some(&context.protocol_image));
        out.push_str(&format!("协议解码: {}\n", view.method));
        out.push_str(&inspect::render_fields(&view));
        for note in &view.notes {
            out.push_str(&format!("  └─ {note}\n"));
        }
    }

    if let Some(partition) = context.partition_for_lba(lba) {
        out.push_str(&format!(
            "分区: index={} type={} relative_lba={} start={} sectors={}\n",
            partition.index,
            partition.partition_type,
            lba - partition.start_sector,
            partition.start_sector,
            partition.sector_count,
        ));
        out.push_str(&format!(
            "加密配置: NeedEncrypt={} EncryptMode={}\n",
            partition.need_encrypt, partition.encrypt_mode
        ));
        out.push_str(&format!(
            "MBR 直接暴露: {}\n",
            context.partition_mbr_exposure(partition)
        ));
        out.push_str(&format!(
            "密钥: FileKeyCRC=0x{:08X} 状态={}\n",
            partition.file_key_crc,
            context.partition_file_key_crc_status(partition)
        ));
        let state = match partition_boot_raw {
            Some(boot) => context.partition_physical_state(partition, boot),
            None => PhysicalDataState::Unknown {
                reason: partition_boot_issue
                    .unwrap_or("缺少分区起始扇区证据")
                    .to_string(),
            },
        };
        out.push_str(&format!("物理数据状态: {}\n", state.label()));
        out.push_str(&format!("decode 策略: {}\n", state.decode_strategy()));
        out.push_str(&format!(
            "文件系统识别: {}\n",
            state
                .filesystem()
                .map(|filesystem| filesystem.label())
                .unwrap_or("未确认")
        ));
    }

    if let Some(lce) = &context.lce {
        if lba >= lce.start_lba && lba < lce.start_lba + lce.sector_count {
            out.push_str(&format!(
                "LCE: start={} sectors={} pointers={:?} mode={:?} chs_crosscheck={:?}\n",
                lce.start_lba,
                lce.sector_count,
                lce.lba7_pointer_entries
                    .iter()
                    .map(|pointer| (pointer.entry_index, pointer.partition_type))
                    .collect::<Vec<_>>(),
                lce.official_partition_mode,
                lce.chs_expected_start_lba
            ));
            out.push_str("LCE 解码: EDPSECDISK200709/A6B0，zero8，64 位物理字节偏移 tweak\n");
        }
    }
    for issue in &context.context_issues {
        out.push_str(&format!("上下文提示: {issue}\n"));
    }
    Ok(out)
}

fn run_advanced_source<F>(
    source: String,
    meta: InspectMeta,
    context: crate::inspect_target::InspectDiskContext,
    request: &AdvancedInspectRequest,
    mut read: F,
) -> Result<AdvancedInspectWorkspace, String>
where
    F: FnMut(u64) -> std::io::Result<Vec<u8>>,
{
    let lbas = if request.lbas.is_empty() {
        (0..METADATA_SECTOR_COUNT as u64).collect::<Vec<_>>()
    } else {
        request.lbas.clone()
    };
    if lbas.len() > MAX_ADVANCED_INSPECT_SECTORS {
        return Err(format!(
            "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
        ));
    }

    let mut items = Vec::with_capacity(lbas.len());
    for lba in lbas {
        context.validate_lba(lba)?;
        let raw = read(lba).map_err(|error| format!("读取 LBA{lba} 失败: {error}"))?;
        if raw.len() != SECTOR {
            return Err(format!("LBA{lba} 返回 {}B，预期 {SECTOR}B", raw.len()));
        }

        let mut partition_boot = None;
        let mut partition_boot_issue = None;
        if request.mode != AdvancedInspectMode::Raw {
            if let Some(partition) = context.partition_for_lba(lba) {
                if partition.start_sector == lba {
                    partition_boot = Some(raw.clone());
                } else {
                    match read(partition.start_sector) {
                        Ok(boot) if boot.len() == SECTOR => partition_boot = Some(boot),
                        Ok(boot) => {
                            partition_boot_issue = Some(format!(
                                "分区起始 LBA{} 只读取到 {}B",
                                partition.start_sector,
                                boot.len()
                            ));
                        }
                        Err(error) => {
                            partition_boot_issue = Some(format!(
                                "无法读取分区起始 LBA{}: {error}",
                                partition.start_sector
                            ));
                        }
                    }
                }
            }
        }

        let regions = context
            .regions(lba)
            .into_iter()
            .map(|region| region.label())
            .collect::<Vec<_>>();
        let raw_sha256 = crate::sha256::sha256_hex(&raw);
        let raw_nonzero = raw.iter().filter(|&&byte| byte != 0).count();

        let mut item = AdvancedInspectItem {
            lba,
            regions,
            raw: raw.clone(),
            raw_sha256,
            raw_nonzero,
            decoded: None,
            decoded_sha256: None,
            method: None,
            meta_text: None,
        };

        match request.mode {
            AdvancedInspectMode::Raw => {
                if let Some(dir) = &request.export_dir {
                    export_advanced_bytes(dir, lba, "raw", &raw)?;
                }
            }
            AdvancedInspectMode::Decode => {
                let (decoded, method) = if lba <= u64::from(crate::common::METADATA_LAST_LBA) {
                    let lba32 =
                        u32::try_from(lba).map_err(|_| format!("LBA{lba} 超出协议解析器范围"))?;
                    let view = inspect::analyze_sector_with_context(
                        lba32,
                        &raw,
                        &meta,
                        Some(&context.protocol_image),
                    );
                    (view.decoded, view.method)
                } else {
                    context.decode_non_protocol_with_boot(lba, &raw, partition_boot.as_deref())?
                };
                item.decoded_sha256 = Some(crate::sha256::sha256_hex(&decoded));
                item.method = Some(method);
                if let Some(dir) = &request.export_dir {
                    export_advanced_bytes(dir, lba, "decoded", &decoded)?;
                }
                item.decoded = Some(decoded);
            }
            AdvancedInspectMode::Meta => {
                let text = advanced_meta_text(
                    &context,
                    &meta,
                    lba,
                    &raw,
                    partition_boot.as_deref(),
                    partition_boot_issue.as_deref(),
                )?;
                if let Some(dir) = &request.export_dir {
                    export_advanced_meta(dir, lba, &text)?;
                }
                item.meta_text = Some(text);
            }
        }
        items.push(item);
    }

    Ok(AdvancedInspectWorkspace {
        source,
        meta,
        mode: request.mode,
        items,
        export_dir: request.export_dir.clone(),
    })
}

struct AdvancedBackupReader {
    path: std::path::PathBuf,
    manifest: crate::edpb::Manifest,
    protocol: Vec<u8>,
    cache: std::collections::BTreeMap<String, Vec<u8>>,
}

impl AdvancedBackupReader {
    fn read_sector(&mut self, lba: u64) -> std::io::Result<Vec<u8>> {
        if lba < METADATA_SECTOR_COUNT as u64 {
            let start = usize::try_from(lba).unwrap() * SECTOR;
            return Ok(self.protocol[start..start + SECTOR].to_vec());
        }
        let found = self.manifest.extents.iter().find_map(|extent| {
            let end = extent.start_lba.checked_add(extent.sector_count)?;
            if lba < extent.start_lba || lba >= end {
                return None;
            }
            let artifact = self.manifest.artifacts.iter().find(|artifact| {
                artifact.kind == "raw_sectors"
                    && artifact.source_extent_ids.iter().any(|id| id == &extent.id)
            })?;
            Some((extent.start_lba, artifact.id.clone()))
        });
        let Some((start_lba, artifact_id)) = found else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("EDPB 未采集 LBA{lba} 的原始扇区"),
            ));
        };
        if !self.cache.contains_key(&artifact_id) {
            let data = crate::edpb::read_artifact(&self.path, &artifact_id)
                .map_err(std::io::Error::other)?;
            self.cache.insert(artifact_id.clone(), data);
        }
        let data = self
            .cache
            .get(&artifact_id)
            .expect("刚插入的 Artifact 必须存在");
        let offset = usize::try_from(lba - start_lba)
            .ok()
            .and_then(|sector| sector.checked_mul(SECTOR))
            .ok_or_else(|| std::io::Error::other("EDPB Artifact 扇区偏移溢出"))?;
        data.get(offset..offset + SECTOR)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "EDPB Artifact 截断")
            })
    }
}

pub fn load_backup_advanced_inspect(
    path: &Path,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, String> {
    let verified = crate::edpb::verify_file(path)
        .map_err(|error| format!("EDPB 校验失败 {}: {error}", path.display()))?;
    let protocol = crate::edpb::read_raw_protocol(path)
        .map_err(|error| format!("读取 EDPB LBA0-12 失败 {}: {error}", path.display()))?;
    let mut meta = InspectMeta {
        device_id: Some(verified.manifest.device.device_id.clone()),
        vid: Some(verified.manifest.device.vid.clone()),
        pid: Some(verified.manifest.device.pid.clone()),
        size_bytes: verified.manifest.geometry.capacity_bytes,
        onlyid: verified.manifest.device.onlyid.clone(),
    };
    if let Some(device_id) = request.device_id_override.as_ref() {
        meta.device_id = Some(device_id.clone());
    }
    let total_sectors = verified
        .manifest
        .geometry
        .total_sectors
        .ok_or_else(|| "EDPB 缺少 total_sectors，无法校验任意 LBA".to_string())?;
    let context = crate::inspect_target::InspectDiskContext::new(
        protocol.clone(),
        meta.device_id.clone(),
        total_sectors,
    );
    let mut reader = AdvancedBackupReader {
        path: path.to_path_buf(),
        manifest: verified.manifest,
        protocol,
        cache: std::collections::BTreeMap::new(),
    };
    run_advanced_source(path.display().to_string(), meta, context, request, |lba| {
        reader.read_sector(lba)
    })
}

pub fn load_disk_advanced_inspect(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, String> {
    crate::application::write::guard_usb_disk(runner, disk).map_err(|error| error.msg)?;
    let path = diskio::raw_path(disk);
    let mut dev =
        FileDev::open_rdonly(&path).map_err(|error| format!("无法只读打开 disk{disk}: {error}"))?;
    let total_sectors = sysinfo::disk_total_sectors(runner, disk)
        .ok_or_else(|| "无法取得设备总扇区数".to_string())?;

    let mut protocol = Vec::with_capacity(METADATA_IMAGE_LEN);
    for lba in 0..METADATA_SECTOR_COUNT as u64 {
        let raw = dev
            .read_sector_u64(lba)
            .map_err(|error| format!("读取协议上下文 LBA{lba} 失败: {error}"))?;
        if raw.len() != SECTOR {
            return Err(format!("LBA{lba} 只读取到 {}B", raw.len()));
        }
        protocol.extend_from_slice(&raw);
    }

    let raw7 = &protocol[7 * SECTOR..8 * SECTOR];
    let id = identify(runner, disk, raw7).device_id;
    let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
    let mut meta = InspectMeta {
        device_id: id,
        vid: (vid != "xxxx").then_some(vid),
        pid: (pid != "xxxx").then_some(pid),
        size_bytes: total_sectors.checked_mul(SECTOR as u64),
        onlyid: diskio::lba4_label_id_from(&protocol[4 * SECTOR..5 * SECTOR]),
    };
    if let Some(device_id) = request.device_id_override.as_ref() {
        meta.device_id = Some(device_id.clone());
    }
    let context = crate::inspect_target::InspectDiskContext::new(
        protocol,
        meta.device_id.clone(),
        total_sectors,
    );

    run_advanced_source(
        format!("物理盘 disk{disk} ({path})"),
        meta,
        context,
        request,
        |lba| dev.read_sector_u64(lba),
    )
}

#[cfg(test)]
mod advanced_tests {
    use super::*;

    #[test]
    fn advanced_lba_parser_matches_cli_list_range_and_count_semantics() {
        assert_eq!(
            parse_advanced_lbas("", "").unwrap(),
            (0u64..13).collect::<Vec<_>>()
        );
        assert_eq!(
            parse_advanced_lbas("7,12,20-22", "").unwrap(),
            vec![7, 12, 20, 21, 22]
        );
        assert_eq!(
            parse_advanced_lbas("100", "3").unwrap(),
            vec![100, 101, 102]
        );
        assert!(parse_advanced_lbas("7,12", "2").is_err());
        assert!(parse_advanced_lbas("22-20", "").is_err());
        assert!(parse_advanced_lbas("", "2").is_err());
        assert!(parse_advanced_lbas("0", "0").is_err());
    }

    #[test]
    fn advanced_mode_cycles_without_hidden_state() {
        assert_eq!(AdvancedInspectMode::Raw.next(), AdvancedInspectMode::Decode);
        assert_eq!(
            AdvancedInspectMode::Decode.next(),
            AdvancedInspectMode::Meta
        );
        assert_eq!(AdvancedInspectMode::Meta.next(), AdvancedInspectMode::Raw);
        assert_eq!(
            AdvancedInspectMode::Raw.previous(),
            AdvancedInspectMode::Meta
        );
        assert_eq!(
            AdvancedInspectMode::Meta.previous(),
            AdvancedInspectMode::Decode
        );
    }
}
