//! 真盘 IO、原子写入、备份/还原与快照读取。
//! 扇区设备抽象为 SectorDev — 这就是 Python 版 `_raw_path` 的 mock 点(升为参数)。

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::common::{NopwdError, NopwdResult, SECTOR, EXIT_INTERMEDIATE, EXIT_IO, EXIT_ROLLED_BACK};
use crate::md5::md5_hex;
use crate::sectors::looks_nopwd;

pub fn raw_path(disk: u32) -> String {
    format!("/dev/rdisk{}", disk)
}

fn io_err(e: io::Error) -> NopwdError {
    NopwdError::new(EXIT_IO, format!("错误: {}", e))
}

// ══════════════════════════════════════════════════════════════════
// 0. 扇区设备
// ══════════════════════════════════════════════════════════════════
pub trait SectorDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>>;
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()>;
    /// 写阶段前切换为 O_RDWR(卸载后调用)。默认无操作 — 测试镜像本就可写。
    fn reopen_rdwr(&mut self, _wait: Duration) -> io::Result<()> {
        Ok(())
    }
}

/// 打开镜像/raw 设备文件。流程以只读打开(挂载态可读); 写阶段经 reopen_rdwr
/// 在卸载后切换为 O_RDWR — 与 Python 版时序一致(dry-run 从不需要写权限,
/// O_RDWR 的 EBUSY 重试只发生在卸载之后)。
pub struct FileDev {
    path: String,
    file: File,
    writable: bool,
}

fn try_open_rdwr(path: &str, wait: Duration) -> io::Result<File> {
    let deadline = Instant::now() + wait;
    loop {
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => return Ok(file),
            Err(e) => {
                // EBUSY=16(macOS); 不用 ErrorKind — 其映射跨 Rust 版本有变
                if e.raw_os_error() == Some(16) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(200));
                } else {
                    return Err(e);
                }
            }
        }
    }
}

impl FileDev {
    pub fn open_rdonly(path: &str) -> io::Result<Self> {
        Ok(FileDev { path: path.to_string(), file: File::open(path)?, writable: false })
    }

    pub fn open_rdwr(path: &str, wait: Duration) -> io::Result<Self> {
        Ok(FileDev { path: path.to_string(), file: try_open_rdwr(path, wait)?, writable: true })
    }

    /// 切换为 O_RDWR(应在卸载后调用)。已可写则不重开, 保持单 fd 全程持有。
    pub fn reopen_rdwr(&mut self, wait: Duration) -> io::Result<()> {
        if self.writable {
            return Ok(());
        }
        self.file = try_open_rdwr(&self.path, wait)?;
        self.writable = true;
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

/// pwrite_full 等价: 以单次尝试闭包写满 data(短写循环, 0 视为失败)。
/// 旧版不查返回值, 短写会静默丢数据 — 抽出为独立函数以便测试注入"每次只写一半"。
pub fn pwrite_loop(
    mut attempt: impl FnMut(&[u8], u64) -> io::Result<usize>,
    data: &[u8],
    base: u64,
) -> io::Result<()> {
    let mut written = 0usize;
    while written < data.len() {
        let n = attempt(&data[written..], base + written as u64)?;
        if n == 0 {
            return Err(io::Error::other(format!(
                "pwrite 未写完(offset {:#x}, 剩 {}B)",
                base + written as u64,
                data.len() - written
            )));
        }
        written += n;
    }
    Ok(())
}

impl SectorDev for FileDev {
    fn reopen_rdwr(&mut self, wait: Duration) -> io::Result<()> {
        FileDev::reopen_rdwr(self, wait)
    }

    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; SECTOR];
        let mut filled = 0usize;
        let base = lba as u64 * SECTOR as u64;
        while filled < SECTOR {
            let n = self.file.read_at(&mut buf[filled..], base + filled as u64)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("LBA{} 读取提前 EOF", lba),
                ));
            }
            filled += n;
        }
        Ok(buf)
    }

    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} 以只读打开(未到写阶段)", self.path),
            ));
        }
        let base = lba as u64 * SECTOR as u64;
        let file = &self.file;
        pwrite_loop(|buf, off| file.write_at(buf, off), data, base)
    }
}

/// 单扇区便捷读(Python read_lba_disk 等价: 每次独立打开)。
pub fn read_lba(path: &str, lba: u32) -> io::Result<Vec<u8>> {
    FileDev::open_rdonly(path)?.read_sector(lba)
}

