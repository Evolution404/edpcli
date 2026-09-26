//! 面向人的元信息汇总。
//!
//! `inspect` 负责按扇区查看协议结构；本模块把多个已知扇区的结果汇总成一张设备/备份
//! 元信息卡片，重点突出 onlyid、device_id、Dept、User、SAFE6 与分区摘要。

use std::io;
use std::path::Path;

use crate::common::SECTOR;
use crate::crypto::crc32_bare;
use crate::diskio::{self, BackupEntry};
use crate::protocol::semantic::{self, SemanticContext, SemanticContextSource};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnershipInfo {
    pub glab: Option<String>,
    pub dept: Option<String>,
    pub user: Option<String>,
    pub label: Option<String>,
    pub rmark: Option<String>,
    pub autonum: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartitionInfo {
    pub source: String,
    pub name: String,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub start_lba: Option<String>,
    pub size: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaInfoSummary {
    pub is_plain: bool,
    pub onlyid: Option<String>,
    pub device_id: Option<String>,
    pub device_crc32: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub size_bytes: Option<u64>,
    pub ownership: OwnershipInfo,
    pub safe6_label: Option<String>,
    pub safe6_user: Option<String>,
    pub safe6_serial: Option<String>,
    pub safe6_register: Option<String>,
    pub safe6_checksum: Option<String>,
    pub pdkb_device_id: Option<String>,
    pub is_nopwd: Option<bool>,
    pub partitions: Vec<PartitionInfo>,
}

impl MetaInfoSummary {
    /// Plain media has no EDP protocol fields to decode from its filesystem sectors.
    pub fn plain(vid: Option<String>, pid: Option<String>, size_bytes: Option<u64>) -> Self {
        Self {
            is_plain: true,
            vid,
            pid,
            size_bytes,
            ..Self::default()
        }
    }
}

fn context_from_backup_meta(meta: &diskio::BackupMeta) -> SemanticContext {
    SemanticContext {
        device_id: Some(meta.device_id.clone()),
        vid: Some(meta.vid.clone()),
        pid: Some(meta.pid.clone()),
        size_bytes: meta
            .secs
            .and_then(|sectors| sectors.checked_mul(SECTOR as u64)),
        onlyid: meta.onlyid.clone(),
    }
}

fn human_bytes(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.2} GB", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.2} MB", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.2} KB", value as f64 / 1_000.0)
    } else {
        format!("{} B", value)
    }
}

fn partition_info(partition: semantic::PartitionSemantics) -> PartitionInfo {
    let role = match partition.partition_type {
        1 => "Boot",
        2 => "Share",
        4 => "Encrypt",
        _ => "未知",
    };
    PartitionInfo {
        source: partition.source.to_string(),
        name: format!("Entry[{}]", partition.index),
        kind: Some(format!("{role} ({})", partition.partition_type)),
        status: None,
        start_lba: Some(partition.start_sector.to_string()),
        size: Some(format!(
            "{} B / {}",
            partition.partition_size,
            human_bytes(partition.partition_size)
        )),
    }
}

pub fn safe6_label_from_lba6<C: SemanticContextSource>(raw: &[u8], _context: &C) -> Option<String> {
    semantic::safe6_label(raw)
}

