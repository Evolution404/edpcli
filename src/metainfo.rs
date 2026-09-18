//! 面向人的元信息汇总。
//!
//! `inspect` 负责按扇区查看协议结构；本模块把多个已知扇区的结果汇总成一张设备/备份
//! 元信息卡片，重点突出 onlyid、device_id、Dept、User、SAFE6 与分区摘要。

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use crate::common::SECTOR;
use crate::crypto::crc32_bare;
use crate::diskio::{self, BackupEntry};
use crate::inspect::{self, InspectMeta, SectorView};

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
    pub partitions: Vec<PartitionInfo>,
}

fn field_value(view: &SectorView, label: &str) -> Option<String> {
    view.fields
        .iter()
        .find(|field| field.label == label && !field.value.is_empty())
        .map(|field| field.value.clone())
}

fn child_value(view: &SectorView, label: &str) -> Option<String> {
    view.fields
        .iter()
        .flat_map(|field| field.children.iter())
        .find(|child| child.label == label && child.value != "<空>")
        .map(|child| child.value.clone())
}

fn partitions(view: &SectorView, source: &str) -> Vec<PartitionInfo> {
    let mut groups: BTreeMap<String, PartitionInfo> = BTreeMap::new();
    for field in &view.fields {
        let Some(group) = field.group.as_deref() else {
            continue;
        };
        if !group.starts_with("Entry[") {
            continue;
        }
        let item = groups.entry(group.to_string()).or_insert_with(|| PartitionInfo {
            source: source.to_string(),
            name: group.to_string(),
            ..Default::default()
        });
        match field.label.as_str() {
            "类型" => item.kind = Some(field.value.clone()),
            "状态" => item.status = Some(field.value.clone()),
            "起始 LBA" => item.start_lba = Some(field.value.clone()),
            "大小" => item.size = Some(field.value.clone()),
            _ => {}
        }
    }
    groups.into_values().collect()
}

pub fn summarize<F>(base: &InspectMeta, mut read: F) -> io::Result<MetaInfoSummary>
where
    F: FnMut(u32) -> io::Result<Vec<u8>>,
{
    let raw4 = read(4)?;
    let raw6 = read(6)?;
    let raw7 = read(7)?;
    let raw8 = read(8)?;
    let raw11 = read(11)?;
    let raw12 = read(12)?;
    for (lba, raw) in [(4, &raw4), (6, &raw6), (7, &raw7), (8, &raw8), (11, &raw11), (12, &raw12)] {
        if raw.len() != SECTOR {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("LBA{lba} 读取 {}B，预期 {}B", raw.len(), SECTOR),
            ));
        }
    }

    let v6 = inspect::analyze_sector(6, &raw6, base);
    let v7 = inspect::analyze_sector(7, &raw7, base);
    let v8 = inspect::analyze_sector(8, &raw8, base);
    let v11 = inspect::analyze_sector(11, &raw11, base);
    let v12 = inspect::analyze_sector(12, &raw12, base);

    let onlyid = diskio::lba4_label_id_from(&raw4).or_else(|| base.onlyid.clone());
    let device_crc32 = base
        .device_id
        .as_deref()
        .map(|id| format!("0x{:08X}", crc32_bare(id.as_bytes())));

    let mut partition_rows = partitions(&v7, "LBA7");
    partition_rows.extend(partitions(&v12, "LBA12"));

    Ok(MetaInfoSummary {
        onlyid,
        device_id: base.device_id.clone(),
        device_crc32,
        vid: base.vid.clone(),
        pid: base.pid.clone(),
        size_bytes: base.size_bytes,
        ownership: OwnershipInfo {
            glab: child_value(&v8, "GLab"),
            dept: child_value(&v8, "Dept"),
            user: child_value(&v8, "User"),
            label: child_value(&v8, "Label"),
            rmark: child_value(&v8, "Rmark"),
            autonum: child_value(&v8, "Autonum"),
        },
        safe6_label: field_value(&v6, "标签"),
        safe6_user: field_value(&v6, "用户"),
        safe6_serial: field_value(&v6, "序列"),
        safe6_register: field_value(&v6, "注册标志"),
        safe6_checksum: field_value(&v6, "校验和"),
        pdkb_device_id: field_value(&v11, "PDKB device_id"),
        partitions: partition_rows,
    })
}

