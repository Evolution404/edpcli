use std::fs;
use std::path::{Path, PathBuf};

use edpcli::common::{METADATA_IMAGE_LEN, SECTOR};
use edpcli::diskio;
use edpcli::edpb::{self, CaptureLevel, CoreCapture};
use serde_json::json;
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

#[derive(Debug)]
struct LegacyItem {
    path: PathBuf,
    output: PathBuf,
    source_sha256: String,
    meta: diskio::BackupMeta,
    actual_onlyid: Option<String>,
    created_epoch: i64,
    timestamp_text: String,
    is_nopwd: bool,
    data: Vec<u8>,
}

fn parse_timestamp(path: &Path) -> Result<(String, i64), String> {
    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .ok_or_else(|| format!("文件名不是 UTF-8: {}", path.display()))?;
    if stem.len() < 15 {
        return Err(format!("文件名缺少 YYYYMMDD_HHMMSS: {}", path.display()));
    }
    let stamp = &stem[stem.len() - 15..];
    let b = stamp.as_bytes();
    if b.len() != 15
        || b[8] != b'_'
        || !b[..8].iter().all(u8::is_ascii_digit)
        || !b[9..].iter().all(u8::is_ascii_digit)
    {
        return Err(format!("文件名时间戳非法: {}", path.display()));
    }
    let year = stamp[0..4].parse::<i32>().map_err(|e| e.to_string())?;
    let month = stamp[4..6].parse::<u8>().map_err(|e| e.to_string())?;
    let day = stamp[6..8].parse::<u8>().map_err(|e| e.to_string())?;
    let hour = stamp[9..11].parse::<u8>().map_err(|e| e.to_string())?;
    let minute = stamp[11..13].parse::<u8>().map_err(|e| e.to_string())?;
    let second = stamp[13..15].parse::<u8>().map_err(|e| e.to_string())?;
    let month = Month::try_from(month).map_err(|e| e.to_string())?;
    let date = Date::from_calendar_date(year, month, day).map_err(|e| e.to_string())?;
    let time = Time::from_hms(hour, minute, second).map_err(|e| e.to_string())?;
    let offset = UtcOffset::current_local_offset()
        .map_err(|e| format!("无法取得迁移主机本地 UTC offset，拒绝猜测 created_epoch: {e}"))?;
    let epoch = PrimitiveDateTime::new(date, time)
        .assume_offset(offset)
        .unix_timestamp();
    Ok((stamp.to_string(), epoch))
}

fn inspect_legacy(path: &Path) -> Result<LegacyItem, String> {
    let data = fs::read(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    if data.len() != METADATA_IMAGE_LEN {
        return Err(format!(
            "{} 长度 {}B，预期严格 LBA0-12 共 {}B",
            path.display(),
            data.len(),
            METADATA_IMAGE_LEN
        ));
    }

    let expected = diskio::read_backup_sha256(path)
        .map_err(|e| format!("读取 {}.sha256 失败: {e}", path.display()))?
        .ok_or_else(|| format!("缺少历史 SHA-256 sidecar: {}.sha256", path.display()))?;
    let actual = edpcli::sha256::sha256_hex(&data);
    if expected != actual {
        return Err(format!(
            "历史 SHA-256 不匹配: {} expected={} actual={}",
            path.display(),
            expected,
            actual
        ));
    }

    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| format!("文件名不是 UTF-8: {}", path.display()))?;
    let fake_edpb_name = format!(
        "{}.edpb",
        name.strip_suffix(".bin")
            .ok_or_else(|| format!("不是 .bin: {}", path.display()))?
    );
    let meta = diskio::parse_backup_name(&fake_edpb_name)
        .ok_or_else(|| format!("无法解析历史备份文件名: {name}"))?;

    let actual_onlyid = diskio::lba4_label_id_from(&data[4 * SECTOR..5 * SECTOR]);
    if let Some(named) = meta.onlyid.as_deref() {
        if actual_onlyid.as_deref() != Some(named) {
            return Err(format!(
                "onlyid 文件名/内容不一致: {} named={} actual={}",
                path.display(),
                named,
                actual_onlyid.as_deref().unwrap_or("未知")
            ));
        }
    }

    let (timestamp_text, created_epoch) = parse_timestamp(path)?;
    let is_nopwd = diskio::image_is_nopwd(&data, &meta.device_id);

    Ok(LegacyItem {
        output: path.with_extension(edpb::EXTENSION),
        path: path.to_path_buf(),
        source_sha256: actual,
        meta,
        actual_onlyid,
        created_epoch,
        timestamp_text,
        is_nopwd,
        data,
    })
}

