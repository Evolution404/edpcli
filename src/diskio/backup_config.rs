use super::*;

pub trait Clock {
    fn now_epoch(&self) -> i64;
    /// 备份文件名时间戳 %Y%m%d_%H%M%S。
    fn fmt_ts(&self, epoch: i64) -> String;
    /// 备份列表显示 %Y-%m-%d %H:%M。
    fn fmt_human(&self, epoch: i64) -> String;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_epoch(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
    fn fmt_ts(&self, epoch: i64) -> String {
        let (y, mo, d, h, mi, s) = local_parts(epoch);
        format!("{y:04}{mo:02}{d:02}_{h:02}{mi:02}{s:02}")
    }
    fn fmt_human(&self, epoch: i64) -> String {
        let (y, mo, d, h, mi, _) = local_parts(epoch);
        format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
    }
}

type UtcParts = (i64, u32, u32, u32, u32, u32);

fn local_parts(epoch: i64) -> UtcParts {
    let Ok(utc) = time::OffsetDateTime::from_unix_timestamp(epoch) else {
        return utc_parts(epoch);
    };
    let offset = time::UtcOffset::local_offset_at(utc).unwrap_or(time::UtcOffset::UTC);
    let local = utc.to_offset(offset);
    (
        local.year() as i64,
        u8::from(local.month()) as u32,
        local.day() as u32,
        local.hour() as u32,
        local.minute() as u32,
        local.second() as u32,
    )
}

/// Howard Hinnant civil_from_days: epoch → (年,月,日,时,分,秒) (UTC)。
pub(super) fn utc_parts(epoch: i64) -> UtcParts {
    let days = epoch.div_euclid(86400);
    let secs = epoch.rem_euclid(86400);
    let (h, mi, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32, h as u32, mi as u32, s as u32)
}

// ══════════════════════════════════════════════════════════════════
// 3. 备份/还原
// ══════════════════════════════════════════════════════════════════
/// 相对路径按 CWD 绝对化（跨提权重执行时也保持确定）。
pub fn absolutize_backup_dir(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    }
}

pub const CONF_NAME: &str = ".edpcli.conf";

fn absolutize_with(p: PathBuf, cwd: &Path) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        cwd.join(p)
    }
}

/// 四级优先级的纯逻辑: 旗标 > env > 配置文件 > cwd/backup(相对值按 cwd 绝对化)。
/// 环境读取留在 resolve_backup_dir 薄包装里, 便于单测。
pub fn resolve_backup_dir_impl(
    flag: Option<&str>,
    env_val: Option<String>,
    conf_val: Option<String>,
    cwd: PathBuf,
) -> PathBuf {
    if let Some(f) = flag {
        return absolutize_with(PathBuf::from(f), &cwd);
    }
    if let Some(v) = env_val.filter(|v| !v.is_empty()) {
        return absolutize_with(PathBuf::from(v), &cwd);
    }
    if let Some(v) = conf_val.filter(|v| !v.is_empty()) {
        return absolutize_with(PathBuf::from(v), &cwd);
    }
    cwd.join("backup")
}

/// `key = value` 配置解析: 取 backup_dir 值; `#` 注释, 未知键忽略, 坏行跳过。
pub fn parse_conf_backup_dir(content: &str) -> Option<String> {
    for line in content.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            if k.trim() == "backup_dir" {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 读取发起用户配置中的备份目录；用户 home 的平台差异由 platform 层处理。
pub fn conf_backup_dir() -> Option<String> {
    let home = crate::platform::invoking_user_home()?;
    let content = std::fs::read_to_string(home.join(CONF_NAME)).ok()?;
    parse_conf_backup_dir(&content)
}

/// 备份目录: --backup-dir 旗标 > $EDPCLI_BACKUP_DIR > ~/.edpcli.conf 的
/// backup_dir > CWD/backup。
pub fn resolve_backup_dir(flag: Option<&str>) -> PathBuf {
    resolve_backup_dir_impl(
        flag,
        std::env::var("EDPCLI_BACKUP_DIR").ok(),
        conf_backup_dir(),
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    )
}

/// 自动提权时的环境桥接：父进程把备份目录解析为绝对路径，以显式旗标并入
/// 重执行 argv（旗标优先于子进程环境）。
/// 返回应追加的参数(空 = 无需追加)。
pub fn backup_dir_argv_suffix(env_val: Option<String>) -> Vec<String> {
    match env_val.filter(|v| !v.is_empty()) {
        Some(v) => {
            let p = absolutize_backup_dir(PathBuf::from(&v));
            vec!["--backup-dir".to_string(), p.to_string_lossy().into_owned()]
        }
        None => vec![],
    }
}
