//! Read-only inspect application service shared by CLI/TUI frontends.

use std::path::Path;

pub use super::evidence::SectorReader;
use super::evidence::{EvidenceError, EvidenceSource};
use crate::common::{METADATA_SECTOR_COUNT, SECTOR};
use crate::inspect::{self, InspectMeta};
use crate::sysinfo::CmdRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectErrorKind {
    InvalidRequest,
    OutOfRange,
    Backup,
    Io,
    Decode,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectError {
    kind: InspectErrorKind,
    message: String,
}

impl InspectError {
    pub fn new(kind: InspectErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> InspectErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::InvalidRequest, message)
    }

    fn out_of_range(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::OutOfRange, message)
    }

    fn io(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::Io, message)
    }

    fn decode(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::Decode, message)
    }
}

impl std::fmt::Display for InspectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for InspectError {}

impl From<EvidenceError> for InspectError {
    fn from(error: EvidenceError) -> Self {
        let kind = match &error {
            EvidenceError::BackupVerify { .. }
            | EvidenceError::BackupProtocolRead { .. }
            | EvidenceError::BackupProtocolLength { .. }
            | EvidenceError::BackupMissingGeometry { .. } => InspectErrorKind::Backup,
            EvidenceError::Target(_) | EvidenceError::DiskMissingGeometry { .. } => {
                InspectErrorKind::Target
            }
            EvidenceError::DiskOpen { .. } | EvidenceError::DiskProtocolRead { .. } => {
                InspectErrorKind::Io
            }
        };
        Self::new(kind, error.to_string())
    }
}

fn default_protocol_request() -> AdvancedInspectRequest {
    AdvancedInspectRequest {
        mode: AdvancedInspectMode::Decode,
        lbas: (0..METADATA_SECTOR_COUNT as u64).collect(),
        export_dir: None,
        device_id_override: None,
        fail_soft_decode: false,
    }
}

pub fn load_backup_inspect(path: &Path) -> Result<AdvancedInspectWorkspace, InspectError> {
    load_backup_advanced_inspect(path, &default_protocol_request())
}

pub fn load_disk_inspect(
    runner: &dyn CmdRunner,
    disk: u32,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    load_disk_advanced_inspect(runner, disk, &default_protocol_request())
}

pub const MAX_ADVANCED_INSPECT_SECTORS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectDecoderKind {
    Protocol,
    Lce,
    Partition,
}

pub const DECODER_REGISTRY: &[InspectDecoderKind] = &[
    InspectDecoderKind::Protocol,
    InspectDecoderKind::Lce,
    InspectDecoderKind::Partition,
];

impl InspectDecoderKind {
    fn matches(self, context: &crate::inspect_target::InspectDiskContext, lba: u64) -> bool {
        match self {
            Self::Protocol => lba <= u64::from(crate::common::METADATA_LAST_LBA),
            Self::Lce => context.lce.as_ref().is_some_and(|extent| {
                lba >= extent.start_lba
                    && lba < extent.start_lba.saturating_add(extent.sector_count)
            }),
            Self::Partition => context.partition_for_lba(lba).is_some(),
        }
    }
}

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
    pub fail_soft_decode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsoluteByteRange {
    pub start: u64,
    pub end_exclusive: u64,
}

impl AbsoluteByteRange {
    pub fn len(self) -> u64 {
        self.end_exclusive - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end_exclusive
    }

    pub fn start_lba(self) -> u64 {
        self.start / SECTOR as u64
    }

    pub fn end_lba(self) -> u64 {
        if self.end_exclusive == self.start {
            return self.start_lba();
        }
        (self.end_exclusive - 1) / SECTOR as u64
    }