// ══════════════════════════════════════════════════════════════════
// 1. 原子写入(全有或全无)
// ══════════════════════════════════════════════════════════════════
fn write_and_verify(
    dev: &mut dyn SectorDev,
    sectors: &BTreeMap<u32, Vec<u8>>,
    order: &[u32],
) -> io::Result<()> {
    for &lba in order {
        dev.write_sector(lba, &sectors[&lba])?;
    }
    for &lba in sectors.keys() {
        if dev.read_sector(lba)? != sectors[&lba] {
            return Err(io::Error::other(format!("LBA{} 读回校验不符", lba)));
        }
    }
    Ok(())
}

/// 全有或全无写盘(patch={lba:512B 新内容})。
///
/// USB 盘硬件没有跨扇区事务, 严格原子不可得; 以四层逼近:
///   1) 单 fd 打开后全程持有 → 不存在中途重开撞 EBUSY/系统重扫的窗口;
///   2) LBA0(唯一改 MBR 的扇区)最后写 → 未到它之前系统视角的 MBR 仍是旧的;
///   3) 写完逐扇读回校验, 落盘与否以读回为准;
///   4) 任一失败 → 以写前内存镜像自动回滚全部扇区并再校验。
///
/// 回滚成功 → EXIT_ROLLED_BACK(盘仍为写前状态, 可安全重试);
/// 回滚失败 → EXIT_INTERMEDIATE(中间态, 指引重插后 nopwd restore 从备份还原)。
pub fn atomic_write_sectors(
    dev: &mut dyn SectorDev,
    patch: &BTreeMap<u32, Vec<u8>>,
) -> NopwdResult<()> {
    let mut order: Vec<u32> = patch.keys().copied().filter(|&l| l != 0).collect();
    order.sort_unstable();
    if patch.contains_key(&0) {
        order.push(0);
    }
    let mut mirror = BTreeMap::new();
    for &lba in patch.keys() {
        mirror.insert(lba, dev.read_sector(lba).map_err(io_err)?);
    }
    match write_and_verify(dev, patch, &order) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("!! 写入失败: {}", e);
            eprintln!("!! 自动回滚到本次写前状态 ...");
            for i in 0..3 {
                match write_and_verify(dev, &mirror, &order) {
                    Ok(()) => {
                        return Err(NopwdError::new(
                            EXIT_ROLLED_BACK,
                            "错误: 已完整回滚, 盘仍为写前状态(未改造)。可换 USB 口/线后重试, 或 nopwd restore 走还原流程。",
                        ))
                    }
                    Err(e2) => {
                        if i == 2 {
                            return Err(NopwdError::new(
                                EXIT_INTERMEDIATE,
                                format!("错误: 回滚亦失败({}) — 盘处于中间状态! 请重插后立即 nopwd restore 从备份还原。", e2),
                            ));
                        }
                        thread::sleep(Duration::from_millis(500));
                    }
                }
            }
            unreachable!()
        }
    }
}

// ══════════════════════════════════════════════════════════════════
// 2. 时钟(本地时间; 真实现借 /bin/date, 测试注入 FixedClock)
// ══════════════════════════════════════════════════════════════════
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
        date_fmt(epoch, "%Y%m%d_%H%M%S", |(y, mo, d, h, mi, s)| {
            format!("{:04}{:02}{:02}_{:02}{:02}{:02}", y, mo, d, h, mi, s)
        })
    }
    fn fmt_human(&self, epoch: i64) -> String {
        date_fmt(epoch, "%Y-%m-%d %H:%M", |(y, mo, d, h, mi, _)| {
            format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, mi)
        })
    }
}

type UtcParts = (i64, u32, u32, u32, u32, u32);
type UtcFormatter = fn(UtcParts) -> String;

fn date_fmt(epoch: i64, fmt: &str, utc: UtcFormatter) -> String {
    let date_fmt_str = format!("+{}", fmt);
    if let Ok(out) = Command::new("/bin/date").arg("-r").arg(epoch.to_string()).arg(&date_fmt_str).output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    // 兜底: UTC 换算(date 不可用时; 文件名场景仍保唯一性)
    utc(utc_parts(epoch))
}

/// Howard Hinnant civil_from_days: epoch → (年,月,日,时,分,秒) (UTC)。
fn utc_parts(epoch: i64) -> UtcParts {
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
/// 相对路径按 CWD 绝对化(跨 sudo 重执行时 CWD 不变的假设下仍更确定)。
pub fn absolutize_backup_dir(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    }
}

