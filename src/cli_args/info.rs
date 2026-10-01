use super::*;

pub(super) fn parse_info(rest: &[String]) -> Result<Parsed, String> {
    if rest.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(Parsed::Help {
            topic: Some("info".into()),
        });
    }
    let mut opts = InfoOpts::default();
    let mut i = 0usize;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a.starts_with('-') && a != "-" {
            match flag_name(a) {
                "--disk" => {
                    let v = take_value(rest, &mut i, "--disk")?;
                    set_once(&mut opts.disk, parse_disk_spec(&v)?, "--disk")?;
                }
                "--id" => {
                    let v = take_value(rest, &mut i, "--id")?;
                    set_once(&mut opts.device_id, v, "--id")?;
                }
                "--backup-dir" => {
                    let v = take_value(rest, &mut i, "--backup-dir")?;
                    set_once(&mut opts.backup_dir, v, "--backup-dir")?;
                }
                other => return Err(format!("错误: info 不认识选项 {}", other)),
            }
        } else if opts.backup.is_none() {
            if opts.disk.is_some() {
                return Err("错误: info 的备份文件不能与 --disk 同时使用".into());
            }
            opts.backup = Some(a.to_string());
        } else {
            return Err(format!("错误: info 只接受一个备份文件参数({})", a));
        }
        i += 1;
    }
    if opts.disk.is_some() && opts.backup.is_some() {
        return Err("错误: info 的 --disk 与备份文件只能选一种".into());
    }
    Ok(Parsed::Info(opts))
}