pub fn summarize<C, F>(base: &C, mut read: F) -> io::Result<MetaInfoSummary>
where
    C: SemanticContextSource,
    F: FnMut(u32) -> io::Result<Vec<u8>>,
{
    let base = base.semantic_context();
    let raw0 = read(0)?;
    let raw4 = read(4)?;
    let raw6 = read(6)?;
    let raw7 = read(7)?;
    let raw8 = read(8)?;
    let raw11 = read(11)?;
    let raw12 = read(12)?;
    for (lba, raw) in [
        (0, &raw0),
        (4, &raw4),
        (6, &raw6),
        (7, &raw7),
        (8, &raw8),
        (11, &raw11),
        (12, &raw12),
    ] {
        if raw.len() != SECTOR {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("LBA{lba} 读取 {}B，预期 {}B", raw.len(), SECTOR),
            ));
        }
    }

    let lba6 = semantic::lba6_view(&raw6);
    let ownership = semantic::ownership_from_lba8(&raw8, &base).unwrap_or_default();

    let onlyid = diskio::lba4_label_id_from(&raw4).or_else(|| base.onlyid.clone());
    let device_crc32 = base
        .device_id
        .as_deref()
        .map(|id| format!("0x{:08X}", crc32_bare(id.as_bytes())));

    let mut partition_rows = semantic::lba7_partitions(&raw7, &base)
        .into_iter()
        .map(partition_info)
        .collect::<Vec<_>>();
    partition_rows.extend(
        semantic::lba12_partitions(&raw12, &base)
            .into_iter()
            .map(partition_info),
    );

    let is_nopwd = base.device_id.as_deref().and_then(|device_id| {
        let snapshot = |lba| match lba {
            0 => Ok(raw0.clone()),
            6 => Ok(raw6.clone()),
            12 => Ok(raw12.clone()),
            _ => Err(crate::common::EdpCliError::new(
                crate::common::EXIT_IO,
                format!("错误: info 免密判断不应读取 LBA{lba}"),
            )),
        };
        crate::sectors::looks_nopwd(&snapshot, device_id).ok()
    });

    Ok(MetaInfoSummary {
        is_plain: false,
        onlyid,
        device_id: base.device_id.clone(),
        device_crc32,
        vid: base.vid.clone(),
        pid: base.pid.clone(),
        size_bytes: base.size_bytes,
        ownership: OwnershipInfo {
            glab: ownership.glab,
            dept: ownership.dept,
            user: ownership.user,
            label: ownership.label,
            rmark: ownership.rmark,
            autonum: ownership.autonum,
        },
        safe6_label: semantic::safe6_label(&raw6),
        safe6_user: semantic::safe6_user(&raw6),
        safe6_serial: semantic::safe6_gserial(&raw6),
        safe6_register: None,
        safe6_checksum: lba6.as_ref().map(|view| {
            let calculated = crate::crypto::lba6_checksum(&raw6[..0x1fc]);
            if view.checksum == calculated {
                format!("0x{:08X} / 计算 0x{:08X} ✓", view.checksum, calculated)
            } else if view.checksum == calculated.wrapping_mul(2) {
                format!(
                    "0x{:08X} / 计算 0x{:08X} ×2 profile ✓",
                    view.checksum, calculated
                )
            } else {
                format!("0x{:08X} / 计算 0x{:08X} ✗", view.checksum, calculated)
            }
        }),
        pdkb_device_id: semantic::pdkb_device_id(&raw11, &base),
        is_nopwd,
        partitions: partition_rows,
    })
}

pub fn backup_ownership(entry: &BackupEntry) -> Option<OwnershipInfo> {
    let meta = entry.meta.as_ref()?;
    let context = context_from_backup_meta(meta);
    let raw = entry.lba8.as_ref()?;
    ownership_from_lba8(raw, &context)
}

pub fn ownership_from_lba8<C: SemanticContextSource>(
    raw: &[u8],
    context: &C,
) -> Option<OwnershipInfo> {
    let context = context.semantic_context();
    let ownership = semantic::ownership_from_lba8(raw, &context)?;
    Some(OwnershipInfo {
        glab: ownership.glab,
        dept: ownership.dept,
        user: ownership.user,
        label: ownership.label,
        rmark: ownership.rmark,
        autonum: ownership.autonum,
    })
}

pub fn render(summary: &MetaInfoSummary) -> String {
    render_with_source(summary, None)
}

