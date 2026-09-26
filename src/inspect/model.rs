use super::*;

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
    pub status: SectorFieldStatus,
    pub transform: Option<FieldTransform>,
}

impl SectorField {
    pub fn with_status(mut self, status: SectorFieldStatus) -> Self {
        self.status = status;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChild {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct SectorView {
    pub lba: u32,
    pub raw: Vec<u8>,
    pub decoded: Vec<u8>,
    pub method: String,
    pub fields: Vec<SectorField>,
    pub notes: Vec<String>,
    pub parse_state: InspectParseState,
    pub diagnostics: Vec<InspectDiagnostic>,
}

pub(super) fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

pub(super) fn field(
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
        status: SectorFieldStatus::Known,
        transform: None,
    }
}

pub(super) fn grouped_field(
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

pub(super) fn field_with_children(
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

pub(super) fn ptype_name(t: u32) -> &'static str {
    match t {
        1 => "Boot",
        2 => "Share",
        4 => "Encrypt",
        _ => "未知",
    }
}

pub(super) fn mbr_type_name(t: u8) -> &'static str {
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

pub(super) fn human_bytes(v: u64) -> String {
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

pub(super) fn c_string_bytes(b: &[u8]) -> &[u8] {
    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    &b[..end]
}

pub(super) fn decode_gbk(b: &[u8]) -> Option<String> {
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

pub(super) fn text_value(b: &[u8]) -> String {
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

pub(super) fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn parse_mbr(decoded: &[u8], fields: &mut Vec<SectorField>, notes: &mut Vec<String>) {
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

pub(super) fn raw_sector(raw: &[u8]) -> Option<&[u8; SECTOR]> {
    raw.try_into().ok()
}

pub(super) fn protocol_sector(protocol_image: Option<&[u8]>, lba: usize) -> Option<&[u8; SECTOR]> {
    let image = protocol_image?;
    let start = lba.checked_mul(SECTOR)?;
    image.get(start..start + SECTOR)?.try_into().ok()
}

pub(super) fn slot_value<const N: usize>(slot: &crate::protocol::types::CStringSlot<N>) -> String {
    text_value(slot.value())
}

pub(super) fn profile_field(
    start: usize,
    end: usize,
    label: &str,
    state: impl std::fmt::Debug,
) -> SectorField {
    field(start, end, label, format!("{state:?}"), FieldStyle::Flag)
}

pub(super) fn pass_info_fields(base: usize, pass: &PassInfo, group: &str) -> Vec<SectorField> {
    let mut out = Vec::new();
    let mut version = grouped_field(
        base,
        base + 2,
        group,
        "版本",
        format!("0x{:04X}", pass.version),
        FieldStyle::Flag,
    );
    version.transform = Some(FieldTransform::XorByte {
        offset: 0,
        mask: 0x88,
    });
    out.push(version);
    let values = [
        ("交换区强制改密", pass.force_change_share),
        ("交换区最大错误次数", pass.max_share_password_errors),
        ("交换区当前错误次数", pass.current_share_password_errors),
        ("保密区强制改密", pass.force_change_encrypt),
        ("保密区最大错误次数", pass.max_encrypt_password_errors),
        ("保密区当前错误次数", pass.current_encrypt_password_errors),
        ("免密标志", pass.no_password_set),
        ("免密跳过 IP 检查", pass.no_password_no_check_ip),
        ("取消密码复杂性验证", pass.no_usb_check_password_safe),
        ("重置 FileKey", pass.reset_file_key),
        ("交换区备份提示周期", pass.share_backup_prompt_period),
        ("保密区备份提示周期", pass.encrypt_backup_prompt_period),
    ];
    for (index, (label, value)) in values.into_iter().enumerate() {
        let mut field = grouped_field(
            base + 2 + index,
            base + 3 + index,
            group,
            label,
            value.to_string(),
            FieldStyle::Flag,
        );
        if matches!(index, 1 | 4) {
            field.transform = Some(FieldTransform::XorByte {
                offset: 0,
                mask: 0x88,
            });
        }
        out.push(field);
    }
    out
}

pub(super) fn legacy_old_hash(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_le_bytes(word))
    })
}

pub(super) fn legacy_key_field(base: usize, group: String, entry: &EdpfEntry64) -> SectorField {
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

pub(super) fn elabel_field(start: usize, body: &[u8]) -> SectorField {
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

pub(super) fn edpf64_fields(base: usize, index: usize, entry: &EdpfEntry64) -> Vec<SectorField> {
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

pub(super) fn edpf96_fields(base: usize, index: usize, entry: &EdpfEntry96) -> Vec<SectorField> {
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
