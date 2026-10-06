#![allow(dead_code)]

//! Parse filenames only to construct test fixtures. Runtime identity comes from EDPB contents.
use edpcli::infrastructure::backup_store::catalog::{ts_suffix_pos, BackupMeta};

pub fn parse_gold_name(name: &str) -> Option<BackupMeta> {
    let stem = name.strip_suffix(".bin")?;
    parse_fixture_backup_name(&format!("{stem}.edpb"))
}

fn strip_numeric_suffix<'a>(s: &'a str, marker: &str) -> Option<(&'a str, String)> {
    let pos = s.rfind(marker)?;
    let value = &s[pos + marker.len()..];
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let start = usize::from(bytes.first() == Some(&b'-'));
    if start == bytes.len() || !bytes[start..].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((&s[..pos], value.to_string()))
}

/// 解析本工具备份文件名。EDP 备份身份段为 device_id；Plain v3 使用显式 `plain`
/// 占位，避免把派生 device_id candidate 冒充成已观测 EDP device_id。
/// 固定锚点只使用 disk/secs/vid/pid 与尾部时间戳/状态/onlyid。
pub fn parse_fixture_backup_name(name: &str) -> Option<BackupMeta> {
    let ts_pos = ts_suffix_pos(name)?;
    let stem_before_ts = &name[..ts_pos];
    let after_disk = stem_before_ts.strip_prefix("disk")?;
    let disk_end = after_disk.find('_')?;
    let disk_s = &after_disk[..disk_end];
    if disk_s.is_empty() || !disk_s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let disk = disk_s.parse::<u32>().ok()?;
    let rest = &after_disk[disk_end + 1..];

    let vid_pos = rest.find("_vid")?;
    let secs_s = &rest[..vid_pos];
    let secs = if secs_s == "unknown" {
        None
    } else if !secs_s.is_empty() && secs_s.bytes().all(|b| b.is_ascii_digit()) {
        Some(secs_s.parse::<u64>().ok()?)
    } else {
        return None;
    };

    let after_vid = &rest[vid_pos + 4..];
    let pid_pos = after_vid.find("_pid")?;
    let vid = &after_vid[..pid_pos];
    if vid.is_empty() || !vid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let after_pid = &after_vid[pid_pos + 4..];
    let device_pos = after_pid.find('_')?;
    let pid = &after_pid[..device_pos];
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    let tail = &after_pid[device_pos + 1..];
    let (device_id, onlyid) = match strip_numeric_suffix(tail, "_onlyid") {
        Some((did, id)) => (did, Some(id)),
        None => match strip_numeric_suffix(tail, "_lid") {
            Some((did, id)) => (did, Some(id)),
            None => (tail, None),
        },
    };
    if device_id != "plain" && !device_id.starts_with("disk&ven_") {
        return None;
    }

    Some(BackupMeta {
        disk,
        secs,
        vid: vid.to_string(),
        pid: pid.to_string(),
        device_id: device_id.to_string(),
        onlyid,
        identity: None,
    })
}