pub fn render_with_source(summary: &MetaInfoSummary, source: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n", crate::ui::bold_cyan("设备")));
    let row = |out: &mut String, key: &str, value: Option<&str>, paint: fn(&str) -> String| {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            out.push_str(&format!(
                "  {}  {}\n",
                crate::ui::dim(&crate::ui::pad_to(key, 18)),
                paint(value)
            ));
        }
    };
    row(&mut out, "来源", source, crate::ui::cyan);
    row(
        &mut out,
        "onlyid",
        summary.onlyid.as_deref(),
        crate::ui::yellow,
    );
    row(
        &mut out,
        "device_id",
        summary.device_id.as_deref(),
        crate::ui::yellow,
    );
    row(
        &mut out,
        "device_id CRC32",
        summary.device_crc32.as_deref(),
        crate::ui::yellow,
    );
    if let (Some(vid), Some(pid)) = (&summary.vid, &summary.pid) {
        out.push_str(&format!(
            "  {}  {}\n",
            crate::ui::dim(&crate::ui::pad_to("USB VID:PID", 18)),
            crate::ui::yellow(&format!("{vid}:{pid}"))
        ));
    }
    if let Some(size) = summary.size_bytes {
        out.push_str(&format!(
            "  {}  {}\n",
            crate::ui::dim(&crate::ui::pad_to("容量", 18)),
            crate::ui::magenta(&crate::common::fmt_gb(size))
        ));
    }
    row(
        &mut out,
        "PDKB device_id",
        summary.pdkb_device_id.as_deref(),
        crate::ui::yellow,
    );

    if summary.is_plain {
        out.push_str(&format!(
            "\n{}\n  {}  {}\n",
            crate::ui::bold_cyan("状态"),
            crate::ui::dim(&crate::ui::pad_to("盘型", 18)),
            crate::ui::green("普通盘 (Plain)")
        ));
        return out;
    }

    out.push('\n');
    out.push_str(&format!("{}\n", crate::ui::bold_cyan("身份")));
    row(
        &mut out,
        "Dept",
        summary.ownership.dept.as_deref(),
        crate::ui::cyan,
    );
    row(
        &mut out,
        "User",
        summary.ownership.user.as_deref(),
        crate::ui::cyan,
    );
    row(
        &mut out,
        "Label",
        summary.ownership.label.as_deref(),
        crate::ui::cyan,
    );
    row(
        &mut out,
        "Rmark",
        summary.ownership.rmark.as_deref(),
        crate::ui::cyan,
    );
    row(
        &mut out,
        "GLab",
        summary.ownership.glab.as_deref(),
        crate::ui::yellow,
    );
    row(
        &mut out,
        "Autonum",
        summary.ownership.autonum.as_deref(),
        crate::ui::yellow,
    );
    if summary.ownership.dept.is_none() && summary.ownership.user.is_none() {
        out.push_str(&format!(
            "  {}\n",
            crate::ui::dim("LBA8 未解析到 Dept/User。")
        ));
    }

    out.push('\n');
    out.push_str(&format!("{}\n", crate::ui::bold_cyan("状态")));
    row(
        &mut out,
        "EDP/cems",
        summary.device_id.as_ref().map(|_| "已识别"),
        crate::ui::green,
    );
    row(
        &mut out,
        "免密",
        summary
            .is_nopwd
            .map(|value| if value { "是" } else { "否" }),
        crate::ui::green,
    );
    row(
        &mut out,
        "标签",
        summary.safe6_label.as_deref(),
        crate::ui::cyan,
    );
    row(
        &mut out,
        "用户",
        summary.safe6_user.as_deref(),
        crate::ui::cyan,
    );
    row(
        &mut out,
        "序列",
        summary.safe6_serial.as_deref(),
        crate::ui::yellow,
    );
    row(
        &mut out,
        "注册",
        summary.safe6_register.as_deref(),
        crate::ui::yellow,
    );
    if let Some(checksum) = summary.safe6_checksum.as_deref() {
        let paint = if checksum.contains('✗') {
            crate::ui::red
        } else {
            crate::ui::green
        };
        row(&mut out, "校验", Some(checksum), paint);
    }

    if !summary.partitions.is_empty() {
        out.push('\n');
        out.push_str(&format!("{}\n", crate::ui::bold_cyan("分区摘要")));
        let rows = summary
            .partitions
            .iter()
            .map(|part| {
                let (active, enc) = part
                    .status
                    .as_deref()
                    .map(|status| {
                        let active = status
                            .split_whitespace()
                            .find_map(|token| token.strip_prefix("active="))
                            .unwrap_or("-");
                        let enc = status
                            .split_whitespace()
                            .find_map(|token| token.strip_prefix("enc="))
                            .unwrap_or("-");
                        (active.to_string(), enc.to_string())
                    })
                    .unwrap_or_else(|| ("-".into(), "-".into()));
                let (bytes, human) = part
                    .size
                    .as_deref()
                    .and_then(|size| size.split_once(" / "))
                    .map(|(bytes, human)| (bytes.to_string(), human.to_string()))
                    .unwrap_or_else(|| {
                        (part.size.clone().unwrap_or_else(|| "-".into()), "-".into())
                    });
                vec![
                    crate::ui::TableCell::left(part.source.clone(), crate::ui::Tone::Green),
                    crate::ui::TableCell::left(part.name.clone(), crate::ui::Tone::BoldCyan),
                    crate::ui::TableCell::left(
                        part.kind.clone().unwrap_or_else(|| "-".into()),
                        crate::ui::Tone::Yellow,
                    ),
                    crate::ui::TableCell::right(active, crate::ui::Tone::Yellow),
                    crate::ui::TableCell::right(enc, crate::ui::Tone::Yellow),
                    crate::ui::TableCell::right(
                        part.start_lba.clone().unwrap_or_else(|| "-".into()),
                        crate::ui::Tone::Green,
                    ),
                    crate::ui::TableCell::right(bytes, crate::ui::Tone::Magenta),
                    crate::ui::TableCell::right(human, crate::ui::Tone::Magenta),
                ]
            })
            .collect::<Vec<_>>();
        out.push_str(&crate::ui::render_table(
            &[
                "来源",
                "条目",
                "类型",
                "Active",
                "Enc",
                "起始LBA",
                "字节数",
                "容量",
            ],
            &rows,
        ));
    }
    out
}

pub fn summarize_backup<C: SemanticContextSource>(
    path: &Path,
    meta: &C,
) -> io::Result<MetaInfoSummary> {
    let data = crate::edpb::read_raw_protocol(path)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
    summarize(meta, |lba| {
        let start = lba as usize * SECTOR;
        let end = start + SECTOR;
        data.get(start..end)
            .map(|bytes| bytes.to_vec())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("EDPB Core 不含 LBA{lba}"),
                )
            })
    })
}
