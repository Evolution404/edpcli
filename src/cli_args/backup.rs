use super::*;

fn parse_keep(s: &str) -> Result<usize, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("错误: --keep 须为大于等于 0 的整数, 得到 {}", s));
    }
    s.parse::<usize>()
        .map_err(|_| format!("错误: --keep 超出范围: {}", s))
}

pub(super) fn parse_backup(rest: &[String]) -> Result<Parsed, String> {
    if rest.iter().any(|a| a == "-h" || a == "--help")
        || rest.first().map(String::as_str) == Some("help")
    {
        return Ok(Parsed::Help {
            topic: Some("backup".into()),
        });
    }
    // 人工使用时 `edpcli backup` 的自然含义就是“看看有哪些备份”。
    // 若第一个 token 是旗标，也按省略 `list` 处理。
    let (action_name, tail): (&str, &[String]) = match rest.first() {
        None => ("list", rest),
        Some(s) if s.starts_with('-') => ("list", rest),
        Some(s) => (s.as_str(), &rest[1..]),
    };
    let mut backup_dir = None;
    let mut keep = None;
    let mut yes = false;
    let action = match action_name {
        "create" => {
            let mut disk = None;
            let mut deep = false;
            let mut i = 0;
            while i < tail.len() {
                match flag_name(&tail[i]) {
                    "--deep" => set_switch(&mut deep, &tail[i], "--deep")?,
                    "--disk" => {
                        let v = take_value(tail, &mut i, "--disk")?;
                        set_once(&mut disk, parse_disk_spec(&v)?, "--disk")?;
                    }
                    "--backup-dir" => {
                        let v = take_value(tail, &mut i, "--backup-dir")?;
                        set_once(&mut backup_dir, v, "--backup-dir")?;
                    }
                    other => return Err(format!("错误: backup create 不认识选项 {}", other)),
                }
                i += 1;
            }
            BackupAction::Create { disk, deep }
        }
        "list" => {
            let mut i = 0;
            while i < tail.len() {
                match flag_name(&tail[i]) {
                    "--backup-dir" => {
                        let v = take_value(tail, &mut i, "--backup-dir")?;
                        set_once(&mut backup_dir, v, "--backup-dir")?;
                    }
                    other => return Err(format!("错误: backup list 不认识选项 {}", other)),
                }
                i += 1;
            }
            BackupAction::List
        }
        "verify" => {
            let mut target = None;
            let mut i = 0;
            while i < tail.len() {
                let a = tail[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--backup-dir" => {
                            let v = take_value(tail, &mut i, "--backup-dir")?;
                            set_once(&mut backup_dir, v, "--backup-dir")?;
                        }
                        other => return Err(format!("错误: backup verify 不认识选项 {}", other)),
                    }
                } else if target.is_none() {
                    target = Some(a.to_string());
                } else {
                    return Err(format!("错误: backup verify 只接受一个备份文件参数({})", a));
                }
                i += 1;
            }
            BackupAction::Verify { target }
        }
        "restore" => {
            let mut target = None;
            let mut disk = None;
            let mut i = 0;
            while i < tail.len() {
                let a = tail[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--disk" => {
                            let v = take_value(tail, &mut i, "--disk")?;
                            set_once(&mut disk, parse_disk_spec(&v)?, "--disk")?;
                        }
                        "--yes" => set_switch(&mut yes, a, "--yes")?,
                        "--backup-dir" => {
                            let v = take_value(tail, &mut i, "--backup-dir")?;
                            set_once(&mut backup_dir, v, "--backup-dir")?;
                        }
                        other => return Err(format!("错误: backup restore 不认识选项 {}", other)),
                    }
                } else if target.is_none() {
                    target = Some(a.to_string());
                } else {
                    return Err(format!(
                        "错误: backup restore 只接受一个备份编号或文件参数({})",
                        a
                    ));
                }
                i += 1;
            }
            BackupAction::Restore { target, disk }
        }
        "prune" => {
            let mut i = 0;
            while i < tail.len() {
                match flag_name(&tail[i]) {
                    "--keep" => {
                        let v = take_value(tail, &mut i, "--keep")?;
                        set_once(&mut keep, parse_keep(&v)?, "--keep")?;
                    }
                    "--yes" => set_switch(&mut yes, &tail[i], "--yes")?,
                    "--backup-dir" => {
                        let v = take_value(tail, &mut i, "--backup-dir")?;
                        set_once(&mut backup_dir, v, "--backup-dir")?;
                    }
                    other => return Err(format!("错误: backup prune 不认识选项 {}", other)),
                }
                i += 1;
            }
            BackupAction::Prune
        }
        "delete" => {
            let mut targets = Vec::new();
            let mut i = 0;
            while i < tail.len() {
                let a = tail[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--yes" => set_switch(&mut yes, a, "--yes")?,
                        "--backup-dir" => {
                            let v = take_value(tail, &mut i, "--backup-dir")?;
                            set_once(&mut backup_dir, v, "--backup-dir")?;
                        }
                        other => return Err(format!("错误: backup delete 不认识选项 {}", other)),
                    }
                } else {
                    targets.push(a.to_string());
                }
                i += 1;
            }
            if targets.is_empty() && yes {
                return Err("错误: backup delete --yes 必须显式给出编号或文件".into());
            }
            BackupAction::Delete { targets }
        }
        "rm" => return Err("错误: v2 已取消 backup rm。请使用: edpcli backup delete".into()),
        other => {
            return Err(format!(
            "错误: 未知 backup 动作: {} (可用 create / list / restore / verify / delete / prune)",
            other
        ))
        }
    };
    Ok(Parsed::Backup {
        action,
        keep: keep.unwrap_or(2),
        yes,
        backup_dir,
    })
}