pub const CONF_NAME: &str = ".nopwd.conf";

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

/// sudo 下发起用户的用户名(sudo 设置, 可信; 形如系统用户名才接受)。
pub fn sudo_user() -> Option<String> {
    let u = std::env::var("SUDO_USER").ok()?;
    let ok = !u.is_empty()
        && u.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    ok.then_some(u)
}

/// 发起用户 home(经 shell `~user` 展开; std 无 getpwnam)。
pub fn sudo_user_home() -> Option<PathBuf> {
    let u = sudo_user()?;
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("echo ~{}", u))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s.starts_with('~') {
        None
    } else {
        Some(PathBuf::from(s))
    }
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

/// 读取用户配置中的备份目录。
/// 定位: sudo 下(手动或自动)读发起用户 home 的 .nopwd.conf — sudo 会剥掉
/// shell 环境变量($NOPWD_BACKUP_DIR 过不去), 磁盘文件是唯一能穿界的载体;
/// 非 root 读 $HOME。
pub fn conf_backup_dir() -> Option<String> {
    let home = match sudo_user() {
        Some(_) => sudo_user_home()?,
        None => PathBuf::from(std::env::var("HOME").ok()?),
    };
    let content = std::fs::read_to_string(home.join(CONF_NAME)).ok()?;
    parse_conf_backup_dir(&content)
}

/// 备份目录: --backup-dir 旗标 > $NOPWD_BACKUP_DIR > ~/.nopwd.conf 的
/// backup_dir > CWD/backup。
pub fn resolve_backup_dir(flag: Option<&str>) -> PathBuf {
    resolve_backup_dir_impl(
        flag,
        std::env::var("NOPWD_BACKUP_DIR").ok(),
        conf_backup_dir(),
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    )
}

/// 自动提权时的环境桥接: sudo 默认清环境变量(env_reset), $NOPWD_BACKUP_DIR
/// 过不去 — 父进程把它解析为绝对路径, 以显式旗标并入重执行 argv(旗标优先于 env)。
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

/// 备份命名所需盘事实(由 sysinfo/LBA4 预先收集, 测试可注入)。
pub struct DiskFacts {
    pub disk: u32,
    pub total_sectors: Option<u64>,
    pub vid: String,
    pub pid: String,
    pub label_id: Option<String>,
}

/// LBA4 开头的 `$$$<labelOnlyId>$$$` → 十进制字符串; 非法返回 None。
///
/// labelOnlyId 在部分盘上以有符号 32 位十进制文本保存；负的 10 位数连同
/// 分隔符需要 17B，因此不能只截取 16B。
pub fn lba4_label_id_from(head: &[u8]) -> Option<String> {
    if head.len() < 3 || &head[..3] != b"$$$" {
        return None;
    }
    let mut i = 3usize;
    let dstart = i; // 返回值含可选负号
    if head.get(i) == Some(&b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < head.len() && head[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return None; // '-' 后至少一位数字
    }
    if i + 3 > head.len() || &head[i..i + 3] != b"$$$" {
        return None;
    }
    Some(String::from_utf8_lossy(&head[dstart..i]).into_owned())
}

/// LBA4 前 16B 身份标签。短扇区安全返回 None，调用方不得假设读取层一定给满 512B。
pub fn lba4_tag16_from(raw: &[u8]) -> Option<[u8; 16]> {
    raw.get(..16)?.try_into().ok()
}

fn read_bytes_at(path: &Path, offset: u64, n: usize) -> io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; n];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// 直接从备份快照的 LBA4 读取 labelOnlyId，不依赖当前插入的真盘。
pub fn backup_label_id(path: &Path) -> Option<String> {
    read_bytes_at(path, 4 * SECTOR as u64, 32).ok().and_then(|b| lba4_label_id_from(&b))
}

/// 备份文件是否为免密状态快照(按内容检测, 与文件名无关)。
pub fn backup_is_nopwd(path: &Path, device_id: &str) -> bool {
    let Ok(data) = fs::read(path) else { return false };
    if data.len() < 14 * SECTOR {
        return false;
    }
    let read = |lba: u32| -> NopwdResult<Vec<u8>> { Ok(data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec()) };
    looks_nopwd(&read, device_id).unwrap_or(false)
}

