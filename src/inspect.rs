//! 只读扇区检查器：把物理盘与备份镜像统一成同一套“解密 → 结构解析 → 字段高亮”视图。
//! 本模块不做任何写盘动作，也不负责提权；CLI 只在物理盘来源时请求裸盘读取权限。

use std::collections::BTreeSet;

use encoding_rs::GBK;

use crate::common::SECTOR;
use crate::crypto::crc32_bare;
use crate::diskio::BackupMeta;
use crate::protocol::{
    edpf::{EdpfEntry64, EdpfEntry96, PassInfo},
    lba0, lba1, lba10, lba11, lba12, lba2, lba3, lba4, lba5, lba6, lba7, lba8, lba9,
    profile::{
        DeptLayout, HostHardinfoSource, Lba10Eesi, Lba11Capacity, Lba12Mode, Lba7EntryCount,
        Lba7PassinfoVersion, Lba8UsbOnlyInfo,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldStyle {
    Magic,
    Text,
    Identity,
    Address,
    Size,
    Flag,
    Checksum,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorField {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub value: String,
    pub style: FieldStyle,
    pub group: Option<String>,
    pub children: Vec<FieldChild>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChild {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct InspectMeta {
    pub device_id: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub size_bytes: Option<u64>,
    pub onlyid: Option<String>,
}

impl InspectMeta {
    pub fn from_backup_meta(meta: &BackupMeta) -> Self {
        Self {
            device_id: Some(meta.device_id.clone()),
            vid: Some(meta.vid.clone()),
            pid: Some(meta.pid.clone()),
            size_bytes: meta.secs.and_then(|s| s.checked_mul(SECTOR as u64)),
            onlyid: meta.onlyid.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SectorView {
    pub lba: u32,
    pub raw: Vec<u8>,
    pub decoded: Vec<u8>,
    pub method: String,
    pub fields: Vec<SectorField>,
    pub notes: Vec<String>,
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

fn field(
    start: usize,
    end: usize,
    label: impl Into<String>,
    value: impl Into<String>,
    style: FieldStyle,
) -> SectorField {
    SectorField {
        start,
        end,
        label: label.into(),
        value: value.into(),
        style,
        group: None,
        children: Vec::new(),
    }
}

fn grouped_field(
    start: usize,
    end: usize,
    group: impl Into<String>,
    label: impl Into<String>,
    value: impl Into<String>,
    style: FieldStyle,
) -> SectorField {
    let mut f = field(start, end, label, value, style);
    f.group = Some(group.into());
    f
}

fn field_with_children(
    start: usize,
    end: usize,
    group: impl Into<String>,
    label: impl Into<String>,
    children: Vec<FieldChild>,
    style: FieldStyle,
) -> SectorField {
    let mut f = grouped_field(start, end, group, label, "", style);
    f.children = children;
    f
}

fn ptype_name(t: u32) -> &'static str {
    match t {
        1 => "Boot",
        2 => "Share",
        4 => "Encrypt",
        _ => "未知",
    }
}

fn mbr_type_name(t: u8) -> &'static str {
    match t {
        0x00 => "空",
        0x01 => "FAT12",
        0x04 | 0x06 | 0x0e => "FAT16",
        0x07 => "NTFS/exFAT",
        0x0b | 0x0c => "FAT32",
        0xee => "GPT Protective",
        0xef => "EFI",
        _ => "其他",
    }
}

fn human_bytes(v: u64) -> String {
    if v >= 1_000_000_000 {
        format!("{:.2} GB", v as f64 / 1_000_000_000.0)
    } else if v >= 1_000_000 {
        format!("{:.2} MB", v as f64 / 1_000_000.0)
    } else if v >= 1_000 {
        format!("{:.2} KB", v as f64 / 1_000.0)
    } else {
        format!("{} B", v)
    }
}

fn c_string_bytes(b: &[u8]) -> &[u8] {
    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    &b[..end]
}

fn decode_gbk(b: &[u8]) -> Option<String> {
    let (decoded, had_errors) = GBK.decode_without_bom_handling(b);
    let value = if had_errors {
        // 旧 LLGB 固定长度字段偶尔会截断一个 GBK 双字节字符。旧实现的
        // `iconv -c` 会丢弃坏字节但保留可读前缀；这里保持相同语义。
        decoded.replace('\u{FFFD}', "")
    } else {
        decoded.into_owned()
    };
    (!value.is_empty()).then_some(value)
}

fn text_value(b: &[u8]) -> String {
    let b = c_string_bytes(b);
    if b.is_empty() {
        return "<空>".into();
    }
    if b.iter().all(|x| (0x20..=0x7e).contains(x)) {
        return String::from_utf8_lossy(b).into_owned();
    }
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    if let Some(s) = decode_gbk(b) {
        if !s.is_empty() {
            return s;
        }
    }
    let ascii: String = b
        .iter()
        .map(|&x| {
            if (0x20..=0x7e).contains(&x) {
                x as char
            } else {
                '·'
            }
        })
        .collect();
    format!(
        "{}  [hex:{}]",
        ascii,
        b.iter().map(|x| format!("{:02X}", x)).collect::<String>()
    )
}

fn crc_key(meta: &InspectMeta) -> Option<(u32, [u8; 4])> {
    let did = meta.device_id.as_deref()?;
    let crc = crc32_bare(did.as_bytes());
    Some((crc, crc.to_le_bytes()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_mbr(decoded: &[u8], fields: &mut Vec<SectorField>, notes: &mut Vec<String>) {
    if decoded.len() < SECTOR {
        return;
    }
    let sig_ok = decoded[0x1fe..0x200] == [0x55, 0xaa];
    fields.push(field(
        0x1fe,
        0x200,
        "MBR 签名",
        if sig_ok {
            "55 AA ✓"
        } else {
            "签名异常 ✗"
        },
        FieldStyle::Magic,
    ));
    let mut empty_slots = Vec::new();
    for i in 0..4 {
        let off = 0x1be + i * 16;
        let ptype = decoded[off + 4];
        let start = u32_at(decoded, off + 8).unwrap_or(0);
        let secs = u32_at(decoded, off + 12).unwrap_or(0);
        if ptype == 0 && start == 0 && secs == 0 {
            empty_slots.push(i + 1);
            continue;
        }
        let group = format!("分区 P{}", i + 1);
        fields.push(grouped_field(
            off + 4,
            off + 5,
            group.clone(),
            "分区类型",
            format!("0x{:02X} {}", ptype, mbr_type_name(ptype)),
            FieldStyle::Flag,
        ));
        fields.push(grouped_field(
            off + 8,
            off + 12,
            group.clone(),
            "起始 LBA",
            start.to_string(),
            FieldStyle::Address,
        ));
        fields.push(grouped_field(
            off + 12,
            off + 16,
            group,
            "大小",
            format!(
                "{} 扇区 / {}",
                secs,
                human_bytes(secs as u64 * SECTOR as u64)
            ),
            FieldStyle::Size,
        ));
    }
    notes.push("MBR 分区表位于 +0x1BE..+0x1FD。".into());
    if !empty_slots.is_empty() {
        notes.push(format!(
            "空分区槽位: {}。",
            empty_slots
                .into_iter()
                .map(|i| format!("P{i}"))
                .collect::<Vec<_>>()
                .join("、")
        ));
    }
}

fn raw_sector(raw: &[u8]) -> Option<&[u8; SECTOR]> {
    raw.try_into().ok()
}

fn protocol_sector(protocol_image: Option<&[u8]>, lba: usize) -> Option<&[u8; SECTOR]> {
    let image = protocol_image?;
    let start = lba.checked_mul(SECTOR)?;
    image.get(start..start + SECTOR)?.try_into().ok()
}

fn meta_onlyid(meta: &InspectMeta) -> Option<u32> {
    let text = meta.onlyid.as_deref()?;
    text.parse::<u32>()
        .ok()
        .or_else(|| text.parse::<i32>().ok().map(|value| value as u32))
}

fn slot_value<const N: usize>(slot: &crate::protocol::types::CStringSlot<N>) -> String {
    text_value(slot.value())
}

fn profile_field(
    start: usize,
    end: usize,
    label: &str,
    state: impl std::fmt::Debug,
) -> SectorField {
    field(start, end, label, format!("{state:?}"), FieldStyle::Flag)
}

fn pass_info_fields(base: usize, pass: &PassInfo, group: &str) -> Vec<SectorField> {
    let mut out = Vec::new();
    out.push(grouped_field(
        base,
        base + 2,
        group,
        "版本",
        format!("0x{:04X}", pass.version),
        FieldStyle::Flag,
    ));
    let values = [
        ("交换区强制改密", pass.force_change_share),
        ("交换区最大错误次数", pass.max_share_password_errors),
        ("交换区当前错误次数", pass.current_share_password_errors),
        ("保密区强制改密", pass.force_change_encrypt),
        ("保密区最大错误次数", pass.max_encrypt_password_errors),
        ("保密区当前错误次数", pass.current_encrypt_password_errors),
        ("免密标志", pass.no_password_set),
        ("免密跳过 IP 检查", pass.no_password_no_check_ip),
        ("免密安全策略", pass.no_usb_check_password_safe),
        ("重置 FileKey", pass.reset_file_key),
        ("交换区备份提示周期", pass.share_backup_prompt_period),
        ("保密区备份提示周期", pass.encrypt_backup_prompt_period),
    ];
    for (index, (label, value)) in values.into_iter().enumerate() {
        out.push(grouped_field(
            base + 2 + index,
            base + 3 + index,
            group,
            label,
            value.to_string(),
            FieldStyle::Flag,
        ));
    }
    out
}

fn legacy_old_hash(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_le_bytes(word))
    })
}

fn legacy_key_field(base: usize, group: String, entry: &EdpfEntry64) -> SectorField {
    let known = b"0000aaaa";
    let mut children = vec![
        FieldChild {
            label: "pwd_crc".into(),
            value: format!("0x{:08X}", entry.user_key_crc),
        },
        FieldChild {
            label: "key_crc".into(),
            value: format!("0x{:08X}", entry.file_key_crc),
        },
    ];
    if crc32_bare(known) == entry.user_key_crc {
        let hash = legacy_old_hash(known);
        let lo = u32::from_le_bytes(entry.encrypted_file_key[..4].try_into().unwrap()) ^ hash;
        let hi = u32::from_le_bytes(entry.encrypted_file_key[4..].try_into().unwrap()) ^ hash;
        let mut key8 = Vec::with_capacity(8);
        key8.extend_from_slice(&lo.to_le_bytes());
        key8.extend_from_slice(&hi.to_le_bytes());
        children.push(FieldChild {
            label: "key8".into(),
            value: key8.iter().map(|byte| format!("{byte:02x}")).collect(),
        });
        children.push(FieldChild {
            label: "key8 CRC".into(),
            value: if crc32_bare(&key8) == entry.file_key_crc {
                "✓".into()
            } else {
                "✗".into()
            },
        });
    } else {
        children.push(FieldChild {
            label: "raw".into(),
            value: hex_bytes(&entry.encrypted_file_key),
        });
    }
    field_with_children(
        base + 0x30,
        base + 0x40,
        group,
        "密钥信息",
        children,
        FieldStyle::Identity,
    )
}

fn elabel_field(start: usize, body: &[u8]) -> SectorField {
    let text = c_string_bytes(body);
    let field_len = text.len();
    let body = text.strip_prefix(b"<ELABEL>").unwrap_or(text);
    let children = body
        .split(|byte| *byte == b'|')
        .filter(|part| !part.is_empty())
        .map(|part| {
            if let Some(eq) = part.iter().position(|byte| *byte == b'=') {
                FieldChild {
                    label: text_value(&part[..eq]),
                    value: {
                        let value = text_value(&part[eq + 1..]);
                        let value = value.strip_prefix("*^$@").unwrap_or(&value);
                        if value.is_empty() {
                            "<空>".into()
                        } else {
                            value.to_string()
                        }
                    },
                }
            } else {
                FieldChild {
                    label: "值".into(),
                    value: text_value(part),
                }
            }
        })
        .collect();
    field_with_children(
        start,
        start + field_len,
        "[ELABEL]",
        "",
        children,
        FieldStyle::Text,
    )
}

fn edpf64_fields(base: usize, index: usize, entry: &EdpfEntry64) -> Vec<SectorField> {
    let group = format!("Entry[{index}]");
    vec![
        grouped_field(
            base,
            base + 4,
            group.clone(),
            "EDPF magic",
            "EDPF",
            FieldStyle::Magic,
        ),
        grouped_field(
            base + 0x04,
            base + 0x08,
            group.clone(),
            "版本",
            format!("0x{:08X}", entry.version),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x08,
            base + 0x0c,
            group.clone(),
            "分区数量",
            entry.partition_count.to_string(),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x0c,
            base + 0x10,
            group.clone(),
            "类型",
            format!(
                "{} ({})",
                ptype_name(entry.partition_type),
                entry.partition_type
            ),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x10,
            base + 0x14,
            group.clone(),
            "NeedDisturb",
            entry.need_disturb.to_string(),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x14,
            base + 0x18,
            group.clone(),
            "NeedEncrypt",
            entry.need_encrypt.to_string(),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x18,
            base + 0x20,
            group.clone(),
            "起始 LBA",
            entry.start_sector.to_string(),
            FieldStyle::Address,
        ),
        grouped_field(
            base + 0x20,
            base + 0x28,
            group.clone(),
            "扇区字节",
            entry.sector_size.to_string(),
            FieldStyle::Size,
        ),
        grouped_field(
            base + 0x28,
            base + 0x30,
            group.clone(),
            "大小",
            format!(
                "{} B / {}",
                entry.partition_size,
                human_bytes(entry.partition_size)
            ),
            FieldStyle::Size,
        ),
        legacy_key_field(base, group, entry),
    ]
}

fn edpf96_fields(base: usize, index: usize, entry: &EdpfEntry96) -> Vec<SectorField> {
    let group = format!("Entry[{index}]");
    vec![
        grouped_field(
            base,
            base + 4,
            group.clone(),
            "EDPF magic",
            "EDPF",
            FieldStyle::Magic,
        ),
        grouped_field(
            base + 0x04,
            base + 0x08,
            group.clone(),
            "版本",
            format!("0x{:08X}", entry.version),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x08,
            base + 0x0c,
            group.clone(),
            "分区数量",
            entry.partition_count.to_string(),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x0c,
            base + 0x10,
            group.clone(),
            "类型",
            format!(
                "{} ({})",
                ptype_name(entry.partition_type),
                entry.partition_type
            ),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x10,
            base + 0x14,
            group.clone(),
            "NeedDisturb",
            entry.need_disturb.to_string(),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x14,
            base + 0x18,
            group.clone(),
            "NeedEncrypt",
            entry.need_encrypt.to_string(),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x18,
            base + 0x20,
            group.clone(),
            "起始 LBA",
            entry.start_sector.to_string(),
            FieldStyle::Address,
        ),
        grouped_field(
            base + 0x20,
            base + 0x28,
            group.clone(),
            "扇区字节",
            entry.sector_size.to_string(),
            FieldStyle::Size,
        ),
        grouped_field(
            base + 0x28,
            base + 0x30,
            group.clone(),
            "大小",
            format!(
                "{} B / {}",
                entry.partition_size,
                human_bytes(entry.partition_size)
            ),
            FieldStyle::Size,
        ),
        grouped_field(
            base + 0x30,
            base + 0x34,
            group.clone(),
            "UserKeyCRC",
            format!("0x{:08X}", entry.user_key_crc),
            FieldStyle::Checksum,
        ),
        grouped_field(
            base + 0x34,
            base + 0x38,
            group.clone(),
            "FileKeyCRC",
            format!("0x{:08X}", entry.file_key_crc),
            FieldStyle::Checksum,
        ),
        grouped_field(
            base + 0x38,
            base + 0x48,
            group.clone(),
            "加密 FileKey",
            hex_bytes(&entry.encrypted_file_key),
            FieldStyle::Identity,
        ),
        grouped_field(
            base + 0x48,
            base + 0x58,
            group.clone(),
            "兼容 Key",
            hex_bytes(&entry.compatibility_key),
            FieldStyle::Identity,
        ),
        grouped_field(
            base + 0x58,
            base + 0x59,
            group.clone(),
            "EncryptMode",
            entry.encrypt_mode.to_string(),
            FieldStyle::Flag,
        ),
        grouped_field(
            base + 0x59,
            base + 0x60,
            group,
            "保留字节",
            hex_bytes(&entry.reserved),
            FieldStyle::Flag,
        ),
    ]
}

fn infer_lba7(
    raw: &[u8; SECTOR],
    device_crc: u32,
) -> Option<(lba7::Lba7View, Lba7EntryCount, Lba7PassinfoVersion)> {
    let mut matches = Vec::new();
    for entry_count in [Lba7EntryCount::TwoEntry, Lba7EntryCount::ThreeEntry] {
        for passinfo in [
            Lba7PassinfoVersion::LegacyV0064,
            Lba7PassinfoVersion::CurrentV0206,
        ] {
            if let Ok(view) = lba7::parse_lba7(raw, device_crc, entry_count, passinfo) {
                matches.push((view, entry_count, passinfo));
            }
        }
    }
    (matches.len() == 1).then(|| matches.remove(0))
}

fn infer_lba8(
    raw: &[u8; SECTOR],
    device_crc: u32,
    onlyid: Option<u32>,
) -> Option<(
    lba8::Lba8View,
    Vec<Lba8UsbOnlyInfo>,
    Vec<HostHardinfoSource>,
)> {
    let mut matches = Vec::new();
    for usb_only_info in [
        Lba8UsbOnlyInfo::Current,
        Lba8UsbOnlyInfo::Transitional2019,
        Lba8UsbOnlyInfo::StrictLegacyAbsent,
    ] {
        for host_hardinfo_source in [
            HostHardinfoSource::CurrentZero,
            HostHardinfoSource::LegacyHostIdentity,
        ] {
            let context = lba8::Lba8Context {
                usb_only_info,
                host_hardinfo_source,
                main_onlyid: onlyid,
            };
            if let Ok(view) = lba8::parse_lba8(raw, device_crc, context) {
                matches.push((view, usb_only_info, host_hardinfo_source));
            }
        }
    }
    let first = matches.first()?.0.clone();
    let mut usb_profiles = Vec::new();
    let mut host_profiles = Vec::new();
    for (_, usb, host) in matches {
        if !usb_profiles.contains(&usb) {
            usb_profiles.push(usb);
        }
        if !host_profiles.contains(&host) {
            host_profiles.push(host);
        }
    }
    Some((first, usb_profiles, host_profiles))
}

fn infer_lba11(
    raw: &[u8; SECTOR],
    meta: &InspectMeta,
) -> Option<(lba11::Lba11View, Vec<Lba11Capacity>)> {
    let vid = meta.vid.as_deref()?;
    let pid = meta.pid.as_deref()?;
    let size = meta.size_bytes?;
    let mut matches = Vec::new();
    for profile in [Lba11Capacity::DiskSize, Lba11Capacity::RepairChs] {
        if let Ok(view) = lba11::parse_lba11(raw, vid, pid, size, profile) {
            matches.push((view, profile));
        }
    }
    let first = matches.first()?.0.clone();
    Some((
        first,
        matches.into_iter().map(|(_, profile)| profile).collect(),
    ))
}

fn infer_lba12(raw: &[u8; SECTOR], device_crc: u32) -> Option<(lba12::Lba12View, Vec<Lba12Mode>)> {
    let mut matches = Vec::new();
    for mode in [
        Lba12Mode::LegacyV0064,
        Lba12Mode::Mode1,
        Lba12Mode::Mode2,
        Lba12Mode::Mode3,
    ] {
        if let Ok(view) = lba12::parse_lba12(raw, device_crc, mode) {
            matches.push((view, mode));
        }
    }
    let first = matches.first()?.0.clone();
    Some((first, matches.into_iter().map(|(_, mode)| mode).collect()))
}

pub fn analyze_sector(lba: u32, raw: &[u8], meta: &InspectMeta) -> SectorView {
    analyze_sector_with_context(lba, raw, meta, None)
}

pub fn analyze_sector_with_context(
    lba: u32,
    raw: &[u8],
    meta: &InspectMeta,
    protocol_image: Option<&[u8]>,
) -> SectorView {
    if raw.len() != SECTOR {
        return SectorView {
            lba,
            raw: raw.to_vec(),
            decoded: raw.to_vec(),
            method: format!("RAW（短读：{}B，应为 {}B）", raw.len(), SECTOR),
            fields: vec![],
            notes: vec!["扇区长度异常，停止结构化解析。".into()],
        };
    }
    let raw_sector = raw_sector(raw).expect("length checked");
    let mut fields = Vec::new();
    let mut notes = Vec::new();
    let mut decoded = raw.to_vec();

    let method = match lba {
        0 => match lba0::parse_lba0(raw_sector) {
            Ok(view) => {
                fields.push(profile_field(
                    0x000,
                    0x190,
                    "MBR 引导代码 profile",
                    view.bootstrap,
                ));
                fields.push(profile_field(
                    0x1a0,
                    0x1a4,
                    "SectorSize overlay",
                    view.sector_size,
                ));
                fields.push(field(
                    0x190,
                    0x1a0,
                    "兼容 backing",
                    hex_bytes(view.compat_190_19f.bytes()),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1a4,
                    0x1b5,
                    "兼容 backing",
                    hex_bytes(view.compat_1a4_1b4.bytes()),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1b5,
                    0x1b8,
                    "旧版消息指针",
                    hex_bytes(&view.message_pointers),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1b8,
                    0x1bc,
                    "MBR 磁盘签名",
                    format!("0x{:08X}", view.disk_signature),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x1bc,
                    0x1be,
                    "MBR 保留字",
                    hex_bytes(view.mbr_reserved.bytes()),
                    FieldStyle::Flag,
                ));
                parse_mbr(raw_sector, &mut fields, &mut notes);
                notes.push("字段语义来自 protocol::lba0::parse_lba0；bootstrap、SectorSize overlay 与 MBR 字段按独立 profile 解释。".into());
                "canonical protocol::lba0".into()
            }
            Err(error) => {
                notes.push(format!("canonical LBA0 parser 拒绝该扇区: {error}"));
                "RAW（LBA0 canonical parser 未通过）".into()
            }
        },
        1 => match lba1::parse_lba1(raw_sector) {
            Ok(view) => {
                match view.header {
                    lba1::GptHeaderState::Absent => {
                        fields.push(field(0, SECTOR, "GPT profile", "absent", FieldStyle::Flag));
                    }
                    lba1::GptHeaderState::Enabled(header) => {
                        fields.push(field(
                            0x00,
                            0x08,
                            "GPT signature",
                            "EFI PART",
                            FieldStyle::Magic,
                        ));
                        fields.push(field(
                            0x08,
                            0x0c,
                            "GPT revision",
                            format!("0x{:08X}", header.revision),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x0c,
                            0x10,
                            "GPT header size",
                            header.header_size.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x10,
                            0x14,
                            "GPT header CRC32",
                            format!("0x{:08X}", header.header_crc32),
                            FieldStyle::Checksum,
                        ));
                        fields.push(field(
                            0x18,
                            0x20,
                            "当前 LBA",
                            header.current_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x20,
                            0x28,
                            "备份 GPT LBA",
                            header.backup_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x28,
                            0x30,
                            "首个可用 LBA",
                            header.first_usable_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x30,
                            0x38,
                            "末个可用 LBA",
                            header.last_usable_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x38,
                            0x48,
                            "磁盘 GUID",
                            hex_bytes(&header.disk_guid),
                            FieldStyle::Identity,
                        ));
                        fields.push(field(
                            0x48,
                            0x50,
                            "分区表首 LBA",
                            header.partition_entries_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x50,
                            0x54,
                            "GPT entry 数量",
                            header.entry_count.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x54,
                            0x58,
                            "GPT entry 大小",
                            header.entry_size.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x58,
                            0x5c,
                            "分区数组 CRC32",
                            format!("0x{:08X}", header.partition_array_crc32),
                            FieldStyle::Checksum,
                        ));
                    }
                }
                notes.push("字段语义与 CRC/几何校验来自 protocol::lba1::parse_lba1。".into());
                "canonical protocol::lba1".into()
            }
            Err(error) => {
                notes.push(format!("canonical LBA1 parser 拒绝该扇区: {error}"));
                "RAW（LBA1 canonical parser 未通过）".into()
            }
        },
        2 => {
            let profile = if raw.iter().all(|byte| *byte == 0) {
                crate::protocol::profile::GptLayout::Absent
            } else {
                crate::protocol::profile::GptLayout::Enabled
            };
            match lba2::parse_lba2(raw_sector, profile) {
                Ok(view) => {
                    fields.push(profile_field(
                        0,
                        SECTOR,
                        "GPT partition profile",
                        view.profile,
                    ));
                    if let Some(entries) = view.entries {
                        for (index, entry) in entries.iter().enumerate() {
                            let base = index * 0x80;
                            match entry {
                                lba2::GptEntry::Unused { residual } => fields.push(grouped_field(
                                    base,
                                    base + 0x80,
                                    format!("GPT Entry[{index}]"),
                                    "未使用 entry/backing",
                                    hex_bytes(residual.bytes()),
                                    FieldStyle::Flag,
                                )),
                                lba2::GptEntry::Used(partition) => {
                                    let group = format!("GPT Entry[{index}]");
                                    fields.push(grouped_field(
                                        base,
                                        base + 0x10,
                                        group.clone(),
                                        "Type GUID",
                                        hex_bytes(&partition.type_guid),
                                        FieldStyle::Identity,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x10,
                                        base + 0x20,
                                        group.clone(),
                                        "Unique GUID",
                                        hex_bytes(&partition.unique_guid),
                                        FieldStyle::Identity,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x20,
                                        base + 0x28,
                                        group.clone(),
                                        "首 LBA",
                                        partition.first_lba.to_string(),
                                        FieldStyle::Address,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x28,
                                        base + 0x30,
                                        group.clone(),
                                        "末 LBA",
                                        partition.last_lba.to_string(),
                                        FieldStyle::Address,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x30,
                                        base + 0x38,
                                        group.clone(),
                                        "属性",
                                        format!("0x{:016X}", partition.attributes),
                                        FieldStyle::Flag,
                                    ));
                                    let name = String::from_utf16_lossy(&partition.name_utf16)
                                        .trim_end_matches('\0')
                                        .to_string();
                                    fields.push(grouped_field(
                                        base + 0x38,
                                        base + 0x80,
                                        group,
                                        "名称",
                                        if name.is_empty() {
                                            "<空>".into()
                                        } else {
                                            name
                                        },
                                        FieldStyle::Text,
                                    ));
                                }
                            }
                        }
                    }
                    notes.push("字段语义来自 protocol::lba2::parse_lba2；非零扇区按 enabled GPT profile 严格解析。".into());
                    "canonical protocol::lba2".into()
                }
                Err(error) => {
                    notes.push(format!("canonical LBA2 parser 拒绝该扇区: {error}"));
                    "RAW（LBA2 canonical parser 未通过）".into()
                }
            }
        }
        3 => {
            let view = lba3::parse_lba3(raw_sector);
            fields.push(profile_field(
                0,
                SECTOR,
                "制造商 metadata profile",
                view.profile,
            ));
            fields.push(field(
                0,
                SECTOR,
                "制造商私有 opaque payload",
                format!(
                    "SHA-256={}",
                    crate::sha256::sha256_hex(view.payload.bytes())
                ),
                FieldStyle::Identity,
            ));
            notes.push("LBA3 在 EDP 协议边界只做 profile 识别与逐字节 preserve；不虚构 Phison 私有子字段。".into());
            "canonical protocol::lba3".into()
        }
        4 => match lba4::parse_lba4(raw_sector, lba4::Lba4Context::default()) {
            Ok(view) => {
                decoded = view.reader.bytes().to_vec();
                fields.push(field(
                    0x000,
                    0x018,
                    "labelOnlyId",
                    format!("{} (0x{:08X})", view.onlyid_text, view.onlyid),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x018,
                    0x01c,
                    "OnlyIdXor8",
                    format!("0x{:08X}", view.node.onlyid_guard),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x01c,
                    0x020,
                    "OnllyID2Nd",
                    format!("0x{:08X}", view.node.second_key),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x020,
                    0x034,
                    "HSerialCRC[5]",
                    view.node
                        .hserial
                        .iter()
                        .map(|v| format!("{v:08X}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x034,
                    0x035,
                    "SingleUsbFlg",
                    view.node.single_usb.to_string(),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x035,
                    0x039,
                    "MyHardinfo",
                    format!("0x{:08X}", view.node.host_hardinfo),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x039,
                    0x03d,
                    "NewLabFlag",
                    hex_bytes(&view.node.new_lab_flag),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x03d,
                    0x041,
                    "Version",
                    format!("0x{:08X}", view.node.version),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x041,
                    0x045,
                    "sector tuple",
                    hex_bytes(&view.node.sector_tuple),
                    FieldStyle::Flag,
                ));
                let reader_flags = view.reader_flags();
                let wire_flags = view.wire_flags();
                fields.push(field(0x045, 0x046, "bDataToServer", format!("reader=0x{:02X}; wire=0x{:02X}; producer=需按 writer 判定 (profile unknown)", reader_flags.data_to_server, wire_flags.data_to_server), FieldStyle::Flag));
                fields.push(field(0x046, 0x047, "bConnetServer", format!("reader=0x{:02X}; wire=0x{:02X}; producer=需按 writer 判定 (profile unknown)", reader_flags.connect_server, wire_flags.connect_server), FieldStyle::Flag));
                fields.push(field(
                    0x047,
                    0x1fc,
                    "representation backing",
                    if view.backing_is_raw_zero() {
                        "raw-zero profile".into()
                    } else {
                        "rolling representation carrier".into()
                    },
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1fc,
                    0x200,
                    "trailing LLGB",
                    hex_bytes(&view.trailing_magic),
                    FieldStyle::Magic,
                ));
                notes.push("字段结构来自 protocol::lba4::parse_lba4。Inspect 不凭身份形态猜 writer：encoding/second-key/HSerial/host-hardinfo profile 保持 Unknown，因此 producer flag 不伪判。".into());
                "canonical protocol::lba4（writer provenance 未强猜）".into()
            }
            Err(error) => {
                notes.push(format!("canonical LBA4 parser 拒绝该扇区: {error}"));
                "RAW（LBA4 canonical parser 未通过）".into()
            }
        },
        5 => {
            let view = lba5::parse_lba5(raw_sector);
            fields.push(field(
                0,
                SECTOR,
                "写保护探测 scratch 区",
                format!(
                    "内容本身不解析；opaque preserve；SHA-256={}",
                    crate::sha256::sha256_hex(view.payload.bytes())
                ),
                FieldStyle::Flag,
            ));
            notes.push("LBA5 内容不解析；EDP 只消费写回结果判断 ERROR_WRITE_PROTECT，扇区本身原样保留。当前样本是否全零不构成协议要求。".into());
            "canonical protocol::lba5（写保护探测）".into()
        }
        6 => match lba6::parse_lba6(raw_sector) {
            Ok(view) => {
                decoded = view.decoded().to_vec();
                let dept = match &view.dept {
                    lba6::DeptInline::Short(slot) => format!("short: {}", slot_value(slot)),
                    lba6::DeptInline::Join59(bytes) => {
                        format!("join59 inline: {}", text_value(bytes))
                    }
                    lba6::DeptInline::Join60(bytes) => {
                        format!("join60 inline: {}", text_value(bytes))
                    }
                };
                fields.push(field(0x000, 0x040, "Dept inline", dept, FieldStyle::Text));
                fields.push(field(
                    0x040,
                    0x050,
                    "SAFE6 模板",
                    hex_bytes(&view.template_040_04f),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x050,
                    0x070,
                    "User",
                    slot_value(&view.user),
                    FieldStyle::Text,
                ));
                fields.push(field(
                    0x070,
                    0x080,
                    "Autonum",
                    slot_value(&view.autonum),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x080,
                    0x0c0,
                    "Office",
                    slot_value(&view.office),
                    FieldStyle::Text,
                ));
                fields.push(field(
                    0x0c0,
                    0x100,
                    "模板区",
                    hex_bytes(&view.template_0c0_0ff),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x100,
                    0x104,
                    "device_id CRC32",
                    format!("0x{:08X}", view.device_crc),
                    FieldStyle::Checksum,
                ));
                fields.push(field(
                    0x104,
                    0x108,
                    "CRC guard",
                    format!("0x{:08X}", view.device_crc_guard),
                    FieldStyle::Checksum,
                ));
                fields.push(field(
                    0x108,
                    0x188,
                    "兼容模板区",
                    hex_bytes(&view.template_108_187),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x188,
                    0x1c0,
                    "Label",
                    slot_value(&view.label),
                    FieldStyle::Text,
                ));
                fields.push(field(
                    0x1c0,
                    0x1d0,
                    "m_usbGSerial 槽",
                    slot_value(&view.gserial),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x1d0,
                    0x1e0,
                    "BeiZhu 槽",
                    slot_value(&view.beizhu),
                    FieldStyle::Text,
                ));
                match &view.mbr_underlay {
                    lba6::MbrUnderlay::ZeroBacking => fields.push(field(
                        0x1e0,
                        0x1ee,
                        "legacy MBR snapshot",
                        "zero-underlay",
                        FieldStyle::Flag,
                    )),
                    lba6::MbrUnderlay::LegacySnapshot(snapshot) => fields.push(field(
                        0x1e0,
                        0x1ee,
                        "legacy MBR snapshot",
                        format!(
                            "start_lba={} sectors={} prefix={}",
                            snapshot.start_lba,
                            snapshot.sector_count,
                            hex_bytes(&snapshot.prefix)
                        ),
                        FieldStyle::Address,
                    )),
                    lba6::MbrUnderlay::Unknown(bytes) => fields.push(field(
                        0x1e0,
                        0x1ee,
                        "legacy MBR snapshot",
                        format!("unknown opaque {}", hex_bytes(bytes.bytes())),
                        FieldStyle::Flag,
                    )),
                }
                fields.push(field(
                    0x1ee,
                    0x1f0,
                    "兼容 tail",
                    hex_bytes(&view.compatibility_tail),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1f0,
                    0x1f4,
                    "m_encrypt",
                    format!(
                        "{} (0x{:08X})",
                        view.encrypt_generation_flag, view.encrypt_generation_flag
                    ),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1f4,
                    0x1fc,
                    "zero tail",
                    hex_bytes(&view.zero_tail),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1fc,
                    0x200,
                    "校验和",
                    format!("0x{:08X}", view.checksum),
                    FieldStyle::Checksum,
                ));
                notes.push(format!(
                    "Dept profile={}；字段语义来自 protocol::lba6::parse_lba6。",
                    view.dept_profile().as_str()
                ));
                "canonical protocol::lba6 (SAFE6 rolling XOR)".into()
            }
            Err(error) => {
                notes.push(format!("canonical LBA6 parser 拒绝该扇区: {error}"));
                "RAW（LBA6 canonical parser 未通过）".into()
            }
        },
        7 => {
            if let Some((crc, _)) = crc_key(meta) {
                match infer_lba7(raw_sector, crc) {
                    Some((view, entry_profile, pass_profile)) => {
                        decoded = view.stored_plain().to_vec();
                        for (index, entry) in view.entries_0_1.iter().enumerate() {
                            fields.extend(edpf64_fields(index * 0x40, index, entry));
                        }
                        if let lba7::Entry2::Present(entry) = view.entry2 {
                            fields.extend(edpf64_fields(0x80, 2, &entry));
                        } else {
                            fields.push(field(
                                0x80,
                                0xc0,
                                "Entry[2]",
                                "absent/zero",
                                FieldStyle::Flag,
                            ));
                        }
                        fields.extend(pass_info_fields(0xc0, &view.pass_info, "PassInfo"));
                        fields.push(field(
                            0xce,
                            0x200,
                            "post-table zero",
                            "306B writer-zero",
                            FieldStyle::Flag,
                        ));
                        if let Some(mode) = view.official_partition_mode() {
                            notes.push(format!("官方模式={} ({})", mode as u8, mode.ui_name_zh()));
                        } else {
                            notes.push(
                                "分区类型序列不属于当前已证实的四个官方模式，保持未分类。".into(),
                            );
                        }
                        notes.push(format!(
                            "profile: entry_count={} passinfo={}",
                            entry_profile.as_str(),
                            pass_profile.as_str()
                        ));
                        "canonical protocol::lba7 (rolling XOR)".into()
                    }
                    None => {
                        notes.push(
                            "无法从已知 entry-count/pass-info profile 中唯一解析 LBA7；拒绝猜测。"
                                .into(),
                        );
                        "RAW（LBA7 profile 未唯一确定）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA7）".into()
            }
        }
        8 => {
            if let Some((crc, _)) = crc_key(meta) {
                match infer_lba8(raw_sector, crc, meta_onlyid(meta)) {
                    Some((view, usb_profiles, host_profiles)) => {
                        decoded = view.mixed_plain().to_vec();
                        fields.push(field(0x000, 0x004, "LLGB magic", "LLGB", FieldStyle::Magic));
                        fields.push(field(
                            0x004,
                            0x008,
                            "logical length",
                            view.logical_length.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x008,
                            0x00c,
                            "tool version",
                            hex_bytes(&view.tool_version),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x00c,
                            0x010,
                            "lab version",
                            format!("0x{:08X}", view.lab_version),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x010,
                            0x014,
                            "writeTime",
                            view.write_time.to_string(),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x014,
                            0x018,
                            "HDSerialInfo/MyHardinfo",
                            format!("0x{:08X}", view.host_hardinfo),
                            FieldStyle::Identity,
                        ));
                        fields.push(field(
                            0x018,
                            0x01e,
                            "MacInfo",
                            hex_bytes(&view.mac_info),
                            FieldStyle::Identity,
                        ));
                        let usb = match &view.usb_only_info {
                            lba8::UsbOnlyInfo::Current(bytes) => {
                                format!("current {}", text_value(bytes))
                            }
                            lba8::UsbOnlyInfo::Transitional(bytes) => {
                                format!("transitional {}", text_value(bytes))
                            }
                            lba8::UsbOnlyInfo::StrictLegacyAbsent => "strict-legacy-absent".into(),
                        };
                        fields.push(field(
                            0x01e,
                            0x02e,
                            "UsbOnlyInfo",
                            usb,
                            FieldStyle::Identity,
                        ));
                        fields.push(field(
                            0x02e,
                            0x03e,
                            "UsbOnlyInfo suffix",
                            hex_bytes(&view.usb_only_suffix),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x03e,
                            0x040,
                            "ELABEL offset",
                            format!("0x{:04X}", view.elab_offset),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x040,
                            0x080,
                            "reserved header",
                            hex_bytes(&view.reserved_header),
                            FieldStyle::Flag,
                        ));
                        fields.push(elabel_field(0x080, &view.elabel_body));
                        if !view.encrypted_backing.is_empty() {
                            let start = 0x080 + view.elabel_body.len() + 1;
                            fields.push(field(
                                start,
                                view.encrypted_len(),
                                "encrypted backing",
                                hex_bytes(&view.encrypted_backing),
                                FieldStyle::Flag,
                            ));
                        }
                        if !view.tail_backing.is_empty() {
                            fields.push(field(
                                view.encrypted_len(),
                                SECTOR,
                                "raw tail backing",
                                hex_bytes(&view.tail_backing),
                                FieldStyle::Flag,
                            ));
                        }
                        notes.push(format!(
                            "profile: UsbOnlyInfo={} host-hardinfo={}; encrypted_len={}B",
                            usb_profile.as_str(),
                            host_profile.as_str(),
                            view.encrypted_len()
                        ));
                        format!("canonical protocol::lba8 A6B0 前 {}B", view.encrypted_len())
                    }
                    None => {
                        notes.push("LBA8 已解出候选但无法在已知 UsbOnlyInfo/host-hardinfo profile 中唯一归类；拒绝强猜。".into());
                        "RAW（LBA8 profile 未唯一确定）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA8）".into()
            }
        }
        9 => {
            if let Some((crc, _)) = crc_key(meta) {
                let companion =
                    protocol_sector(protocol_image, 6).and_then(|raw6| lba6::parse_lba6(raw6).ok());
                let (dept_profile, long_user) = companion
                    .as_ref()
                    .map(|view| (view.dept_profile(), view.has_long_user()))
                    .unwrap_or((DeptLayout::Short, false));
                if companion.is_none() {
                    notes.push("未提供完整 LBA0–12 上下文；LBA9 使用 short/non-long-user 兼容解析，仅用于单扇区 API。CLI/TUI 会提供 LBA6 上下文。".into());
                }
                match lba9::parse_lba9(raw_sector, crc, dept_profile, long_user) {
                    Ok(view) => {
                        decoded = view.decoded().to_vec();
                        fields.push(field(
                            0x080,
                            0x100,
                            "Dept continuation",
                            slot_value(&view.dept_continuation),
                            FieldStyle::Text,
                        ));
                        match &view.eetu {
                            lba9::EetuState::Absent => fields.push(field(
                                0x000,
                                0x080,
                                "EETU",
                                "absent-zero",
                                FieldStyle::Flag,
                            )),
                            lba9::EetuState::Present(eetu) => {
                                fields.push(field(
                                    0x000,
                                    0x004,
                                    "EETU magic",
                                    "EETU",
                                    FieldStyle::Magic,
                                ));
                                fields.push(field(
                                    0x004,
                                    0x00c,
                                    "EETU 开始时间 (ullBTime)",
                                    eetu.begin_time.to_string(),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x00c,
                                    0x014,
                                    "EETU 结束时间 (ullETime)",
                                    eetu.end_time.to_string(),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x014,
                                    0x018,
                                    "EETU 使用次数 (useCount)",
                                    if eetu.use_count == u32::MAX {
                                        "无限（0xFFFFFFFF）".into()
                                    } else {
                                        eetu.use_count.to_string()
                                    },
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x018,
                                    0x07e,
                                    "EETU reverse backing",
                                    hex_bytes(eetu.reverse.bytes()),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x07e,
                                    0x080,
                                    "EETU zero tail",
                                    hex_bytes(&eetu.zero_tail),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::EetuState::Unknown(bytes) => fields.push(field(
                                0x000,
                                0x080,
                                "EETU",
                                format!("unknown backing {}", hex_bytes(bytes.bytes())),
                                FieldStyle::Flag,
                            )),
                        }
                        match &view.upper {
                            lba9::UpperPayload::Zero => fields.push(field(
                                0x100,
                                0x200,
                                "upper payload",
                                "zero",
                                FieldStyle::Flag,
                            )),
                            lba9::UpperPayload::Sapf(sapf) => {
                                fields.push(field(
                                    0x100,
                                    0x104,
                                    "SAPF magic",
                                    "SAPF",
                                    FieldStyle::Magic,
                                ));
                                fields.push(field(
                                    0x108,
                                    0x109,
                                    "partition type",
                                    format!("0x{:02X}", sapf.partition.partition_type),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x10c,
                                    0x110,
                                    "SAPF 起始 LBA",
                                    sapf.partition.start_lba.to_string(),
                                    FieldStyle::Address,
                                ));
                                fields.push(field(
                                    0x110,
                                    0x114,
                                    "SAPF 扇区数",
                                    sapf.partition.sector_count.to_string(),
                                    FieldStyle::Size,
                                ));
                                fields.push(field(
                                    0x114,
                                    0x120,
                                    "SAPF compatibility",
                                    hex_bytes(&sapf.compatibility),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x120,
                                    0x200,
                                    "SAPF tail backing",
                                    hex_bytes(sapf.tail.bytes()),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::UpperPayload::LongUser { continuation, tail } => {
                                fields.push(field(
                                    0x100,
                                    0x180,
                                    "User continuation",
                                    slot_value(continuation),
                                    FieldStyle::Text,
                                ));
                                fields.push(field(
                                    0x180,
                                    0x200,
                                    "User tail backing",
                                    hex_bytes(tail.bytes()),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::UpperPayload::Eppe(eppe) => {
                                fields.push(field(
                                    0x180,
                                    0x184,
                                    "EPPE magic",
                                    "EPPE",
                                    FieldStyle::Magic,
                                ));
                                fields.push(field(
                                    0x184,
                                    0x188,
                                    "最小密码长度",
                                    eppe.min_length.to_string(),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x100,
                                    0x180,
                                    "EPPE prefix backing",
                                    hex_bytes(eppe.prefix.bytes()),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x188,
                                    0x200,
                                    "EPPE zero tail",
                                    hex_bytes(&eppe.zero_tail),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::UpperPayload::Unknown(bytes) => fields.push(field(
                                0x100,
                                0x200,
                                "upper payload",
                                format!("unknown backing {}", hex_bytes(bytes.bytes())),
                                FieldStyle::Flag,
                            )),
                        }
                        notes.push(format!("字段语义来自 protocol::lba9::parse_lba9；Dept profile={}；EETU 时间字段由运行时与 time(NULL) 比较，useCount=0xFFFFFFFF 表示无限；旧称 reverse[104] 已拆分为 reverse backing[102] + zero tail[2]，语义状态 COMPLETE。", dept_profile.as_str()));
                        "canonical protocol::lba9".into()
                    }
                    Err(error) => {
                        notes.push(format!("canonical LBA9 parser 拒绝该扇区: {error}"));
                        "RAW（LBA9 canonical parser 未通过）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA9）".into()
            }
        }
        10 => {
            if let Some((crc, _)) = crc_key(meta) {
                let profile = if raw.iter().all(|byte| *byte == 0) {
                    Lba10Eesi::AbsentZero
                } else {
                    Lba10Eesi::EesiEnabled
                };
                match lba10::parse_lba10(raw_sector, crc, profile) {
                    Ok(lba10::Lba10View::Absent { .. }) => {
                        fields.push(field(
                            0,
                            0x80,
                            "EESI profile",
                            "absent-zero",
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x80,
                            0x200,
                            "preserve/ignore tail",
                            "当前全零；协议不赋予 payload 语义",
                            FieldStyle::Flag,
                        ));
                        "canonical protocol::lba10 absent-zero".into()
                    }
                    Ok(lba10::Lba10View::Enabled {
                        plain_prefix,
                        suspension_flag,
                        share_label,
                        encrypt_label,
                        extension,
                        tail,
                        ..
                    }) => {
                        decoded[..0x80].copy_from_slice(&plain_prefix);
                        fields.push(field(0x000, 0x004, "EESI magic", "EESI", FieldStyle::Magic));
                        fields.push(field(
                            0x004,
                            0x008,
                            "UsbSuspensionWnd flag",
                            format!("{} (0x{:08X})", suspension_flag, suspension_flag),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x008,
                            0x018,
                            "EESI 交换区卷标",
                            slot_value(&share_label),
                            FieldStyle::Text,
                        ));
                        fields.push(field(
                            0x018,
                            0x028,
                            "EESI 保密区卷标",
                            slot_value(&encrypt_label),
                            FieldStyle::Text,
                        ));
                        fields.push(field(
                            0x028,
                            0x080,
                            "caller extension",
                            hex_bytes(extension.bytes()),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x080,
                            0x200,
                            "preserve/ignore tail",
                            hex_bytes(tail.bytes()),
                            FieldStyle::Flag,
                        ));
                        notes.push("EESI +0x04 最新语义为 UsbSuspensionWnd flag；仅前 0x80B 属于 EESI，后 0x180B 为 preserve/ignore backing；交换区/保密区卷标分别进入 type2/type4 SetVolumeLabelA。".into());
                        "canonical protocol::lba10 EESI 前 0x80B".into()
                    }
                    Err(error) => {
                        notes.push(format!("canonical LBA10 parser 拒绝该扇区: {error}"));
                        "RAW（LBA10 canonical parser 未通过）".into()
                    }
                }
            } else if raw.iter().all(|byte| *byte == 0) {
                fields.push(field(0, SECTOR, "LBA10", "absent-zero", FieldStyle::Flag));
                "canonical LBA10 absent-zero（无需 device_id）".into()
            } else {
                "RAW（缺 device_id，无法解 LBA10）".into()
            }
        }
        11 => match infer_lba11(raw_sector, meta) {
            Some((view, profiles)) => {
                decoded[..0x100].copy_from_slice(&view.reconstruct()[..0x100]);
                decoded[0x100..].copy_from_slice(view.decoded_pdkb());
                fields.push(field(0x000, 0x004, "DRKB magic", "DRKB", FieldStyle::Magic));
                fields.push(field(
                    0x004,
                    0x100,
                    "random252",
                    hex_bytes(&view.random252),
                    FieldStyle::Identity,
                ));
                fields.push(field(0x100, 0x104, "PDKB magic", "PDKB", FieldStyle::Magic));
                fields.push(field(
                    0x104,
                    0x200,
                    "PDKB device_id",
                    slot_value(&view.uid),
                    FieldStyle::Identity,
                ));
                notes.push(format!("capacity profile 候选={}；选中容量={}B。若 DiskSize 与 repair-CHS 数值相同，两者可同时通过但盘面语义等价。", profiles.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(","), view.capacity_bytes));
                "canonical protocol::lba11 (DRKB + PDKB)".into()
            }
            None => "RAW（缺 VID/PID/容量或 LBA11 canonical parser 未通过）".into(),
        },
        12 => {
            if let Some((crc, _)) = crc_key(meta) {
                match infer_lba12(raw_sector, crc) {
                    Some((view, modes)) => {
                        decoded = view.decoded().to_vec();
                        for (index, entry) in view.entries.iter().enumerate() {
                            fields.extend(edpf96_fields(index * 0x60, index, entry));
                        }
                        fields.extend(pass_info_fields(0x120, &view.pass_info, "PassInfo"));
                        fields.push(field(
                            0x12e,
                            0x200,
                            "zero padding",
                            "210B writer-zero（位于整扇外层密文内）",
                            FieldStyle::Flag,
                        ));
                        notes.push(format!(
                            "wrapped-key mode 候选={}；外层始终是 device CRC 派生的整扇 A6B0。",
                            modes
                                .iter()
                                .map(|mode| mode.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        ));
                        "canonical protocol::lba12 (A6B0 整扇 512B)".into()
                    }
                    None => {
                        notes.push("无法在 legacy-v0064/mode1/mode2/mode3 中解析 LBA12；拒绝猜测 wrapped-key profile。".into());
                        "RAW（LBA12 canonical parser 未通过）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA12）".into()
            }
        }
        _ => "RAW（无已知结构解析器）".into(),
    };

    SectorView {
        lba,
        raw: raw.to_vec(),
        decoded,
        method,
        fields,
        notes,
    }
}

fn style_name(style: FieldStyle) -> &'static str {
    match style {
        FieldStyle::Magic => "魔数/签名",
        FieldStyle::Text => "文本",
        FieldStyle::Identity => "身份/密钥",
        FieldStyle::Address => "地址/LBA",
        FieldStyle::Size => "大小",
        FieldStyle::Flag => "类型/标志",
        FieldStyle::Checksum => "校验",
    }
}

fn paint(style: FieldStyle, text: &str, bad: bool) -> String {
    match style {
        FieldStyle::Magic => crate::ui::bold_cyan(text),
        FieldStyle::Text => crate::ui::cyan(text),
        FieldStyle::Identity => crate::ui::yellow(text),
        FieldStyle::Address => crate::ui::green(text),
        FieldStyle::Size => crate::ui::magenta(text),
        FieldStyle::Flag => crate::ui::yellow(text),
        FieldStyle::Checksum if bad => crate::ui::red(text),
        FieldStyle::Checksum => crate::ui::green(text),
    }
}

fn byte_style(fields: &[SectorField], idx: usize) -> Option<(FieldStyle, bool)> {
    fields
        .iter()
        .find(|f| idx >= f.start && idx < f.end)
        .map(|f| (f.style, f.value.contains('✗')))
}

/// 16B/行字段感知 hex。`raw_mode=true` 时不对加密态字节套用解密字段颜色，避免误导。
pub fn render_hex(view: &SectorView, raw_mode: bool) -> String {
    let data = if raw_mode { &view.raw } else { &view.decoded };
    let fields: &[SectorField] = if raw_mode { &[] } else { &view.fields };
    let mut out = String::new();
    out.push_str(&format!(
        "LBA{} {} hex ({}B)\n",
        view.lba,
        if raw_mode { "RAW" } else { "解码" },
        data.len()
    ));
    for (line_no, line) in data.chunks(16).enumerate() {
        let base = line_no * 16;
        out.push_str(&format!("  +0x{base:03X}: "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(&b) = line.get(i) {
                let token = format!("{b:02X}");
                if let Some((style, bad)) = byte_style(fields, base + i) {
                    out.push_str(&paint(style, &token, bad));
                } else {
                    out.push_str(&token);
                }
            } else {
                out.push_str("  ");
            }
            out.push(' ');
        }
        out.push(' ');
        for &b in line {
            out.push(if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            });
        }
        out.push('\n');
    }
    if !raw_mode && !view.fields.is_empty() {
        let styles: BTreeSet<FieldStyle> = view.fields.iter().map(|f| f.style).collect();
        out.push_str("  字段图例: ");
        let mut first = true;
        for s in styles {
            if !first {
                out.push_str(" · ");
            }
            first = false;
            out.push_str(&paint(s, style_name(s), false));
        }
        out.push('\n');
    }
    out
}

pub fn render_fields(view: &SectorView) -> String {
    if view.fields.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("  结构化字段:\n");
    let mut current_group: Option<&str> = None;
    for f in &view.fields {
        let group = f.group.as_deref();
        if group != current_group {
            if current_group.is_some() && group.is_some() {
                out.push('\n');
            }
            current_group = group;
            if let Some(group_name) = group {
                let (start, end) = view
                    .fields
                    .iter()
                    .filter(|candidate| candidate.group.as_deref() == Some(group_name))
                    .fold((usize::MAX, 0usize), |(min_start, max_end), candidate| {
                        (min_start.min(candidate.start), max_end.max(candidate.end))
                    });
                let range = format!("+0x{start:03X}..0x{:03X}", end.saturating_sub(1));
                out.push_str(&format!(
                    "    {}  {}\n",
                    crate::ui::bold_cyan(group_name),
                    crate::ui::dim(&range)
                ));
            }
        }
        let range = if f.end == f.start + 1 {
            format!("+0x{:03X}", f.start)
        } else {
            format!("+0x{:03X}..0x{:03X}", f.start, f.end.saturating_sub(1))
        };
        if !f.value.is_empty() {
            let indent = if group.is_some() { "      " } else { "    " };
            let prefix = format!(
                "{}{}  {}  ",
                indent,
                crate::ui::pad_to(&range, 18),
                crate::ui::pad_to(&f.label, 18)
            );
            let chunks = wrap_value(&f.value, 64);
            for (idx, chunk) in chunks.iter().enumerate() {
                if idx == 0 {
                    out.push_str(&prefix);
                } else {
                    out.push_str(&" ".repeat(6 + 18 + 2 + 18 + 2));
                }
                if chunk == "<空>" {
                    out.push_str(&crate::ui::dim(chunk));
                } else {
                    out.push_str(&paint(f.style, chunk, f.value.contains('✗')));
                }
                out.push('\n');
            }
        }
        if !f.children.is_empty() {
            let child_indent = if f.label.is_empty() {
                "      "
            } else {
                "        "
            };
            if !f.label.is_empty() {
                out.push_str(&format!(
                    "      {}  {}\n",
                    crate::ui::pad_to(&f.label, 12),
                    crate::ui::dim(&range)
                ));
            }
            let mut empty_labels = Vec::new();
            for child in &f.children {
                if child.value == "<空>" {
                    empty_labels.push(child.label.as_str());
                    continue;
                }
                let child_value = wrap_value(&child.value, 72);
                for (idx, chunk) in child_value.iter().enumerate() {
                    if idx == 0 {
                        out.push_str(&format!(
                            "{}{}  {}\n",
                            child_indent,
                            crate::ui::pad_to(&child.label, 12),
                            if chunk == "<空>" {
                                crate::ui::dim(chunk)
                            } else {
                                paint(f.style, chunk, chunk.contains('✗'))
                            }
                        ));
                    } else {
                        let rendered = if chunk == "<空>" {
                            crate::ui::dim(chunk)
                        } else {
                            paint(f.style, chunk, chunk.contains('✗'))
                        };
                        out.push_str(&format!(
                            "{}{}  {}\n",
                            child_indent,
                            " ".repeat(12),
                            rendered
                        ));
                    }
                }
            }
            if !empty_labels.is_empty() {
                out.push_str(&format!(
                    "{}{}  {}\n",
                    child_indent,
                    crate::ui::pad_to("空字段", 12),
                    crate::ui::dim(&empty_labels.join(" · "))
                ));
            }
        }
    }
    out
}

fn wrap_value(value: &str, max_chars: usize) -> Vec<String> {
    if value.chars().count() <= max_chars {
        return vec![value.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for token in value.split_whitespace() {
        let token_len = token.chars().count();
        if current_len > 0 && current_len + 1 + token_len > max_chars {
            lines.push(current);
            current = String::new();
            current_len = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            current_len += 1;
        }
        if token_len <= max_chars {
            current.push_str(token);
            current_len += token_len;
            continue;
        }
        for ch in token.chars() {
            if current_len == max_chars {
                lines.push(current);
                current = String::new();
                current_len = 0;
            }
            current.push(ch);
            current_len += 1;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

pub fn overview_line(view: &SectorView) -> String {
    let nz = view.raw.iter().filter(|&&b| b != 0).count();
    let head: String = view
        .raw
        .iter()
        .take(12)
        .map(|&b| {
            if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    format!(
        "LBA{:>2}  {:>3}/512  {:<12}  {}",
        view.lba, nz, head, view.method
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_legacy_gbk_keeps_readable_prefix() {
        let mut raw = b"Dept=*^$@".to_vec();
        raw.extend_from_slice(&[
            0xBD, 0xAD, 0xCB, 0xD5, 0xCA, 0xA1, 0xB5, 0xE7, 0xC1, 0xA6, 0xD3, 0xD0, 0xCF, 0xDE,
            0xB9, 0xAB, 0xCB, 0xBE, 0x2F, 0xBD,
        ]);
        let decoded = text_value(&raw);
        assert!(
            decoded.starts_with("Dept=*^$@江苏省电力有限公司/"),
            "{decoded}"
        );
        assert!(!decoded.contains("[hex:"), "{decoded}");
    }
}