pub fn backup_ownership(entry: &BackupEntry) -> Option<OwnershipInfo> {
    let meta = entry.meta.as_ref()?;
    let inspect_meta = InspectMeta::from_backup_meta(meta);
    let raw = diskio::read_lba(entry.path.to_str()?, 8).ok()?;
    if raw.len() != SECTOR {
        return None;
    }
    let view = inspect::analyze_sector(8, &raw, &inspect_meta);
    Some(OwnershipInfo {
        glab: child_value(&view, "GLab"),
        dept: child_value(&view, "Dept"),
        user: child_value(&view, "User"),
        label: child_value(&view, "Label"),
        rmark: child_value(&view, "Rmark"),
        autonum: child_value(&view, "Autonum"),
    })
}

pub fn render(summary: &MetaInfoSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n", crate::ui::bold("身份信息")));
    let row = |out: &mut String, key: &str, value: Option<&str>| {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            out.push_str(&format!("  {}  {}\n", crate::ui::pad_to(key, 18), value));
        }
    };
    row(&mut out, "onlyid", summary.onlyid.as_deref());
    row(&mut out, "device_id", summary.device_id.as_deref());
    row(&mut out, "device_id CRC32", summary.device_crc32.as_deref());
    if let (Some(vid), Some(pid)) = (&summary.vid, &summary.pid) {
        out.push_str(&format!("  {}  {}:{}\n", crate::ui::pad_to("USB VID:PID", 18), vid, pid));
    }
    if let Some(size) = summary.size_bytes {
        out.push_str(&format!(
            "  {}  {}\n",
            crate::ui::pad_to("容量", 18),
            crate::common::fmt_gb(size)
        ));
    }
    row(&mut out, "PDKB device_id", summary.pdkb_device_id.as_deref());

    out.push('\n');
    out.push_str(&format!("{}\n", crate::ui::bold("归属信息")));
    row(&mut out, "Dept", summary.ownership.dept.as_deref());
    row(&mut out, "User", summary.ownership.user.as_deref());
    row(&mut out, "Label", summary.ownership.label.as_deref());
    row(&mut out, "Rmark", summary.ownership.rmark.as_deref());
    row(&mut out, "GLab", summary.ownership.glab.as_deref());
    row(&mut out, "Autonum", summary.ownership.autonum.as_deref());
    if summary.ownership.dept.is_none() && summary.ownership.user.is_none() {
        out.push_str(&format!("  {}\n", crate::ui::dim("LBA8 未解析到 Dept/User。")));
    }

    out.push('\n');
    out.push_str(&format!("{}\n", crate::ui::bold("SAFE6")));
    row(&mut out, "标签", summary.safe6_label.as_deref());
    row(&mut out, "用户", summary.safe6_user.as_deref());
    row(&mut out, "序列", summary.safe6_serial.as_deref());
    row(&mut out, "注册", summary.safe6_register.as_deref());
    row(&mut out, "校验", summary.safe6_checksum.as_deref());

    if !summary.partitions.is_empty() {
        out.push('\n');
        out.push_str(&format!("{}\n", crate::ui::bold("分区摘要")));
        for part in &summary.partitions {
            let mut values = Vec::new();
            if let Some(v) = &part.kind {
                values.push(v.clone());
            }
            if let Some(v) = &part.status {
                values.push(v.clone());
            }
            if let Some(v) = &part.start_lba {
                values.push(format!("start={v}"));
            }
            if let Some(v) = &part.size {
                values.push(v.clone());
            }
            out.push_str(&format!(
                "  {} {}  {}\n",
                crate::ui::pad_to(&part.source, 7),
                crate::ui::pad_to(&part.name, 10),
                values.join(" · ")
            ));
        }
    }
    out
}

pub fn summarize_backup(path: &Path, meta: &InspectMeta) -> io::Result<MetaInfoSummary> {
    let path = path.to_string_lossy().into_owned();
    summarize(meta, |lba| diskio::read_lba(&path, lba))
}
