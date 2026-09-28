use super::*;
use std::collections::HashSet;

const MAX_INSPECT_SECTORS: usize = 65_536;

fn parse_lba_value(value: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("错误: LBA 须为非负十进制整数，得到 {value}"));
    }
    value
        .parse::<u64>()
        .map_err(|_| format!("错误: LBA 超出 u64 范围: {value}"))
}

fn push_unique_lba(out: &mut Vec<u64>, seen: &mut HashSet<u64>, lba: u64) -> Result<(), String> {
    if seen.insert(lba) {
        if out.len() >= MAX_INSPECT_SECTORS {
            return Err(format!(
                "错误: 单次 inspect 最多读取 {MAX_INSPECT_SECTORS} 个扇区"
            ));
        }
        out.push(lba);
    }
    Ok(())
}

fn parse_lbas(s: &str) -> Result<Vec<u64>, String> {
    if s.is_empty() {
        return Err("错误: --lba 缺少 LBA 值".into());
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for token in s.split(',') {
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_lba_value(start)?;
            let end = parse_lba_value(end)?;
            if start > end {
                return Err(format!("错误: LBA 范围起点大于终点: {token}"));
            }
            let span = end
                .checked_sub(start)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| format!("错误: LBA 范围溢出: {token}"))?;
            if span > MAX_INSPECT_SECTORS as u64 {
                return Err(format!(
                    "错误: 单个 LBA 范围最多包含 {MAX_INSPECT_SECTORS} 个扇区"
                ));
            }
            for lba in start..=end {
                push_unique_lba(&mut out, &mut seen, lba)?;
            }
        } else {
            push_unique_lba(&mut out, &mut seen, parse_lba_value(token)?)?;
        }
    }
    Ok(out)
}

fn parse_inspect_count(s: &str) -> Result<u64, String> {
    let count = parse_lba_value(s)?;
    if count == 0 || count > MAX_INSPECT_SECTORS as u64 {
        return Err(format!(
            "错误: --count 须为 1..={MAX_INSPECT_SECTORS}，得到 {s}"
        ));
    }
    Ok(count)
}

pub(super) fn parse_inspect(rest: &[String]) -> Result<Parsed, String> {
    if rest.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(Parsed::Help {
            topic: Some("inspect".into()),
        });
    }
    let Some(mode_text) = rest.first() else {
        return Err("错误: inspect 需要模式 raw / decode / meta".into());
    };
    let mode = InspectMode::parse(mode_text).ok_or_else(|| {
        format!(
            "错误: inspect 模式必须是 raw / decode / meta，得到 {}",
            mode_text
        )
    })?;
    let rest = rest[1..].to_vec();
    let mut opts = InspectOpts {
        mode,
        ..InspectOpts::default()
    };
    let mut lba_seen = false;
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a.starts_with('-') && a != "-" {
            match flag_name(a) {
                "--disk" => {
                    let v = take_value(&rest, &mut i, "--disk")?;
                    set_once(&mut opts.disk, parse_disk_spec(&v)?, "--disk")?;
                }
                "--lba" => {
                    if lba_seen {
                        return Err("错误: --lba 重复指定".into());
                    }
                    let v = take_value(&rest, &mut i, "--lba")?;
                    opts.lbas = parse_lbas(&v)?;
                    lba_seen = true;
                }
                "--count" => {
                    let v = take_value(&rest, &mut i, "--count")?;
                    set_once(&mut opts.count, parse_inspect_count(&v)?, "--count")?;
                }
                "--export" => {
                    let v = take_value(&rest, &mut i, "--export")?;
                    set_once(&mut opts.export, v, "--export")?;
                }
                "--id" => {
                    let v = take_value(&rest, &mut i, "--id")?;
                    set_once(&mut opts.device_id, v, "--id")?;
                }
                "--backup-dir" => {
                    let v = take_value(&rest, &mut i, "--backup-dir")?;
                    set_once(&mut opts.backup_dir, v, "--backup-dir")?;
                }
                other => return Err(format!("错误: inspect 不认识选项 {}", other)),
            }
        } else if !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "错误: inspect 不接受裸 LBA {}。请使用 --lba {}",
                a, a
            ));
        } else if opts.backup.is_none() {
            opts.backup = Some(a.to_string());
        } else {
            return Err(format!("错误: inspect 多余的位置参数: {}", a));
        }
        i += 1;
    }
    let source_count = usize::from(opts.disk.is_some()) + usize::from(opts.backup.is_some());
    if source_count > 1 {
        return Err("错误: inspect 的 --disk 与备份文件只能选一种".into());
    }
    if let Some(count) = opts.count {
        if opts.lbas.len() != 1 {
            return Err("错误: --count 只能与单个 --lba 起点同时使用".into());
        }
        let start = opts.lbas[0];
        opts.lbas.clear();
        for offset in 0..count {
            let lba = start
                .checked_add(offset)
                .ok_or_else(|| "错误: --count 产生的 LBA 范围溢出".to_string())?;
            opts.lbas.push(lba);
        }
    }
    Ok(Parsed::Inspect(opts))
}