fn verify_output(item: &LegacyItem) -> Result<String, String> {
    let verified = edpb::verify_file(&item.output)?;
    if verified.manifest.snapshot.capture_level != CaptureLevel::LegacyMigrated {
        return Err(format!(
            "{} capture_level 不是 legacy_migrated",
            item.output.display()
        ));
    }
    if verified.manifest.device.device_id != item.meta.device_id
        || verified.manifest.device.vid != item.meta.vid
        || verified.manifest.device.pid != item.meta.pid
        || verified.manifest.device.onlyid != item.actual_onlyid
        || verified.manifest.geometry.total_sectors != item.meta.secs
    {
        return Err(format!("{} 容器身份字段复核失败", item.output.display()));
    }
    let raw = edpb::read_raw_protocol(&item.output)?;
    if raw != item.data {
        return Err(format!(
            "{} raw.protocol.lba0_12 与历史 .bin 不逐字节一致",
            item.output.display()
        ));
    }
    let not_captured = verified
        .manifest
        .artifacts
        .iter()
        .filter(|a| a.completeness == edpb::ArtifactCompleteness::NotCaptured)
        .count();
    if not_captured != 4 {
        return Err(format!(
            "{} not_captured 占位 Artifact 数量错误: {}",
            item.output.display(),
            not_captured
        ));
    }
    Ok(verified.file_sha256)
}

fn migrate(item: &LegacyItem) -> Result<(String, bool), String> {
    if item.output.exists() {
        let hash = verify_output(item)?;
        return Ok((hash, true));
    }

    let capture = CoreCapture {
        snapshot_id: format!(
            "{}-{}",
            item.actual_onlyid
                .as_deref()
                .unwrap_or(item.meta.device_id.as_str()),
            item.timestamp_text
        ),
        created_epoch: item.created_epoch,
        disk_number: Some(item.meta.disk),
        vid: item.meta.vid.clone(),
        pid: item.meta.pid.clone(),
        device_id: item.meta.device_id.clone(),
        onlyid: item.actual_onlyid.clone(),
        total_sectors: item.meta.secs,
        logical_sector_size: SECTOR as u32,
        edpcli_version: env!("CARGO_PKG_VERSION").to_string(),
        device_state: if item.is_nopwd {
            "passwordless".into()
        } else {
            "encrypted".into()
        },
        lba0_12: &item.data,
    };
    let notes = vec![
        format!("migrated_from_legacy_file={}", item.path.display()),
        format!("legacy_source_sha256={}", item.source_sha256),
        format!("legacy_source_sidecar={}.sha256", item.path.display()),
        "legacy source contains exactly LBA0-12 (6656 bytes); no LBA13 exists".into(),
        format!(
            "legacy filename timestamp {} interpreted using migration host local UTC offset",
            item.timestamp_text
        ),
        "metadata/deep fields absent from the legacy source are explicitly not_captured".into(),
    ];
    edpb::write_legacy_migrated_backup(&item.output, &capture, &notes)?;
    let hash = verify_output(item)?;
    Ok((hash, false))
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap()).join(".edpcli-backup"));
    let apply = args.any(|arg| arg == "--apply");

    let mut paths = fs::read_dir(&dir)
        .map_err(|e| format!("读取目录 {} 失败: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("bin"))
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("{} 没有历史 .bin", dir.display()));
    }

    let mut items = Vec::with_capacity(paths.len());
    for path in &paths {
        items.push(inspect_legacy(path)?);
    }

    println!(
        "预检通过: {} 份历史备份，全部严格 6656B LBA0-12，SHA-256 sidecar 有效。",
        items.len()
    );
    for item in &items {
        println!(
            "{} -> {} | onlyid={} | state={} | source_sha256={}",
            item.path.file_name().unwrap().to_string_lossy(),
            item.output.file_name().unwrap().to_string_lossy(),
            item.actual_onlyid.as_deref().unwrap_or("未知"),
            if item.is_nopwd {
                "passwordless"
            } else {
                "encrypted"
            },
            item.source_sha256
        );
    }

    if !apply {
        println!("DRY-RUN: 未写文件；追加 --apply 执行一次性迁移。");
        return Ok(());
    }

    let mut report = Vec::with_capacity(items.len());
    for item in &items {
        let (edpb_sha256, reused) = migrate(item)?;
        report.push(json!({
            "source": item.path.file_name().unwrap().to_string_lossy(),
            "source_sha256": item.source_sha256,
            "output": item.output.file_name().unwrap().to_string_lossy(),
            "edpb_sha256": edpb_sha256,
            "reused_existing_verified_output": reused,
            "onlyid": item.actual_onlyid,
            "device_id": item.meta.device_id,
            "capture_level": "legacy_migrated"
        }));
    }

    let report_path = dir.join("legacy-edpb-migration-report.json");
    let report_bytes = serde_json::to_vec_pretty(&json!({
        "schema": "edpcli.legacy-edpb-migration.v1",
        "source_count": items.len(),
        "success_count": report.len(),
        "old_files_deleted": false,
        "entries": report
    }))
    .map_err(|e| e.to_string())?;
    fs::write(&report_path, report_bytes)
        .map_err(|e| format!("写迁移报告 {} 失败: {e}", report_path.display()))?;

    println!(
        "迁移完成: {}/{} 全部生成并通过 EDPB 完整校验；旧 .bin/.sha256 原样保留。",
        report.len(),
        items.len()
    );
    println!("报告: {}", report_path.display());
    Ok(())
}
