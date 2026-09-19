//! 只读扇区检查器：把物理盘与备份镜像统一成同一套“解密 → 结构解析 → 字段高亮”视图。
//! 本模块不做任何写盘动作，也不负责提权；CLI 只在物理盘来源时请求裸盘读取权限。

use std::collections::BTreeSet;

use encoding_rs::GBK;

use crate::common::SECTOR;
use crate::crypto::{a6b0_full, crc32_bare, lba6_checksum, lba6_decode, xor_rolling};
use crate::diskio::BackupMeta;
use crate::sectors::EDPF_TABLE_LEN;

const LLGB_FALLBACK_LEN: usize = 0x170;

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

fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
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

fn c_field_end(b: &[u8], start: usize, end: usize) -> usize {
    let slice = &b[start..end];
    match slice.iter().position(|&x| x == 0) {
        Some(pos) => start + pos + 1,
        None => end,
    }
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

fn fixed_c_slot_value(bytes: &[u8]) -> String {
    let nul = bytes.iter().position(|byte| *byte == 0);
    let text_end = nul.unwrap_or(bytes.len());
    let text = text_value(&bytes[..text_end]);
    let tail_start = nul.map_or(bytes.len(), |index| index + 1);
    let tail = &bytes[tail_start..];
    if tail.iter().any(|byte| *byte != 0) {
        format!("{text}；NUL 后原始字节={}", hex_bytes(tail))
    } else {
        text
    }
}

fn lba7_k0(crc: u32) -> u32 {
    (crc & 0xffff) ^ (crc >> 16)
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

fn lba4_serial(raw: &[u8]) -> Option<(u32, String, usize, usize)> {
    let max = raw.len().min(64);
    let b = &raw[..max];
    for i in 0..b.len().saturating_sub(6) {
        if b.get(i..i + 3) != Some(b"$$$") {
            continue;
        }
        let mut j = i + 3;
        if b.get(j) == Some(&b'-') {
            j += 1;
        }
        let digits_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > digits_start && b.get(j..j + 3) == Some(b"$$$") {
            let text = std::str::from_utf8(&b[i + 3..j]).ok()?.to_string();
            let bits = if text.starts_with('-') {
                text.parse::<i32>().ok()? as u32
            } else {
                text.parse::<u32>().ok()?
            };
            return Some((bits, text, i, j + 3));
        }
    }
    None
}

fn decode_lba4(raw: &[u8]) -> Option<(Vec<u8>, u32, String, u32, usize, usize)> {
    let (serial, text, hs, he) = lba4_serial(raw)?;
    let k0 = (serial & 0xffff) ^ (serial >> 16);
    let mut dec = raw.to_vec();
    if raw.len() >= 0x18 {
        let region = &raw[0x18..];
        let x = xor_rolling(region, k0);
        dec[0x18..].copy_from_slice(&x);
        // Short-form LBA4 leaves the whole extension gap physically unwritten.
        // Only that *region-level* all-zero condition is special.  A zero byte
        // inside an actually written rolling-XOR region is valid ciphertext and
        // must still be decrypted.
        if raw.len() >= 0x1fc && raw[0x47..0x1fc].iter().all(|byte| *byte == 0) {
            dec[0x47..0x1fc].fill(0);
        }
    }
    Some((dec, serial, text, k0, hs, he))
}

fn parse_lba6(
    raw: &[u8],
    meta: &InspectMeta,
    fields: &mut Vec<SectorField>,
    notes: &mut Vec<String>,
) -> Vec<u8> {
    let dec = lba6_decode(raw);
    let label_end = c_field_end(&dec, 0x000, 0x040);
    let user_end = c_field_end(&dec, 0x050, 0x070);
    let serial_end = c_field_end(&dec, 0x070, 0x080);
    let label = text_value(&dec[0x000..label_end]);
    let label = label.strip_prefix("*^$@").unwrap_or(&label).to_string();
    fields.push(field(0x000, label_end, "标签", label, FieldStyle::Text));
    fields.push(field(
        0x050,
        user_end,
        "用户",
        text_value(&dec[0x050..user_end]),
        FieldStyle::Text,
    ));
    fields.push(field(
        0x070,
        serial_end,
        "序列",
        text_value(&dec[0x070..serial_end]),
        FieldStyle::Identity,
    ));
    if let Some((crc, _)) = crc_key(meta) {
        let stored = u32_at(&dec, 0x100).unwrap_or(0);
        fields.push(field(
            0x100,
            0x104,
            "device_id CRC32",
            format!(
                "0x{stored:08X} / 期望 0x{crc:08X} {}",
                if stored == crc { "✓" } else { "✗" }
            ),
            FieldStyle::Checksum,
        ));
        let stored2 = u32_at(&dec, 0x104).unwrap_or(0);
        let want2 = crc.wrapping_shl(1);
        fields.push(field(
            0x104,
            0x108,
            "CRC32<<1",
            format!(
                "0x{stored2:08X} / 期望 0x{want2:08X} {}",
                if stored2 == want2 { "✓" } else { "✗" }
            ),
            FieldStyle::Checksum,
        ));
    }
    if let Some(pos) = dec[0x188..0x1c0].windows(6).position(|w| w == b"!SAFE6") {
        fields.push(field(
            0x188,
            0x188 + pos + 6,
            "SAFE6 标识",
            text_value(&dec[0x188..0x188 + pos + 6]),
            FieldStyle::Magic,
        ));
    }
    fields.push(field(
        0x1c0,
        0x1d0,
        "m_usbGSerial 槽",
        fixed_c_slot_value(&dec[0x1c0..0x1d0]),
        FieldStyle::Identity,
    ));
    fields.push(field(
        0x1d0,
        0x1e0,
        "BeiZhu 槽",
        fixed_c_slot_value(&dec[0x1d0..0x1e0]),
        FieldStyle::Text,
    ));
    fields.push(field(
        0x1e0,
        0x1f0,
        "模板/版本扩展区",
        hex_bytes(&dec[0x1e0..0x1f0]),
        FieldStyle::Flag,
    ));
    let encrypt = u32_at(&dec, 0x1f0).unwrap_or(0);
    fields.push(field(
        0x1f0,
        0x1f4,
        "m_encrypt",
        format!("{encrypt} (0x{encrypt:08X})"),
        FieldStyle::Flag,
    ));
    let stored = u32_at(raw, 0x1fc).unwrap_or(0);
    let calc = lba6_checksum(&raw[..0x1fc]);
    fields.push(field(
        0x1fc,
        0x200,
        "校验和",
        format!(
            "0x{stored:08X} / 计算 0x{calc:08X} {}",
            if stored == calc { "✓" } else { "✗" }
        ),
        FieldStyle::Checksum,
    ));
    notes.push(
        "LBA6：前 508B 使用 rolling XOR K0=0x4DAA，最后 4B 校验和保持明文。当前 writer 把 0x1C0..0x1CF / 0x1D0..0x1DF 分别作为固定 16B GSerial / BeiZhu 槽；NUL 后字节不能按独立字段解释。".into(),
    );
    dec
}

fn old_hash(data: &[u8]) -> u32 {
    let mut total = 0u32;
    for chunk in data.chunks(4) {
        let mut tmp = [0u8; 4];
        tmp[..chunk.len()].copy_from_slice(chunk);
        total = total.wrapping_add(u32::from_le_bytes(tmp));
    }
    total
}

fn parse_edpf(dec: &[u8], stride: usize, fields: &mut Vec<SectorField>, notes: &mut Vec<String>) {
    let max = dec.len() / stride;
    let mut count = 0usize;
    for idx in 0..max.min(8) {
        let base = idx * stride;
        let e = &dec[base..base + stride];
        if e.get(..4) != Some(b"EDPF") {
            break;
        }
        count += 1;
        let ptype = u32_at(e, 0x0c).unwrap_or(0);
        let active = u32_at(e, 0x10).unwrap_or(0);
        let enc = u32_at(e, 0x14).unwrap_or(0);
        let start = u64_at(e, 0x18).unwrap_or(0);
        let bps = u64_at(e, 0x20).unwrap_or(0);
        let size = u64_at(e, 0x28).unwrap_or(0);
        let group = format!("Entry[{idx}]");
        fields.push(grouped_field(
            base,
            base + 4,
            group.clone(),
            "EDPF magic",
            "EDPF",
            FieldStyle::Magic,
        ));
        fields.push(grouped_field(
            base + 0x0c,
            base + 0x10,
            group.clone(),
            "类型",
            format!("{} ({ptype})", ptype_name(ptype)),
            FieldStyle::Flag,
        ));
        fields.push(grouped_field(
            base + 0x10,
            base + 0x18,
            group.clone(),
            "状态",
            format!("active={active}  enc={enc}"),
            FieldStyle::Flag,
        ));
        fields.push(grouped_field(
            base + 0x18,
            base + 0x20,
            group.clone(),
            "起始 LBA",
            start.to_string(),
            FieldStyle::Address,
        ));
        fields.push(grouped_field(
            base + 0x20,
            base + 0x28,
            group.clone(),
            "扇区字节",
            bps.to_string(),
            FieldStyle::Size,
        ));
        fields.push(grouped_field(
            base + 0x28,
            base + 0x30,
            group.clone(),
            "大小",
            format!("{size} B / {}", human_bytes(size)),
            FieldStyle::Size,
        ));
        let key_end = if stride >= 0x60 { 0x48 } else { 0x40 };
        if e[0x30..key_end].iter().any(|&b| b != 0) {
            let children = if stride == 0x40 && key_end == 0x40 {
                let pwd_crc = u32_at(e, 0x30).unwrap_or(0);
                let key_crc = u32_at(e, 0x34).unwrap_or(0);
                let wrapped = &e[0x38..0x40];
                let known = b"0000aaaa";
                let mut children = vec![
                    FieldChild {
                        label: "pwd_crc".into(),
                        value: format!("0x{pwd_crc:08X}"),
                    },
                    FieldChild {
                        label: "key_crc".into(),
                        value: format!("0x{key_crc:08X}"),
                    },
                ];
                if crc32_bare(known) == pwd_crc {
                    let h = old_hash(known);
                    let lo = u32_at(wrapped, 0).unwrap_or(0) ^ h;
                    let hi = u32_at(wrapped, 4).unwrap_or(0) ^ h;
                    let mut key8 = Vec::with_capacity(8);
                    key8.extend_from_slice(&lo.to_le_bytes());
                    key8.extend_from_slice(&hi.to_le_bytes());
                    children.push(FieldChild {
                        label: "key8".into(),
                        value: key8
                            .iter()
                            .map(|x| format!("{:02x}", x))
                            .collect::<String>(),
                    });
                    children.push(FieldChild {
                        label: "key8 CRC".into(),
                        value: if crc32_bare(&key8) == key_crc {
                            "✓".into()
                        } else {
                            "✗".into()
                        },
                    });
                } else {
                    children.push(FieldChild {
                        label: "raw".into(),
                        value: e[0x30..key_end]
                            .iter()
                            .map(|x| format!("{:02x}", x))
                            .collect::<String>(),
                    });
                }
                children
            } else {
                vec![FieldChild {
                    label: "Hash".into(),
                    value: e[0x30..key_end]
                        .iter()
                        .map(|x| format!("{:02x}", x))
                        .collect::<String>(),
                }]
            };
            fields.push(field_with_children(
                base + 0x30,
                base + key_end,
                group,
                "密钥信息",
                children,
                FieldStyle::Identity,
            ));
        }
    }
    notes.push(format!(
        "EDPF：检测到 {count} 条记录，entry stride=0x{stride:X}。"
    ));
}

fn parse_llgb(dec: &[u8], fields: &mut Vec<SectorField>, notes: &mut Vec<String>) {
    let mut cursor = 0usize;
    let mut count = 0usize;
    while cursor < dec.len() {
        let Some(rel) = dec[cursor..].iter().position(|&b| b == b'<') else {
            break;
        };
        let start = cursor + rel;
        if dec.get(start + 1) == Some(&b'/') {
            cursor = start + 2;
            continue;
        }
        let Some(gt_rel) = dec[start..].iter().position(|&b| b == b'>') else {
            break;
        };
        let gt = start + gt_rel;
        let tag = text_value(&dec[start + 1..gt]);
        let close = dec[gt + 1..]
            .windows(2)
            .position(|w| w == b"</")
            .map(|r| gt + 1 + r)
            .or_else(|| {
                dec[gt + 1..]
                    .windows(2)
                    .position(|w| w == [0, 0])
                    .map(|r| gt + 1 + r)
            })
            .unwrap_or(dec.len());
        let body = &dec[gt + 1..close];
        let mut parts = Vec::new();
        let mut part_start = 0usize;
        let mut p = 0usize;
        while p + 1 < body.len() {
            if body[p] == b'|' && body[p + 1] == b'|' {
                if p > part_start {
                    parts.push(text_value(&body[part_start..p]));
                }
                p += 2;
                part_start = p;
            } else {
                p += 1;
            }
        }
        if part_start < body.len() {
            parts.push(text_value(&body[part_start..]));
        }
        let display_end = if dec.get(close..close + 2) == Some(b"</") {
            dec[close..]
                .iter()
                .position(|&b| b == b'>')
                .map(|r| close + r + 1)
                .unwrap_or(close)
        } else {
            close
        };
        let children = parts
            .into_iter()
            .map(|part| {
                if let Some((key, value)) = part.split_once('=') {
                    let value = value.trim();
                    let value = value.strip_prefix("*^$@").unwrap_or(value);
                    FieldChild {
                        label: key.trim().to_string(),
                        value: if value.is_empty() {
                            "<空>".into()
                        } else {
                            value.to_string()
                        },
                    }
                } else {
                    FieldChild {
                        label: "值".into(),
                        value: part,
                    }
                }
            })
            .collect();
        fields.push(field_with_children(
            start,
            display_end.min(dec.len()),
            format!("[{tag}]"),
            "",
            children,
            FieldStyle::Text,
        ));
        count += 1;
        cursor = display_end.saturating_add(1);
    }
    if count > 0 {
        notes.push(format!("LLGB：解析到 {count} 个标签。"));
    }
}

fn parse_sapf(dec: &[u8], fields: &mut Vec<SectorField>, notes: &mut Vec<String>) {
    if dec.get(0x100..0x104) != Some(b"SAPF") {
        return;
    }
    fields.push(field(0x100, 0x104, "SAPF magic", "SAPF", FieldStyle::Magic));
    let entry = 0x104;
    let status = dec[entry];
    let ptype = dec[entry + 4];
    let start = u32_at(dec, entry + 8).unwrap_or(0);
    let secs = u32_at(dec, entry + 12).unwrap_or(0);
    fields.push(grouped_field(
        entry,
        entry + 1,
        "MBR 恢复表项",
        "status",
        format!("0x{status:02X}"),
        FieldStyle::Flag,
    ));
    fields.push(grouped_field(
        entry + 1,
        entry + 4,
        "MBR 恢复表项",
        "start CHS",
        hex_bytes(&dec[entry + 1..entry + 4]),
        FieldStyle::Address,
    ));
    fields.push(grouped_field(
        entry + 4,
        entry + 5,
        "MBR 恢复表项",
        "partition type",
        format!("0x{ptype:02X}"),
        FieldStyle::Flag,
    ));
    fields.push(grouped_field(
        entry + 5,
        entry + 8,
        "MBR 恢复表项",
        "end CHS",
        hex_bytes(&dec[entry + 5..entry + 8]),
        FieldStyle::Address,
    ));
    fields.push(grouped_field(
        entry + 8,
        entry + 12,
        "MBR 恢复表项",
        "起始 LBA",
        start.to_string(),
        FieldStyle::Address,
    ));
    fields.push(grouped_field(
        entry + 12,
        entry + 16,
        "MBR 恢复表项",
        "扇区数",
        format!("{secs} / {}", human_bytes(secs as u64 * SECTOR as u64)),
        FieldStyle::Size,
    ));
    notes.push(
        "SAPF：magic 后 +0x04..+0x13 是 16B MBR 分区表项，Windows LBA0 自愈会以此作为第一恢复源。"
            .into(),
    );
    if dec[0x114..0x120].iter().any(|byte| *byte != 0) {
        notes.push("SAPF +0x14..+0x1F 存在附加材料；字段语义尚未闭合。".into());
    }
}

fn parse_eppe(dec: &[u8], fields: &mut Vec<SectorField>, notes: &mut Vec<String>) {
    if dec.get(0x180..0x184) != Some(b"EPPE") {
        return;
    }
    fields.push(field(0x180, 0x184, "EPPE magic", "EPPE", FieldStyle::Magic));
    let min_len = u32_at(dec, 0x184).unwrap_or(0);
    fields.push(field(
        0x184,
        0x188,
        "最小密码长度",
        min_len.to_string(),
        FieldStyle::Flag,
    ));
    let text_end = c_field_end(dec, 0x188, 0x200);
    if text_end > 0x188 {
        fields.push(field(
            0x188,
            text_end,
            "EPPE +0x08 文本槽",
            text_value(&dec[0x188..text_end]),
            FieldStyle::Text,
        ));
    }
    notes.push(
        "EPPE：独立 0x80B A6B0 块；SetPassInfoEx 将 +0x04 限定为 6..19，ReadMinPassLenInfo 从同一字段返回最小密码长度。"
            .into(),
    );
}

fn padded4(s: &str) -> [u8; 4] {
    let mut out = [0u8; 4];
    let b = s.as_bytes();
    let n = b.len().min(4);
    out[..n].copy_from_slice(&b[..n]);
    out
}

fn chs_capacity(size: u64) -> u64 {
    const UNIT: u64 = 255 * 63 * 512;
    size / UNIT * UNIT
}

fn decode_lba11(raw: &[u8], meta: &InspectMeta) -> Option<(Vec<u8>, String)> {
    if raw.len() != SECTOR {
        return None;
    }
    let vid = meta.vid.as_deref()?;
    let pid = meta.pid.as_deref()?;
    let size = meta.size_bytes?;
    let rand = &raw[..0x100];
    let cipher = &raw[0x100..0x200];
    let mut candidates = vec![(size, "DiskSize")];
    let chs = chs_capacity(size);
    if chs != size {
        candidates.push((chs, "CHS"));
    }
    for (sz, source) in candidates {
        let mut buf = Vec::with_capacity(0x100 + 16);
        buf.extend_from_slice(rand);
        buf.extend_from_slice(&padded4(vid));
        buf.extend_from_slice(&padded4(pid));
        buf.extend_from_slice(&sz.to_le_bytes());
        let crc = crc32_bare(&buf);
        let pt = a6b0_full(cipher, &crc.to_le_bytes(), 0);
        if pt.get(..4) == Some(b"PDKB") {
            let mut dec = rand.to_vec();
            dec.extend_from_slice(&pt);
            return Some((
                dec,
                format!("A6B0 key=CRC32(rand+VID+PID+{source})=0x{crc:08X} → PDKB ✓"),
            ));
        }
    }
    None
}

pub fn analyze_sector(lba: u32, raw: &[u8], meta: &InspectMeta) -> SectorView {
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
    let mut fields = Vec::new();
    let mut notes = Vec::new();
    let mut decoded = raw.to_vec();
    let method = match lba {
        0 => {
            parse_mbr(&decoded, &mut fields, &mut notes);
            "RAW + MBR 结构解析".into()
        }
        4 => {
            if let Some((d, serial, serial_text, k0, hs, he)) = decode_lba4(raw) {
                decoded = d;
                fields.push(field(
                    hs,
                    he,
                    "labelOnlyId",
                    format!("{} (0x{serial:08X})", serial_text),
                    FieldStyle::Identity,
                ));
                if decoded.get(0x39..0x3d) == Some(b"LLGB") {
                    fields.push(field(0x39, 0x3d, "LLGB magic", "LLGB", FieldStyle::Magic));
                }
                notes.push(
                    "LBA4 的 0x18 以后按 labelOnlyId 派生 K0 做 rolling XOR；原始 0 填充保持为 0。"
                        .into(),
                );
                format!("XOR K0=0x{k0:04X} from labelOnlyId={serial_text}")
            } else {
                "RAW（未找到 $$$<onlyid>$$$）".into()
            }
        }
        6 => {
            decoded = parse_lba6(raw, meta, &mut fields, &mut notes);
            "XOR K0=0x4DAA (SAFE6)".into()
        }
        7 => {
            if let Some((crc, _)) = crc_key(meta) {
                let k0 = lba7_k0(crc);
                decoded = xor_rolling(raw, k0);
                if decoded.get(..4) == Some(b"EDPF") {
                    parse_edpf(&decoded, 0x40, &mut fields, &mut notes);
                }
                format!("XOR K0=0x{k0:04X} from CRC32(device_id)=0x{crc:08X}")
            } else {
                "RAW（缺 device_id，无法解 LBA7）".into()
            }
        }
        8 => {
            if let Some((crc, key)) = crc_key(meta) {
                let head = a6b0_full(&raw[..16], &key, 0);
                let encrypted_len = if head.get(..4) == Some(b"LLGB") {
                    u32_at(&head, 4)
                        .map(|value| value as usize)
                        .and_then(|logical_len| logical_len.checked_add(15))
                        .map(|value| value & !15)
                        .filter(|value| *value >= 16 && *value <= SECTOR)
                        .unwrap_or(LLGB_FALLBACK_LEN)
                } else {
                    LLGB_FALLBACK_LEN
                };
                decoded = raw.to_vec();
                decoded[..encrypted_len].copy_from_slice(&a6b0_full(
                    &raw[..encrypted_len],
                    &key,
                    0,
                ));
                if decoded.get(..4) == Some(b"LLGB") || decoded.contains(&b'<') {
                    parse_llgb(&decoded, &mut fields, &mut notes);
                }
                format!(
                    "A6B0 前 {}B，key=CRC32(device_id)=0x{crc:08X}",
                    encrypted_len
                )
            } else {
                "RAW（缺 device_id，无法解 LBA8）".into()
            }
        }
        9 => {
            if let Some((crc, key)) = crc_key(meta) {
                decoded = raw.to_vec();
                let eetu = a6b0_full(&raw[..0x80], &key, 0);
                decoded[..0x80].copy_from_slice(&eetu);
                for off in 0x100..0x120 {
                    decoded[off] = raw[off] ^ 0x88;
                }
                if raw[0x180..0x200].iter().any(|byte| *byte != 0) {
                    let eppe = a6b0_full(&raw[0x180..0x200], &key, 0);
                    decoded[0x180..0x200].copy_from_slice(&eppe);
                }
                if decoded.get(..4) == Some(b"EETU") {
                    fields.push(field(0x00, 0x04, "EETU magic", "EETU", FieldStyle::Magic));
                    if let Some(value) = u64_at(&decoded, 0x04) {
                        fields.push(field(
                            0x04,
                            0x0c,
                            "EETU 开始时间 (ullBTime)",
                            if value == 0 {
                                "0（不限制）".into()
                            } else {
                                value.to_string()
                            },
                            FieldStyle::Flag,
                        ));
                    }
                    if let Some(value) = u64_at(&decoded, 0x0c) {
                        fields.push(field(
                            0x0c,
                            0x14,
                            "EETU 结束时间 (ullETime)",
                            if value == 0 {
                                "0（不限制）".into()
                            } else {
                                value.to_string()
                            },
                            FieldStyle::Flag,
                        ));
                    }
                    if let Some(value) = u32_at(&decoded, 0x14) {
                        fields.push(field(
                            0x14,
                            0x18,
                            "EETU 使用次数 (useCount)",
                            if value == u32::MAX {
                                "无限（0xFFFFFFFF）".into()
                            } else {
                                value.to_string()
                            },
                            FieldStyle::Flag,
                        ));
                    }
                    notes.push(
                        "EETU：tagEdpEDiskTmpUse 独立 0x80B A6B0 运行时块；ullBTime/ullETime 与 time(NULL) 比较，二者都为 0 时不限制时间；useCount=0xFFFFFFFF 表示无限次数，0 表示次数耗尽，其它正值每次检查后减 1 并由 WriteTempUseInfo 回写。reverse[104] 的业务语义仍未闭合。".into(),
                    );
                }
                let before = fields.len();
                parse_sapf(&decoded, &mut fields, &mut notes);
                if fields.len() == before {
                    notes.push("未检测到 SAPF 结构。".into());
                }
                parse_eppe(&decoded, &mut fields, &mut notes);
                format!("A6B0 EETU@0x000/EPPE@0x180 + XOR 0x88 SAPF@0x100，CRC=0x{crc:08X}")
            } else {
                "RAW（缺 device_id，无法解 LBA9）".into()
            }
        }
        10 => {
            if raw.iter().all(|&b| b == 0) {
                notes.push("LBA10 全零。".into());
                "RAW（当前样本为空）".into()
            } else if let Some((crc, key)) = crc_key(meta) {
                let head = a6b0_full(&raw[..0x80], &key, 0);
                if head.get(..4) == Some(b"EESI") {
                    decoded[..0x80].copy_from_slice(&head);
                    fields.push(field(0x00, 0x04, "EESI magic", "EESI", FieldStyle::Magic));
                    if let Some(value) = u32_at(&decoded, 0x04) {
                        fields.push(field(
                            0x04,
                            0x08,
                            "EESI +0x04",
                            format!("{} (0x{value:08X})", value),
                            FieldStyle::Flag,
                        ));
                    }
                    fields.push(field(
                        0x08,
                        0x18,
                        "EESI 交换区卷标",
                        text_value(&decoded[0x08..0x18]),
                        FieldStyle::Text,
                    ));
                    fields.push(field(
                        0x18,
                        0x28,
                        "EESI 保密区卷标",
                        text_value(&decoded[0x18..0x28]),
                        FieldStyle::Text,
                    ));
                    notes.push(
                        "EESI 仅前 0x80B 由读写端加解密；UserLogin 将 +0x08 的字符串用于 type2(交换区) SetVolumeLabelA，将 +0x18 的字符串用于 type4(保密区) SetVolumeLabelA；后 0x180B 不属于该结构，writer 读改写时保持原字节。".into(),
                    );
                    format!("A6B0 前 0x80B，key=CRC32(device_id)=0x{crc:08X} → EESI ✓")
                } else {
                    notes.push("LBA10 非零，但前 0x80B 未解出 EESI。".into());
                    format!("A6B0 前 0x80B 尝试，CRC=0x{crc:08X}（非 EESI）")
                }
            } else {
                notes.push("LBA10 非零；缺 device_id，无法验证 EESI。".into());
                "RAW（缺 device_id，无法解 LBA10）".into()
            }
        }
        11 => {
            if let Some((d, m)) = decode_lba11(raw, meta) {
                decoded = d;
                fields.push(field(0x100, 0x104, "PDKB magic", "PDKB", FieldStyle::Magic));
                fields.push(field(
                    0x104,
                    0x200,
                    "PDKB device_id",
                    text_value(&decoded[0x104..0x200]),
                    FieldStyle::Identity,
                ));
                m
            } else {
                "RAW（缺 VID/PID/容量或 PDKB 解密未通过）".into()
            }
        }
        12 => {
            if let Some((crc, key)) = crc_key(meta) {
                decoded = a6b0_full(raw, &key, 0);
                if decoded.get(..4) == Some(b"EDPF") {
                    parse_edpf(&decoded[..EDPF_TABLE_LEN], 0x60, &mut fields, &mut notes);
                }
                notes.push(
                    "LBA12：整扇 512B 使用连续 A6B0；0x170 只是 EDPF 表区边界，解密后的 0x170..0x1FF 为零填充。".into(),
                );
                format!("A6B0 整扇 512B，CRC=0x{crc:08X}")
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