    pub fn spans_sectors(self) -> bool {
        self.start_lba() != self.end_lba()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectFieldType {
    Magic,
    Text,
    Identity,
    Address,
    Size,
    Flag,
    Checksum,
}

impl From<crate::inspect::FieldStyle> for InspectFieldType {
    fn from(style: crate::inspect::FieldStyle) -> Self {
        match style {
            crate::inspect::FieldStyle::Magic => Self::Magic,
            crate::inspect::FieldStyle::Text => Self::Text,
            crate::inspect::FieldStyle::Identity => Self::Identity,
            crate::inspect::FieldStyle::Address => Self::Address,
            crate::inspect::FieldStyle::Size => Self::Size,
            crate::inspect::FieldStyle::Flag => Self::Flag,
            crate::inspect::FieldStyle::Checksum => Self::Checksum,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectFieldStatus {
    Known,
    Unknown,
    Reserved,
    Preserved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectField {
    pub range: AbsoluteByteRange,
    pub field_type: InspectFieldType,
    pub raw: Vec<u8>,
    pub decoded: Vec<u8>,
    pub status: InspectFieldStatus,
    pub label: String,
    pub value: String,
    pub style: crate::inspect::FieldStyle,
    pub group: Option<String>,
    pub children: Vec<crate::inspect::FieldChild>,
}

fn materialize_protocol_fields(
    lba: u64,
    raw: &[u8],
    decoded: &[u8],
    fields: &[crate::inspect::SectorField],
) -> Result<Vec<InspectField>, InspectError> {
    let base = lba
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| InspectError::decode(format!("LBA{lba} 字段绝对字节偏移溢出")))?;
    fields
        .iter()
        .map(|field| {
            if field.end < field.start || field.end > raw.len() || field.end > decoded.len() {
                return Err(InspectError::decode(format!(
                    "LBA{lba} 字段 {} range +0x{:X}..+0x{:X} 越界",
                    field.label, field.start, field.end
                )));
            }
            let start = base
                .checked_add(field.start as u64)
                .ok_or_else(|| {
                    InspectError::decode(format!("LBA{lba} 字段 {} 绝对起点溢出", field.label))
                })?;
            let end_exclusive = base
                .checked_add(field.end as u64)
                .ok_or_else(|| {
                    InspectError::decode(format!("LBA{lba} 字段 {} 绝对终点溢出", field.label))
                })?;
            Ok(InspectField {
                range: AbsoluteByteRange {
                    start,
                    end_exclusive,
                },
                field_type: field.style.into(),
                raw: raw[field.start..field.end].to_vec(),
                decoded: decoded[field.start..field.end].to_vec(),
                status: InspectFieldStatus::Known,
                label: field.label.clone(),
                value: field.value.clone(),
                style: field.style,
                group: field.group.clone(),
                children: field.children.clone(),
            })
        })
        .collect()
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
    pub decode_error: Option<String>,
    pub fields: Vec<InspectField>,
    pub notes: Vec<String>,
    pub meta_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectWorkspace {
    pub source: String,
    pub meta: InspectMeta,
    pub mode: AdvancedInspectMode,
    pub items: Vec<AdvancedInspectItem>,
    pub export_dir: Option<std::path::PathBuf>,
    pub topology: super::inspect_tree::InspectTopology,
}

fn parse_u64_decimal(value: &str, label: &str) -> Result<u64, InspectError> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(InspectError::invalid(format!(
            "{label} 必须为非负十进制整数"
        )));
    }
    value
        .parse::<u64>()
        .map_err(|_| InspectError::invalid(format!("{label} 超出 u64 范围")))
}

pub fn parse_advanced_lbas(spec: &str, count: &str) -> Result<Vec<u64>, InspectError> {
    use std::collections::HashSet;

    let spec = spec.trim();
    let count = count.trim();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if spec.is_empty() {
        if !count.is_empty() {
            return Err(InspectError::invalid("填写 count 时必须先填写单个起始 LBA"));
        }
        return Ok((0..METADATA_SECTOR_COUNT as u64).collect());
    }

    for token in spec.split(',').map(str::trim) {
        if token.is_empty() {
            return Err(InspectError::invalid("LBA 列表包含空项"));
        }
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_u64_decimal(start, "LBA 范围起点")?;
            let end = parse_u64_decimal(end, "LBA 范围终点")?;
            if start > end {
                return Err(InspectError::invalid(format!(
                    "LBA 范围起点大于终点: {token}"
                )));
            }
            let span = end
                .checked_sub(start)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| InspectError::invalid(format!("LBA 范围溢出: {token}")))?;
            if span > MAX_ADVANCED_INSPECT_SECTORS as u64 {
                return Err(InspectError::invalid(format!(
                    "单个 LBA 范围最多包含 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                )));
            }
            for lba in start..=end {
                if seen.insert(lba) {
                    if out.len() >= MAX_ADVANCED_INSPECT_SECTORS {
                        return Err(InspectError::invalid(format!(
                            "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                        )));
                    }
                    out.push(lba);
                }
            }
        } else {
            let lba = parse_u64_decimal(token, "LBA")?;
            if seen.insert(lba) {
                if out.len() >= MAX_ADVANCED_INSPECT_SECTORS {
                    return Err(InspectError::invalid(format!(
                        "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                    )));
                }
                out.push(lba);
            }
        }
    }

    if !count.is_empty() {
        if out.len() != 1 {
            return Err(InspectError::invalid("count 只能与单个起始 LBA 同时使用"));
        }
        let count = parse_u64_decimal(count, "count")?;
        if count == 0 || count > MAX_ADVANCED_INSPECT_SECTORS as u64 {
            return Err(InspectError::invalid(format!(
                "count 必须为 1..={MAX_ADVANCED_INSPECT_SECTORS}"
            )));
        }
        let start = out[0];
        out.clear();
        for offset in 0..count {
            out.push(
                start
                    .checked_add(offset)
                    .ok_or_else(|| InspectError::invalid("count 产生的 LBA 范围溢出"))?,
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

fn export_advanced_bytes(
    dir: &Path,
    lba: u64,
    suffix: &str,
    data: &[u8],
) -> Result<(), InspectError> {
    std::fs::create_dir_all(dir).map_err(|error| {
        InspectError::io(format!(
            "创建 Inspect 导出目录 {} 失败: {error}",
            dir.display()
        ))
    })?;
    let base = format!("LBA{lba}_{suffix}");
    std::fs::write(dir.join(format!("{base}.bin")), data)
        .map_err(|error| InspectError::io(format!("导出 {base}.bin 失败: {error}")))?;
    std::fs::write(dir.join(format!("{base}.hex")), advanced_plain_hex(data))
        .map_err(|error| InspectError::io(format!("导出 {base}.hex 失败: {error}")))?;
    Ok(())
}

fn export_advanced_meta(dir: &Path, lba: u64, text: &str) -> Result<(), InspectError> {
    std::fs::create_dir_all(dir).map_err(|error| {
        InspectError::io(format!(
            "创建 Inspect 导出目录 {} 失败: {error}",
            dir.display()
        ))
    })?;
    std::fs::write(dir.join(format!("LBA{lba}_meta.txt")), text)
        .map_err(|error| InspectError::io(format!("导出 LBA{lba}_meta.txt 失败: {error}")))
}

pub fn decode_sector(
    context: &crate::inspect_target::InspectDiskContext,
    meta: &InspectMeta,
    lba: u64,
    raw: &[u8],
    partition_boot_raw: Option<&[u8]>,
) -> Result<(Vec<u8>, String), InspectError> {
    let decoder = DECODER_REGISTRY
        .iter()
        .copied()
        .find(|decoder| decoder.matches(context, lba))
        .ok_or_else(|| {
            InspectError::decode(format!(
                "LBA{lba} 不属于已注册 decoder 区域；raw 可读，decode 拒绝猜测"
            ))
        })?;
    match decoder {
        InspectDecoderKind::Protocol => {
            let lba32 = u32::try_from(lba)
                .map_err(|_| InspectError::out_of_range(format!("LBA{lba} 超出协议解析器范围")))?;
            let view = inspect::analyze_sector_with_context(
                lba32,
                raw,
                meta,
                Some(&context.protocol_image),
            );
            Ok((view.decoded, view.method))
        }
        InspectDecoderKind::Lce | InspectDecoderKind::Partition => context
            .decode_non_protocol_with_boot(lba, raw, partition_boot_raw)
            .map_err(InspectError::decode),
    }
}

pub fn sector_meta_text(
    context: &crate::inspect_target::InspectDiskContext,
    meta: &InspectMeta,
    lba: u64,
    raw: &[u8],
    partition_boot_raw: Option<&[u8]>,
    partition_boot_issue: Option<&str>,
) -> Result<String, InspectError> {
    use crate::inspect_target::PhysicalDataState;

    let offset = lba
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| InspectError::out_of_range("LBA 字节偏移溢出"))?;
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
        let lba32 = u32::try_from(lba)
            .map_err(|_| InspectError::out_of_range(format!("LBA{lba} 超出协议解析器范围")))?;
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

fn run_advanced_source<R: SectorReader + ?Sized>(
    source: String,
    meta: InspectMeta,
    context: crate::inspect_target::InspectDiskContext,
    request: &AdvancedInspectRequest,
    reader: &mut R,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    let lbas = if request.lbas.is_empty() {
        (0..METADATA_SECTOR_COUNT as u64).collect::<Vec<_>>()
    } else {
        request.lbas.clone()
    };
    if lbas.len() > MAX_ADVANCED_INSPECT_SECTORS {
        return Err(InspectError::invalid(format!(
            "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
        )));
    }

    let mut items = Vec::with_capacity(lbas.len());
    for lba in lbas {
        context
            .validate_lba(lba)
            .map_err(InspectError::out_of_range)?;
        let raw = reader
            .read_sector(lba)
            .map_err(|error| InspectError::io(format!("读取 LBA{lba} 失败: {error}")))?;
        if raw.len() != SECTOR {
            return Err(InspectError::io(format!(
                "LBA{lba} 返回 {}B，预期 {SECTOR}B",
                raw.len()
            )));
        }

        let mut partition_boot = None;
        let mut partition_boot_issue = None;
        if request.mode != AdvancedInspectMode::Raw {
            if let Some(partition) = context.partition_for_lba(lba) {
                if partition.start_sector == lba {
                    partition_boot = Some(raw.clone());
                } else {
                    match reader.read_sector(partition.start_sector) {
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
        let protocol_view = if lba <= u64::from(crate::common::METADATA_LAST_LBA) {
            let lba32 = u32::try_from(lba)
                .map_err(|_| InspectError::out_of_range(format!("LBA{lba} 超出协议解析器范围")))?;
            Some(inspect::analyze_sector_with_context(
                lba32,
                &raw,
                &meta,
                Some(&context.protocol_image),
            ))
        } else {
            None
        };

        let fields = match protocol_view.as_ref() {
            Some(view) => materialize_protocol_fields(lba, &raw, &view.decoded, &view.fields)?,
            None => Vec::new(),
        };
        let mut item = AdvancedInspectItem {
            lba,
            regions,
            raw: raw.clone(),
            raw_sha256,
            raw_nonzero,
            decoded: None,
            decoded_sha256: None,
            method: None,
            decode_error: None,
            fields,
            notes: protocol_view
                .as_ref()
                .map(|view| view.notes.clone())
                .unwrap_or_default(),
            meta_text: None,
        };

        match request.mode {
            AdvancedInspectMode::Raw => {
                if let Some(dir) = &request.export_dir {
                    export_advanced_bytes(dir, lba, "raw", &raw)?;
                }
            }
            AdvancedInspectMode::Decode => {
                let decoded = if let Some(view) = protocol_view {
                    Ok((view.decoded, view.method))
                } else {
                    decode_sector(&context, &meta, lba, &raw, partition_boot.as_deref())
                };
                match decoded {
                    Ok((decoded, method)) => {
                        item.decoded_sha256 = Some(crate::sha256::sha256_hex(&decoded));
                        item.method = Some(method);
                        if let Some(dir) = &request.export_dir {
                            export_advanced_bytes(dir, lba, "decoded", &decoded)?;
                        }
                        item.decoded = Some(decoded);
                    }
                    Err(error) if request.fail_soft_decode => {
                        item.decode_error = Some(error.to_string());
                        item.method = Some("raw-only".into());
                    }
                    Err(error) => return Err(error),
                }
            }
            AdvancedInspectMode::Meta => {
                let text = sector_meta_text(
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

    let topology = super::inspect_tree::build_inspect_topology(&context);
    Ok(AdvancedInspectWorkspace {
        source,
        meta,
        mode: request.mode,
        items,
        export_dir: request.export_dir.clone(),
        topology,
    })
}

fn run_evidence_source(
    mut evidence: EvidenceSource,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    let identity = evidence.identity().clone();
    let mut meta = InspectMeta {
        device_id: identity.device_id,
        vid: identity.vid,
        pid: identity.pid,
        size_bytes: identity.size_bytes,
        onlyid: identity.onlyid,
    };
    if let Some(device_id) = request.device_id_override.as_ref() {
        meta.device_id = Some(device_id.clone());
    }
    let context = crate::inspect_target::InspectDiskContext::new(
        evidence.protocol().to_vec(),
        meta.device_id.clone(),
        evidence.total_sectors(),
    );
    let source = evidence.source_label().to_string();
    run_advanced_source(source, meta, context, request, &mut evidence)
}

pub fn load_backup_advanced_inspect(
    path: &Path,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    run_evidence_source(EvidenceSource::open_backup(path)?, request)
}

pub fn load_disk_advanced_inspect(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    run_evidence_source(EvidenceSource::open_disk(runner, disk)?, request)
}

#[cfg(test)]
mod advanced_tests {
    use super::*;
    use std::io;

    use crate::common::METADATA_IMAGE_LEN;

    struct MemoryReader {
        sectors: Vec<Vec<u8>>,
    }

    impl SectorReader for MemoryReader {
        fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            self.sectors
                .get(lba as usize)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "missing sector"))
        }
    }

    #[test]
    fn sector_reader_range_is_checked_and_lossless() {
        let mut reader = MemoryReader {
            sectors: vec![vec![0x11; SECTOR], vec![0x22; SECTOR], vec![0x33; SECTOR]],
        };
        let range = reader.read_range(1, 2).unwrap();
        assert_eq!(range.len(), 2 * SECTOR);
        assert_eq!(&range[..SECTOR], vec![0x22; SECTOR].as_slice());
        assert_eq!(&range[SECTOR..], vec![0x33; SECTOR].as_slice());

        reader.sectors[2].truncate(SECTOR - 1);
        let error = reader.read_range(2, 1).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(error.to_string().contains("511B"));
    }

    #[test]
    fn decoder_registry_fails_closed_outside_registered_regions() {
        let context =
            crate::inspect_target::InspectDiskContext::new(vec![0; METADATA_IMAGE_LEN], None, 4096);
        let meta = InspectMeta::default();
        assert_eq!(DECODER_REGISTRY[0], InspectDecoderKind::Protocol);
        let (_, method) = decode_sector(&context, &meta, 0, &[0; SECTOR], None).unwrap();
        assert!(!method.is_empty());

        let error = decode_sector(&context, &meta, 100, &[0; SECTOR], None).unwrap_err();
        assert_eq!(error.kind(), InspectErrorKind::Decode);
        assert!(error.message().contains("不属于已注册 decoder"), "{error}");
    }

    #[test]
    fn field_range_is_absolute_and_cross_sector_capable() {
        let range = AbsoluteByteRange {
            start: 100 * SECTOR as u64 + 0x1f0,
            end_exclusive: 101 * SECTOR as u64 + 0x30,
        };
        assert_eq!(range.start_lba(), 100);
        assert_eq!(range.end_lba(), 101);
        assert!(range.spans_sectors());
        assert_eq!(range.len(), 64);
    }

    #[test]
    fn protocol_fields_preserve_raw_decoded_and_absolute_range() {
        let lba = 4u64;
        let mut raw = vec![0u8; SECTOR];
        raw[8..12].copy_from_slice(&[1, 2, 3, 4]);
        let mut decoded = raw.clone();
        decoded[8..12].copy_from_slice(&[5, 6, 7, 8]);
        let fields = vec![crate::inspect::SectorField {
            start: 8,
            end: 12,
            label: "test".into(),
            value: "value".into(),
            style: crate::inspect::FieldStyle::Identity,
            group: Some("group".into()),
            children: Vec::new(),
        }];
        let materialized = materialize_protocol_fields(lba, &raw, &decoded, &fields).unwrap();
        assert_eq!(materialized.len(), 1);
        let field = &materialized[0];
        assert_eq!(field.range.start, 4 * SECTOR as u64 + 8);
        assert_eq!(field.range.end_exclusive, 4 * SECTOR as u64 + 12);
        assert_eq!(field.range.len(), 4);
        assert_eq!(field.raw, [1, 2, 3, 4]);
        assert_eq!(field.decoded, [5, 6, 7, 8]);
        assert_eq!(field.field_type, InspectFieldType::Identity);
        assert_eq!(field.status, InspectFieldStatus::Known);
        assert_eq!(field.group.as_deref(), Some("group"));
    }

    #[test]
    fn sector_inspector_decode_failure_preserves_raw_without_weakening_strict_decode() {
        let context =
            crate::inspect_target::InspectDiskContext::new(vec![0; METADATA_IMAGE_LEN], None, 4096);
        let meta = InspectMeta::default();
        let mut sectors = vec![vec![0; SECTOR]; 101];
        sectors[100] = vec![0x5a; SECTOR];
        let mut reader = MemoryReader {
            sectors: sectors.clone(),
        };
        let fail_soft = AdvancedInspectRequest {
            mode: AdvancedInspectMode::Decode,
            lbas: vec![100],
            export_dir: None,
            device_id_override: None,
            fail_soft_decode: true,
        };
        let workspace = run_advanced_source(
            "memory".into(),
            meta.clone(),
            context.clone(),
            &fail_soft,
            &mut reader,
        )
        .unwrap();
        let item = &workspace.items[0];
        assert_eq!(item.raw, vec![0x5a; SECTOR]);
        assert!(item.decoded.is_none());
        assert!(item
            .decode_error
            .as_deref()
            .is_some_and(|error| { error.contains("不属于已注册 decoder") }));

        let strict = AdvancedInspectRequest {
            fail_soft_decode: false,
            ..fail_soft
        };
        let mut reader = MemoryReader { sectors };
        let error =
            run_advanced_source("memory".into(), meta, context, &strict, &mut reader).unwrap_err();
        assert_eq!(error.kind(), InspectErrorKind::Decode);
        assert!(error.message().contains("不属于已注册 decoder"), "{error}");
    }

    #[test]
    fn inspect_reader_boundary_is_read_only_and_reads_only_requested_raw_sectors() {
        struct AuditReader {
            reads: Vec<u64>,
        }

        impl SectorReader for AuditReader {
            fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
                self.reads.push(lba);
                Ok(vec![(lba & 0xff) as u8; SECTOR])
            }
        }

        let context =
            crate::inspect_target::InspectDiskContext::new(vec![0; METADATA_IMAGE_LEN], None, 4096);
        let request = AdvancedInspectRequest {
            mode: AdvancedInspectMode::Raw,
            lbas: vec![100, 3_000],
            export_dir: None,
            device_id_override: None,
            fail_soft_decode: false,
        };
        let mut reader = AuditReader { reads: Vec::new() };
        let workspace = run_advanced_source(
            "audit-reader".into(),
            InspectMeta::default(),
            context,
            &request,
            &mut reader,
        )
        .unwrap();

        assert_eq!(reader.reads, vec![100, 3_000]);
        assert_eq!(
            workspace
                .items
                .iter()
                .map(|item| item.lba)
                .collect::<Vec<_>>(),
            vec![100, 3_000]
        );
        assert_eq!(workspace.items[0].raw[0], 100);
        assert_eq!(workspace.items[1].raw[0], (3_000 & 0xff) as u8);
    }

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
        assert_eq!(
            parse_advanced_lbas("", "2").unwrap_err().kind(),
            InspectErrorKind::InvalidRequest
        );
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