fn has_onlyid(name: &str) -> bool {
    // `_onlyid-?\d+_` 存在?
    let b = name.as_bytes();
    let mut i = 0;
    while let Some(p) = name[i..].find("_onlyid") {
        let mut j = i + p + 7;
        if b.get(j) == Some(&b'-') {
            j += 1;
        }
        let dstart = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > dstart && b.get(j) == Some(&b'_') {
            return true;
        }
        i = i + p + 1;
    }
    false
}

fn replace_lid(name: &str, onlyid: &str) -> Option<String> {
    // 把首个 `_lid-?\d+_` 段替换为 `_onlyid{id}_`
    let b = name.as_bytes();
    let mut i = 0;
    while let Some(p) = name[i..].find("_lid") {
        let mut j = i + p + 4;
        if b.get(j) == Some(&b'-') {
            j += 1;
        }
        let dstart = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > dstart && b.get(j) == Some(&b'_') {
            let mut out = String::with_capacity(name.len() + 8);
            out.push_str(&name[..i + p]);
            out.push_str(&format!("_onlyid{}_", onlyid));
            out.push_str(&name[j..]);
            return Some(out);
        }
        i = i + p + 1;
    }
    None
}

pub fn ts_suffix_pos(name: &str) -> Option<usize> {
    // 尾部 `_\d{8}_\d{6}.bin` 的 '_' 位置
    if !name.ends_with(".bin") {
        return None;
    }
    let stem = &name[..name.len() - 4];
    let b = stem.as_bytes();
    if b.len() < 16 {
        return None;
    }
    let p = &b[b.len() - 16..];
    if p[0] == b'_'
        && p[1..9].iter().all(|c| c.is_ascii_digit())
        && p[9] == b'_'
        && p[10..16].iter().all(|c| c.is_ascii_digit())
    {
        Some(b.len() - 16)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupMeta {
    pub disk: u32,
    pub secs: Option<u64>,
    pub vid: String,
    pub pid: String,
    pub device_id: String,
    pub onlyid: Option<String>,
    pub tagged_nopwd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Md5Status {
    Ok,
    Mismatch,
    NoSidecar,
}

#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub meta: Option<BackupMeta>,
    pub path: PathBuf,
    pub mtime: i64,
    pub is_nopwd: bool,
    pub md5_ok: Md5Status,
    pub size_ok: bool,
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

/// 解析本工具备份文件名。device_id 自身含 `&` / `_`，因此不能按 `_` 粗暴 split；
/// 固定锚点只使用 disk/secs/vid/pid 与尾部时间戳/状态/onlyid。
pub fn parse_backup_name(name: &str) -> Option<BackupMeta> {
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

    let mut tail = &after_pid[device_pos + 1..];
    let tagged_nopwd = tail.ends_with("_nopwd");
    if tagged_nopwd {
        tail = &tail[..tail.len() - "_nopwd".len()];
    }
    let (device_id, onlyid) = match strip_numeric_suffix(tail, "_onlyid") {
        Some((did, id)) => (did, Some(id)),
        None => match strip_numeric_suffix(tail, "_lid") {
            Some((did, id)) => (did, Some(id)),
            None => (tail, None),
        },
    };
    if !device_id.starts_with("disk&ven_") {
        return None;
    }

    Some(BackupMeta {
        disk,
        secs,
        vid: vid.to_string(),
        pid: pid.to_string(),
        device_id: device_id.to_string(),
        onlyid,
        tagged_nopwd,
    })
}

fn md5_status(path: &Path, data: &[u8]) -> Md5Status {
    let sidecar = PathBuf::from(format!("{}.md5", path.display()));
    if !sidecar.exists() {
        return Md5Status::NoSidecar;
    }
    let Ok(expected) = fs::read_to_string(sidecar) else {
        return Md5Status::Mismatch;
    };
    let Some(expected) = expected.split_whitespace().next() else {
        return Md5Status::Mismatch;
    };
    if expected.eq_ignore_ascii_case(&md5_hex(data)) {
        Md5Status::Ok
    } else {
        Md5Status::Mismatch
    }
}

/// 扫描备份目录并给出跨盘管理所需的完整元数据。
///
/// 扫描必须是只读操作：旧 `_lid` 直接解析；完全没有 onlyid 的历史文件从其自身
/// LBA4 在内存中补齐 onlyid，不改名、不移动 `.bin/.md5`。未识别 `.bin` 仍保留。
pub fn scan_backup_dir(dir: &Path) -> Vec<BackupEntry> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let mut meta = parse_backup_name(name);
        if let Some(m) = meta.as_mut() {
            if m.onlyid.is_none() {
                m.onlyid = backup_label_id(&path);
            }
        }
        let data = fs::read(&path).ok();
        let size_ok = data.as_ref().map(|d| d.len() == 14 * SECTOR).unwrap_or(false);
        let md5_ok = data
            .as_ref()
            .map(|d| md5_status(&path, d))
            .unwrap_or(Md5Status::Mismatch);
        let is_nopwd = match (&meta, &data) {
            (Some(m), Some(d)) if d.len() >= 14 * SECTOR => {
                let read = |lba: u32| -> NopwdResult<Vec<u8>> {
                    Ok(d[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
                };
                looks_nopwd(&read, &m.device_id).unwrap_or(false)
            }
            _ => false,
        };
        entries.push(BackupEntry {
            meta,
            path: path.clone(),
            mtime: mtime_epoch(&path),
            is_nopwd,
            md5_ok,
            size_ok,
        });
    }
    entries.sort_by(|a, b| {
        b.mtime
            .cmp(&a.mtime)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
    entries
}

/// 同一物理盘的备份分组键。现代命名优先使用 onlyid；历史/缺失 onlyid 时退化为
/// (device_id, total sectors)。未识别文件不参与自动清理策略。
pub fn backup_group_key(entry: &BackupEntry) -> Option<String> {
    let meta = entry.meta.as_ref()?;
    Some(match &meta.onlyid {
        Some(id) => format!("onlyid:{id}"),
        None => format!(
            "legacy:{}:{}",
            meta.device_id,
            meta.secs.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into())
        ),
    })
}

/// `backup prune` 的纯策略层：
/// - 加密原盘备份从不成为候选；
/// - 每盘只对已按内容确认的免密快照按 mtime 新→旧保留 `keep` 份；
/// - 若该盘组没有任何加密原盘备份，则至少保留最新 1 份快照，防止清到零份。
pub fn prune_candidates(entries: &[BackupEntry], keep: usize) -> Vec<PathBuf> {
    let mut groups: BTreeMap<String, Vec<&BackupEntry>> = BTreeMap::new();
    for entry in entries {
        if let Some(key) = backup_group_key(entry) {
            groups.entry(key).or_default().push(entry);
        }
    }

    let mut out = Vec::new();
    for group in groups.values() {
        let has_original = group.iter().any(|e| !e.is_nopwd);
        let mut snaps: Vec<&BackupEntry> = group.iter().copied().filter(|e| e.is_nopwd).collect();
        snaps.sort_by(|a, b| {
            b.mtime
                .cmp(&a.mtime)
                .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
        });
        let preserve = if has_original { keep } else { keep.max(1) };
        let mut deletable: Vec<&BackupEntry> = snaps.into_iter().skip(preserve).collect();
        // 候选清单按最旧→较新展示/删除，便于人工核对；保留判定仍严格按最新优先。
        deletable.reverse();
        out.extend(deletable.into_iter().map(|e| e.path.clone()));
    }
    out
}

/// 把历史备份文件名统一为 `_onlyid<labelOnlyId>_`，并同步改名 .md5。
///
/// 兼容早期 `_lid..._` 命名以及完全没有 onlyid 段的历史备份。onlyid 始终
/// 从该备份自身的 LBA4 读取，避免依赖当前磁盘或按型号猜测。
pub fn migrate_backup_names(bak_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    if !bak_dir.is_dir() {
        return vec![];
    }
    let mut renamed = Vec::new();
    let Ok(entries) = fs::read_dir(bak_dir) else { return vec![] };
    let mut bins: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "bin").unwrap_or(false))
        .collect();
    bins.sort(); // 确定性
    for path in bins {
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if has_onlyid(&name) {
            continue;
        }
        let onlyid = match backup_label_id(&path) {
            Some(o) => o,
            None => continue,
        };
        let new_name = if let Some(n) = replace_lid(&name, &onlyid) {
            n
        } else if let Some(pos) = ts_suffix_pos(&name) {
            format!("{}_onlyid{}{}", &name[..pos], onlyid, &name[pos..])
        } else {
            continue;
        };
        let new_path = bak_dir.join(&new_name);
        if new_path.exists() {
            println!("警告: 历史备份改名目标已存在，跳过: {}", new_path.display());
            continue;
        }
        match fs::rename(&path, &new_path) {
            Ok(()) => {
                let old_md5 = bak_dir.join(format!("{}.md5", name));
                let new_md5 = bak_dir.join(format!("{}.md5", new_name));
                if old_md5.exists() {
                    let _ = fs::rename(&old_md5, &new_md5);
                }
                renamed.push((path, new_path));
            }
            Err(e) => {
                println!("警告: 历史备份无法改名: {} ({})", path.display(), e);
                continue;
            }
        }
    }
    renamed
}

/// 备份 LBA0-13 到备份目录, 附 .md5 sidecar。
/// 返回 (备份路径, 是否免密状态快照); `还原:` 提示由 CLI 打印。
pub fn backup_disk(
    facts: &DiskFacts,
    data: &[u8],
    device_id: &str,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> NopwdResult<(PathBuf, bool)> {
    fs::create_dir_all(bak_dir).map_err(io_err)?;
    let ts = clock.fmt_ts(clock.now_epoch());
    let secs = facts.total_sectors.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into());
    let onlyid_part = facts
        .label_id
        .as_ref()
        .map(|o| format!("_onlyid{}", o))
        .unwrap_or_default();
    // 免密状态快照打 _nopwd 标: 区别于加密原盘备份, 防止还原时拿错
    let is_nopwd = {
        let read = |lba: u32| -> NopwdResult<Vec<u8>> {
            Ok(data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
        };
        looks_nopwd(&read, device_id).unwrap_or(false)
    };
    let state_part = if is_nopwd { "_nopwd" } else { "" };
    let base = format!(
        "disk{}_{}_vid{}_pid{}_{}{}{}_{}",
        facts.disk, secs, facts.vid, facts.pid, device_id, onlyid_part, state_part, ts
    );
    let path = bak_dir.join(format!("{}.bin", base));
    fs::write(&path, data).map_err(io_err)?;
    // sidecar 命名与 Python 版一致: <备份.bin>.md5
    let md5_path = PathBuf::from(format!("{}.md5", path.display()));
    fs::write(&md5_path, format!("{}\n", md5_hex(data))).map_err(io_err)?;
    println!("{}  {}", crate::ui::green("备份"), path.display());
    if is_nopwd {
        println!(
            "{}",
            crate::ui::yellow("注意: 本份备份为【免密状态】快照 — 还原它不会回到加密原盘。")
        );
    }
    Ok((path, is_nopwd))
}

/// 只支持 `*` 的通配匹配(device_id/文件名只含 &/_/字母数字, 无其它元字符)。
pub fn wildcard_match(pat: &str, text: &str) -> bool {
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == text;
    }
    let first = parts[0];
    let last = parts[parts.len() - 1];
    if !text.starts_with(first) || !text.ends_with(last) {
        return false;
    }
    if first.len() + last.len() > text.len() {
        return false;
    }
    let mut idx = first.len();
    let bound = text.len() - last.len();
    for part in &parts[1..parts.len() - 1] {
        match find_in_range(text, idx, bound, part) {
            Some(end) => idx = end,
            None => return false,
        }
    }
    true
}

fn find_in_range(text: &str, from: usize, to: usize, part: &str) -> Option<usize> {
    if from > to {
        return if part.is_empty() { Some(from) } else { None };
    }
    text[from..to].find(part).map(|p| from + p + part.len())
}

pub fn mtime_epoch(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 匹配备份目录中本盘备份, 新→旧排序。
/// 注意: device_id/总扇区/VID/PID 均非盘唯一(同型号盘全同), 最终以
/// LBA4 labelOnlyId(每盘随机唯一, 明文) 终验剔除他盘备份。
/// my_tag 为本盘 LBA4 前 16 字节; None/全零 跳过终验。
/// 兼容旧命名(device_id 中 & 被替换为 _)。
pub fn find_backups(
    bak_dir: &Path,
    facts: &DiskFacts,
    device_id: Option<&str>,
    my_tag: Option<[u8; 16]>,
) -> Vec<PathBuf> {
    if !bak_dir.is_dir() {
        return vec![];
    }
    let secs = facts.total_sectors.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into());
    let mut tiers: Vec<Vec<String>> = Vec::new();
    if let Some(did) = device_id {
        tiers.push(vec![
            format!("disk*_{}_vid{}_pid{}_{}_*", secs, facts.vid, facts.pid, did),
            format!("disk*_{}_vid{}_pid{}_{}_*", secs, facts.vid, facts.pid, did.replace('&', "_")),
        ]);
    }
    tiers.push(vec![format!("disk*_{}_vid{}_pid{}_*", secs, facts.vid, facts.pid)]); // 兜底(识别失败时)
    for pats in &tiers {
        let mut out: Vec<PathBuf> = Vec::new();
        for pat in pats {
            let Ok(entries) = fs::read_dir(bak_dir) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(".md5") {
                    continue;
                }
                if wildcard_match(pat, &name) {
                    out.push(entry.path());
                }
            }
        }
        if out.is_empty() {
            continue;
        }
        // LBA4 终验: 剔除同型号他盘的备份
        if let Some(tag) = my_tag.filter(|t| t.iter().any(|&b| b != 0)) {
            out.retain(|f| {
                read_bytes_at(f, 4 * SECTOR as u64, 16)
                    .map(|b| b[..16] == tag)
                    .unwrap_or(false)
            });
        }
        out.sort_by(|a, b| {
            mtime_epoch(b)
                .cmp(&mtime_epoch(a))
                .then_with(|| a.file_name().cmp(&b.file_name()))
        });
        out.dedup();
        return out;
    }
    Vec::new()
}

// ══════════════════════════════════════════════════════════════════
// 4. 快照读取
// ══════════════════════════════════════════════════════════════════
/// 快照目录读扇区, 兼容 LBA7.bin / LBA07.bin 命名。缺失返回全零扇区。
pub fn read_lba_file(dir: &Path, lba: u32) -> Vec<u8> {
    for name in [format!("LBA{}.bin", lba), format!("LBA{:02}.bin", lba)] {
        let p = dir.join(name);
        if p.exists() {
            if let Ok(data) = fs::read(&p) {
                // Python f.read(SECTOR): 截到 512; 不足补零(短文件在 Python 会半路崩, 这里安全失败)
                let mut v = data;
                v.truncate(SECTOR);
                v.resize(SECTOR, 0);
                return v;
            }
        }
    }
    vec![0u8; SECTOR]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_id_positive_negative_malformed() {
        let pos: Vec<u8> = b"$$$1402259934$$$".iter().chain([0u8; 18].iter()).copied().collect();
        assert_eq!(lba4_label_id_from(&pos), Some("1402259934".to_string()));
        let neg: Vec<u8> = b"$$$-1833210541$$$".iter().chain([0u8; 15].iter()).copied().collect();
        assert_eq!(lba4_label_id_from(&neg), Some("-1833210541".to_string()));
        assert_eq!(lba4_label_id_from(b""), None);
        assert_eq!(lba4_label_id_from(&[0u8; 32]), None);
        assert_eq!(lba4_label_id_from(b"@@@1@@@"), None);
        assert_eq!(lba4_label_id_from(b"$$$-$$$"), None); // '-' 后无数字
        assert_eq!(lba4_label_id_from(b"$$$$$$"), None); // 无数字
    }

    #[test]
    fn wildcard_only_star() {
        assert!(wildcard_match("a*c*", "abc"));
        assert!(wildcard_match("disk*_122880000_vid0dd8_pid2005_x_*.bin",
            "disk6_122880000_vid0dd8_pid2005_x_onlyid1402259934_20260910_172300.bin"));
        assert!(!wildcard_match("disk*_999_*", "disk6_122880000_x"));
        assert!(wildcard_match("abc", "abc"));
        assert!(!wildcard_match("abc", "abcd"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("a**b", "ab"));
        assert!(!wildcard_match("a*b", "a")); // 尾段放不下
    }

    #[test]
    fn read_lba_file_naming() {
        let d = std::env::temp_dir().join(format!("nopwd_test_{}_lbafile", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("LBA7.bin"), vec![b'7'; SECTOR]).unwrap();
        fs::write(d.join("LBA12.bin"), vec![b'c'; SECTOR]).unwrap();
        assert_eq!(read_lba_file(&d, 7), vec![b'7'; SECTOR]); // 无前导零
        assert_eq!(read_lba_file(&d, 12), vec![b'c'; SECTOR]);
        assert_eq!(read_lba_file(&d, 9), vec![0u8; SECTOR]); // 缺失→全零
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn backup_dir_priority_and_conf_parse() {
        let cwd = PathBuf::from("/w");
        // 四级优先: 旗标 > env > conf > CWD 兜底
        assert_eq!(
            resolve_backup_dir_impl(Some("/f"), Some("/e".into()), Some("/c".into()), cwd.clone()),
            PathBuf::from("/f")
        );
        assert_eq!(
            resolve_backup_dir_impl(None, Some("/e".into()), Some("/c".into()), cwd.clone()),
            PathBuf::from("/e")
        );
        assert_eq!(
            resolve_backup_dir_impl(None, None, Some("/c".into()), cwd.clone()),
            PathBuf::from("/c")
        );
        assert_eq!(
            resolve_backup_dir_impl(None, None, None, cwd.clone()),
            PathBuf::from("/w/backup")
        );
        // env 空串视同未设 → conf 兜底
        assert_eq!(
            resolve_backup_dir_impl(None, Some(String::new()), Some("/c".into()), cwd.clone()),
            PathBuf::from("/c")
        );
        // 相对值按 cwd 绝对化
        assert_eq!(
            resolve_backup_dir_impl(None, Some("bk".into()), None, cwd.clone()),
            PathBuf::from("/w/bk")
        );
        // conf 解析: 注释/坏行/空值/未知键
        assert_eq!(
            parse_conf_backup_dir("# 注释\nbackup_dir = /Users/x/.nopwd-backup\n"),
            Some("/Users/x/.nopwd-backup".to_string())
        );
        assert_eq!(parse_conf_backup_dir("backup_dir=/a/b"), Some("/a/b".to_string()));
        assert_eq!(parse_conf_backup_dir("backup_dir =   \n"), None); // 空值
        assert_eq!(parse_conf_backup_dir("other = 1\nnoise\n"), None);
        assert_eq!(parse_conf_backup_dir(""), None);
    }

    #[test]
    fn sudo_user_name_validated() {
        // 形如系统用户名才接受(防 shell 插值注入)
        assert!(sudo_user().is_some() || std::env::var("SUDO_USER").is_err());
        // 直接验证判定逻辑(无 SUDO_USER 环境时)
        let ok = |s: &str| {
            !s.is_empty()
                && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        };
        assert!(ok("zhangyuxi") && ok("a.b-c_1"));
        assert!(!ok("") && !ok("x; rm") && !ok("$(cmd)"));
    }

    #[test]
    fn backup_dir_argv_suffix_bridges_env() {
        // sudo env_reset 会清环境变量 — env 值须转为显式旗标(绝对路径)随 argv 过界
        let abs = backup_dir_argv_suffix(Some("/Users/x/.nopwd-backup".into()));
        assert_eq!(abs, vec!["--backup-dir".to_string(), "/Users/x/.nopwd-backup".into()]);
        // 相对值按 CWD 绝对化
        let rel = backup_dir_argv_suffix(Some("bk".into()));
        assert_eq!(rel.len(), 2);
        assert!(rel[1].starts_with('/'), "{}", rel[1]);
        assert!(rel[1].ends_with("/bk"), "{}", rel[1]);
        // 未设/空值 → 不追加
        assert!(backup_dir_argv_suffix(None).is_empty());
        assert!(backup_dir_argv_suffix(Some(String::new())).is_empty());
    }

    #[test]
    fn utc_parts_known_date() {
        // 以 Python datetime(UTC) 校准: 2026-09-17 00:00:00 UTC = 1789603200
        assert_eq!(utc_parts(1789603200), (2026, 9, 17, 0, 0, 0));
        assert_eq!(utc_parts(1789660800), (2026, 9, 17, 16, 0, 0));
        assert_eq!(utc_parts(0), (1970, 1, 1, 0, 0, 0));
    }

    struct FixedClock;
    impl Clock for FixedClock {
        fn now_epoch(&self) -> i64 { 1789660800 }
        fn fmt_ts(&self, epoch: i64) -> String { format!("fixed_{}", epoch) }
        fn fmt_human(&self, epoch: i64) -> String { format!("h_{}", epoch) }
    }

    #[test]
    fn clock_fmt_uses_date_cmd() {
        let c = SystemClock;
        // /bin/date 在 macOS 必在; 校验格式形状
        let ts = c.fmt_ts(1789660800);
        assert_eq!(ts.len(), 15, "{}", ts); // YYYYmmdd_HHMMSS
        let h = c.fmt_human(1789660800);
        assert_eq!(h.len(), 16, "{}", h); // YYYY-mm-dd HH:MM
        let _ = FixedClock.fmt_ts(1);
    }
}
