//! 真盘 IO、原子写入、备份/还原与快照读取。
//! 扇区设备抽象为 SectorDev — 这就是 Python 版 `_raw_path` 的 mock 点(升为参数)。

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::common::{
    EdpCliError, EdpCliResult, EXIT_BACKUP, EXIT_INTERMEDIATE, EXIT_IO, EXIT_ROLLED_BACK, SECTOR,
};
use crate::sectors::looks_nopwd;
use crate::sha256::sha256_hex;

pub fn raw_path(disk: u32) -> String {
    crate::platform::raw_disk_path(disk)
}

fn io_err(e: io::Error) -> EdpCliError {
    EdpCliError::new(EXIT_IO, format!("错误: {}", e))
}

fn validate_backup_device_id(device_id: &str) -> EdpCliResult<()> {
    let safe = device_id.starts_with("disk&ven_")
        && device_id.len() <= 128
        && device_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'&' | b'.' | b'-'));
    if safe {
        Ok(())
    } else {
        Err(EdpCliError::new(
            EXIT_BACKUP,
            format!(
                "错误: device_id 含不安全的备份文件名字符或长度异常，拒绝创建备份: {:?}",
                device_id
            ),
        ))
    }
}

fn write_new_synced(path: &Path, data: &[u8], label: &str) -> EdpCliResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                EdpCliError::new(
                    EXIT_IO,
                    format!("错误: {}已存在，拒绝覆盖: {}", label, path.display()),
                )
            } else {
                io_err(e)
            }
        })?;
    if let Err(e) = file.write_all(data).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(io_err(e));
    }
    Ok(())
}

fn sync_dir(dir: &Path) -> EdpCliResult<()> {
    crate::platform::sync_directory(dir).map_err(io_err)
}

// ══════════════════════════════════════════════════════════════════
// 0. 扇区设备
// ══════════════════════════════════════════════════════════════════
pub trait SectorDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>>;
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()>;
    /// 将此前写入提交到设备/介质。测试假件默认无操作；真实 FileDev 覆盖实现。
    fn sync(&mut self) -> io::Result<()> {
        Ok(())
    }
    /// 写阶段前切换为 O_RDWR(卸载后调用)。默认无操作 — 测试镜像本就可写。
    fn reopen_rdwr(&mut self, _wait: Duration) -> io::Result<()> {
        Ok(())
    }
}

/// 只读展示/诊断路径的扇区缓存。
///
/// `info` / `inspect` 可能先读取身份扇区，再由摘要/渲染阶段请求同一 LBA。
/// 这些路径不承担写入前后的新鲜度校验，因此可以在一次命令会话内复用已读数据，
/// 避免重复访问同一裸盘扇区。apply/restore 的安全复核不得使用该缓存。
pub struct SectorReadCache<'a> {
    dev: &'a mut dyn SectorDev,
    cache: BTreeMap<u32, Vec<u8>>,
}

impl<'a> SectorReadCache<'a> {
    pub fn new(dev: &'a mut dyn SectorDev) -> Self {
        Self {
            dev,
            cache: BTreeMap::new(),
        }
    }

    pub fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        if let Some(data) = self.cache.get(&lba) {
            return Ok(data.clone());
        }
        let data = self.dev.read_sector(lba)?;
        self.cache.insert(lba, data.clone());
        Ok(data)
    }
}

/// `list` 等只读多盘扫描使用的设备池：同一 disk 在一次命令会话内只打开一次。
/// 写盘流程不得复用该类型，避免跨安全检查持有旧设备句柄。
pub(crate) struct ReadOnlyDiskPool<F, D>
where
    F: FnMut(u32) -> io::Result<D>,
    D: SectorDev,
{
    open: F,
    devices: BTreeMap<u32, D>,
}

impl<F, D> ReadOnlyDiskPool<F, D>
where
    F: FnMut(u32) -> io::Result<D>,
    D: SectorDev,
{
    pub(crate) fn new(open: F) -> Self {
        Self {
            open,
            devices: BTreeMap::new(),
        }
    }

    pub(crate) fn read_sector(&mut self, disk: u32, lba: u32) -> io::Result<Vec<u8>> {
        if !self.devices.contains_key(&disk) {
            let dev = (self.open)(disk)?;
            self.devices.insert(disk, dev);
        }
        self.devices
            .get_mut(&disk)
            .expect("刚插入的只读设备必须存在")
            .read_sector(lba)
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
                if crate::platform::raw_busy_error(&e) && Instant::now() < deadline {
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
        Ok(FileDev {
            path: path.to_string(),
            file: File::open(path)?,
            writable: false,
        })
    }

    pub fn open_rdwr(path: &str, wait: Duration) -> io::Result<Self> {
        Ok(FileDev {
            path: path.to_string(),
            file: try_open_rdwr(path, wait)?,
            writable: true,
        })
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
        let base = lba as u64 * SECTOR as u64;
        self.file.seek(SeekFrom::Start(base))?;
        self.file.read_exact(&mut buf).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                io::Error::new(error.kind(), format!("LBA{} 读取提前 EOF", lba))
            } else {
                error
            }
        })?;
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
        self.file.seek(SeekFrom::Start(base))?;
        self.file.write_all(data)
    }

    fn sync(&mut self) -> io::Result<()> {
        if crate::platform::is_raw_device_path(&self.path) {
            crate::platform::sync_raw_device(&self.file)
        } else {
            self.file.sync_all()
        }
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
    // 读回前先把写缓存提交到介质；否则紧随其后的 pread 可能只验证到内核缓存。
    dev.sync()?;
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
/// 回滚失败 → EXIT_INTERMEDIATE(中间态, 指引重插后 edpcli backup restore 从备份还原)。
pub fn atomic_write_sectors(
    dev: &mut dyn SectorDev,
    patch: &BTreeMap<u32, Vec<u8>>,
) -> EdpCliResult<()> {
    for (&lba, data) in patch {
        if lba > crate::common::METADATA_LAST_LBA {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!(
                    "错误: 原子写仅允许元数据 LBA0-{}，收到 LBA{}",
                    crate::common::METADATA_LAST_LBA,
                    lba
                ),
            ));
        }
        if data.len() != SECTOR {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!(
                    "错误: LBA{} 写入数据长度 {}B，必须恰好为一个扇区 {}B",
                    lba,
                    data.len(),
                    SECTOR
                ),
            ));
        }
    }
    if patch.is_empty() {
        return Ok(());
    }
    // 在第一笔写入前先验证设备支持持久化屏障。若 raw USB 控制器不支持
    // DKIOCSYNCHRONIZECACHE，应在 0 写入状态下失败，而不是写完后才发现。
    dev.sync().map_err(|e| {
        EdpCliError::new(
            EXIT_IO,
            format!("错误: 写前介质缓存同步预检失败，拒绝开始写入: {}", e),
        )
    })?;
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
        Err(_write_error) => {
            for i in 0..3 {
                match write_and_verify(dev, &mirror, &order) {
                    Ok(()) => {
                        return Err(EdpCliError::new(
                            EXIT_ROLLED_BACK,
                            "错误: 已完整回滚, 盘仍为写前状态(未改造)。可换 USB 口/线后重试, 或 edpcli backup restore 走还原流程。",
                        ))
                    }
                    Err(e2) => {
                        if i == 2 {
                            return Err(EdpCliError::new(
                                EXIT_INTERMEDIATE,
                                format!("错误: 回滚亦失败({}) — 盘处于中间状态! 请重插后立即 edpcli backup restore 从备份还原。", e2),
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
// 2. 时钟(本地时间; 进程内换算, 测试注入 FixedClock)
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
    read_bytes_at(path, 4 * SECTOR as u64, 32)
        .ok()
        .and_then(|b| lba4_label_id_from(&b))
}

/// 备份文件是否为免密状态快照(按内容检测, 与文件名无关)。
pub fn backup_is_nopwd(path: &Path, device_id: &str) -> bool {
    let Ok(data) = fs::read(path) else {
        return false;
    };
    image_is_nopwd(&data, device_id)
}

/// 已在内存中的 LBA0-12 镜像是否为免密状态。
/// 供扫描、restore、备份创建共用，避免上层重复构造扇区闭包或二次读文件。
pub fn image_is_nopwd(data: &[u8], device_id: &str) -> bool {
    if data.len() < crate::common::METADATA_IMAGE_LEN {
        return false;
    }
    let read = |lba: u32| -> EdpCliResult<Vec<u8>> {
        Ok(data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
    };
    looks_nopwd(&read, device_id).unwrap_or(false)
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

/// 备份文件名尾部 `_YYYYMMDD_HHMMSS.bin` 转为可直接比较的 YYYYMMDDHHMMSS 数值。
/// 这是备份真实创建时间；文件系统 mtime 可能因复制/touch 改变，只作为旧文件兜底。
pub fn backup_name_time_key(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let pos = ts_suffix_pos(name)?;
    let end = name.len().checked_sub(4)?;
    let stamp = name.get(pos + 1..end)?;
    let mut digits = String::with_capacity(14);
    for ch in stamp.bytes() {
        if ch == b'_' {
            continue;
        }
        if !ch.is_ascii_digit() {
            return None;
        }
        digits.push(ch as char);
    }
    if digits.len() != 14 {
        return None;
    }
    digits.parse().ok()
}

pub fn backup_name_time_human(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let pos = ts_suffix_pos(name)?;
    let end = name.len().checked_sub(4)?;
    let stamp = name.get(pos + 1..end)?;
    if stamp.len() != 15 {
        return None;
    }
    Some(format!(
        "{}-{}-{} {}:{}",
        &stamp[0..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13]
    ))
}

/// `sort_by` 可直接使用的“最新备份优先”比较器。
/// 两边都有文件名时间时完全忽略 mtime；无法解析旧命名时才退回 mtime。
pub fn cmp_backup_newest_first(a: &BackupEntry, b: &BackupEntry) -> Ordering {
    match (backup_name_time_key(&a.path), backup_name_time_key(&b.path)) {
        (Some(at), Some(bt)) => bt.cmp(&at).then_with(|| a.path.cmp(&b.path)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => b.mtime.cmp(&a.mtime).then_with(|| a.path.cmp(&b.path)),
    }
}

pub fn backup_display_time(path: &Path, mtime: i64) -> String {
    backup_name_time_human(path).unwrap_or_else(|| SystemClock.fmt_human(mtime))
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
pub enum Sha256Status {
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
    pub sha256_ok: Sha256Status,
    pub size_ok: bool,
    /// 扫描时缓存的 LBA8 原始 512B；用于列表/元信息展示，避免随后再次打开同一备份。
    pub lba8: Option<[u8; SECTOR]>,
    /// 扫描时实际 `.bin` 内容摘要；删除前用于确认同名文件未被替换/改写。
    pub content_sha256: Option<String>,
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

pub fn sha256_sidecar_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.sha256", path.display()))
}

/// 读取 `<备份.bin>.sha256` 的首个摘要 token。
/// 同时兼容本工具的“仅摘要”格式与标准 `sha256sum` 风格的 `HASH  filename`。
pub fn read_backup_sha256(path: &Path) -> io::Result<Option<String>> {
    let sidecar = sha256_sidecar_path(path);
    let metadata = match fs::symlink_metadata(&sidecar) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} 不是普通校验文件", sidecar.display()),
        ));
    }
    let content = fs::read_to_string(&sidecar)?;
    let Some(expected) = content.split_whitespace().next() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} 为空", sidecar.display()),
        ));
    };
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} 不含合法 64 位 SHA-256", sidecar.display()),
        ));
    }
    Ok(Some(expected.to_ascii_lowercase()))
}

fn sha256_status(path: &Path, digest: &str) -> Sha256Status {
    match read_backup_sha256(path) {
        Ok(None) => Sha256Status::NoSidecar,
        Ok(Some(expected)) if expected == digest => Sha256Status::Ok,
        Ok(Some(_)) | Err(_) => Sha256Status::Mismatch,
    }
}

/// 扫描备份目录并给出跨盘管理所需的完整元数据。
///
/// 扫描必须是只读操作：文件名仅用于解析设备信息；只要备份内容可读且包含完整
/// LBA4，onlyid 始终以 LBA4 为权威（即使文件名声称了另一个 onlyid）。旧 `_lid`
/// 与无 onlyid 文件因此都无需改名即可正确归组。未识别 `.bin` 仍保留。
pub fn scan_backup_dir(dir: &Path) -> Vec<BackupEntry> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        if let Some(entry) = scan_backup_file(&item.path()) {
            entries.push(entry);
        }
    }
    entries.sort_by(cmp_backup_newest_first);
    entries
}

/// Load and validate one exact backup without hashing every sibling in its directory.
pub fn scan_backup_file(path: &Path) -> Option<BackupEntry> {
    let file_type = fs::symlink_metadata(path).ok()?.file_type();
    if !file_type.is_file() || path.extension().and_then(|e| e.to_str()) != Some("bin") {
        return None;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let mut meta = parse_backup_name(name);
    let data = fs::read(path).ok();
    let lba8 = data.as_ref().and_then(|d| {
        d.get(8 * SECTOR..9 * SECTOR)
            .and_then(|raw| raw.try_into().ok())
    });
    let content_sha256 = data.as_ref().map(|d| sha256_hex(d));
    if let (Some(m), Some(d)) = (meta.as_mut(), data.as_ref()) {
        if d.len() >= 5 * SECTOR {
            m.onlyid = lba4_label_id_from(&d[4 * SECTOR..5 * SECTOR]);
        }
    }
    let size_ok = data
        .as_ref()
        .map(|d| d.len() == crate::common::METADATA_IMAGE_LEN)
        .unwrap_or(false);
    let sha256_ok = content_sha256
        .as_deref()
        .map(|digest| sha256_status(path, digest))
        .unwrap_or(Sha256Status::Mismatch);
    let is_nopwd = match (&meta, &data) {
        (Some(m), Some(d)) => image_is_nopwd(d, &m.device_id),
        _ => false,
    };
    Some(BackupEntry {
        meta,
        path: path.to_path_buf(),
        mtime: mtime_epoch(path),
        is_nopwd,
        sha256_ok,
        size_ok,
        lba8,
        content_sha256,
    })
}

/// Shell completion 专用的轻量备份索引。
///
/// 这里只判断普通 `.bin` 文件以及文件名是否符合本工具备份命名；绝不读取备份内容、
/// 计算 SHA-256 或解析 LBA。完整健康状态仍由 `scan_backup_dir` 负责。
pub fn scan_backup_names(dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = read_dir
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
                return None;
            }
            let name = path.file_name()?.to_str()?;
            parse_backup_name(name)?;
            Some(path)
        })
        .collect();
    paths.sort_by(
        |a, b| match (backup_name_time_key(a), backup_name_time_key(b)) {
            (Some(at), Some(bt)) => bt.cmp(&at).then_with(|| a.cmp(b)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.cmp(b),
        },
    );
    paths
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
            meta.secs
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into())
        ),
    })
}

/// `backup prune` 的纯策略层：
/// - 加密原盘备份从不成为候选；
/// - 每盘只对已按内容确认的免密快照按备份文件名时间新→旧保留 `keep` 份；
///   仅旧命名无法解析时间时才回退文件系统 mtime；
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
        snaps.sort_by(|a, b| cmp_backup_newest_first(a, b));
        let preserve = if has_original { keep } else { keep.max(1) };
        let mut deletable: Vec<&BackupEntry> = snaps.into_iter().skip(preserve).collect();
        // 候选清单按最旧→较新展示/删除，便于人工核对；保留判定仍严格按最新优先。
        deletable.reverse();
        out.extend(deletable.into_iter().map(|e| e.path.clone()));
    }
    out
}

/// 备份 LBA0-12 到备份目录, 附 .sha256 sidecar。
/// 返回 (备份路径, 是否免密状态快照); `还原:` 提示由 CLI 打印。
pub fn create_backup(
    facts: &DiskFacts,
    data: &[u8],
    device_id: &str,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<(PathBuf, bool)> {
    validate_backup_device_id(device_id)?;
    if data.len() != crate::common::METADATA_IMAGE_LEN {
        return Err(EdpCliError::new(
            EXIT_BACKUP,
            format!(
                "错误: 备份镜像长度 {}B，必须恰好为 {}B（LBA0-12）",
                data.len(),
                crate::common::METADATA_IMAGE_LEN
            ),
        ));
    }
    fs::create_dir_all(bak_dir).map_err(io_err)?;
    let ts = clock.fmt_ts(clock.now_epoch());
    let secs = facts
        .total_sectors
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into());
    let onlyid_part = lba4_label_id_from(&data[4 * SECTOR..5 * SECTOR])
        .as_ref()
        .map(|o| format!("_onlyid{}", o))
        .unwrap_or_default();
    // 免密状态快照打 _nopwd 标: 区别于加密原盘备份, 防止还原时拿错
    let is_nopwd = image_is_nopwd(data, device_id);
    let state_part = if is_nopwd { "_nopwd" } else { "" };
    let base = format!(
        "disk{}_{}_vid{}_pid{}_{}{}{}_{}",
        facts.disk, secs, facts.vid, facts.pid, device_id, onlyid_part, state_part, ts
    );
    let path = bak_dir.join(format!("{}.bin", base));
    write_new_synced(&path, data, "备份文件")?;
    let sha256_path = sha256_sidecar_path(&path);
    let sha256_data = format!("{}\n", sha256_hex(data));
    if let Err(e) = write_new_synced(&sha256_path, sha256_data.as_bytes(), "备份校验文件") {
        let _ = fs::remove_file(&path);
        return Err(e);
    }
    // 两个目录项也持久化后才允许调用方继续进入真实盘写入阶段。
    sync_dir(bak_dir)?;
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
    let Ok(entries) = fs::read_dir(bak_dir) else {
        return vec![];
    };
    let files: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("bin") {
                return None;
            }
            Some((entry.file_name().to_string_lossy().into_owned(), path))
        })
        .collect();
    let secs = facts
        .total_sectors
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into());
    let mut tiers: Vec<Vec<String>> = Vec::new();
    if let Some(did) = device_id {
        tiers.push(vec![
            format!("disk*_{}_vid{}_pid{}_{}_*", secs, facts.vid, facts.pid, did),
            format!(
                "disk*_{}_vid{}_pid{}_{}_*",
                secs,
                facts.vid,
                facts.pid,
                did.replace('&', "_")
            ),
        ]);
    }
    tiers.push(vec![format!(
        "disk*_{}_vid{}_pid{}_*",
        secs, facts.vid, facts.pid
    )]); // 兜底(识别失败时)
    for pats in &tiers {
        let mut out: Vec<PathBuf> = files
            .iter()
            .filter(|(name, _)| pats.iter().any(|pat| wildcard_match(pat, name)))
            .map(|(_, path)| path.clone())
            .collect();
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
        out.sort_by(
            |a, b| match (backup_name_time_key(a), backup_name_time_key(b)) {
                (Some(at), Some(bt)) => bt.cmp(&at).then_with(|| a.cmp(b)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => mtime_epoch(b).cmp(&mtime_epoch(a)).then_with(|| a.cmp(b)),
            },
        );
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

    struct CountingSectorDev {
        reads: usize,
    }

    impl SectorDev for CountingSectorDev {
        fn reopen_rdwr(&mut self, _wait: Duration) -> io::Result<()> {
            Err(io::Error::other("not used"))
        }

        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            self.reads += 1;
            Ok(vec![lba as u8; SECTOR])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Err(io::Error::other("not used"))
        }

        fn sync(&mut self) -> io::Result<()> {
            Err(io::Error::other("not used"))
        }
    }

    #[test]
    fn sector_read_cache_reuses_info_prefetch_during_summary() {
        let mut dev = CountingSectorDev { reads: 0 };
        {
            let mut cache = SectorReadCache::new(&mut dev);
            // physical info 会先为 device_id / onlyid 预读 7、4，随后 summary
            // 请求 0、4、6、7、8、11、12。预读扇区不得再次访问底层。
            assert_eq!(cache.read_sector(7).unwrap(), vec![7u8; SECTOR]);
            assert_eq!(cache.read_sector(4).unwrap(), vec![4u8; SECTOR]);
            for lba in [0, 4, 6, 7, 8, 11, 12] {
                assert_eq!(cache.read_sector(lba).unwrap(), vec![lba as u8; SECTOR]);
            }
        }
        assert_eq!(
            dev.reads, 7,
            "info 预读 + summary 共涉及 7 个唯一 LBA，不应重复访问 LBA4/LBA7"
        );
    }

    #[test]
    fn read_only_disk_pool_opens_each_disk_once() {
        use std::cell::Cell;

        struct DiskDev(u32);
        impl SectorDev for DiskDev {
            fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
                Ok(vec![(self.0 + lba) as u8; SECTOR])
            }

            fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
                Err(io::Error::other("read-only test device"))
            }
        }

        let opens = Cell::new(0usize);
        let mut pool = ReadOnlyDiskPool::new(|disk| {
            opens.set(opens.get() + 1);
            Ok(DiskDev(disk))
        });

        assert_eq!(pool.read_sector(6, 7).unwrap()[0], 13);
        assert_eq!(pool.read_sector(6, 12).unwrap()[0], 18);
        assert_eq!(pool.read_sector(7, 7).unwrap()[0], 14);
        assert_eq!(pool.read_sector(6, 4).unwrap()[0], 10);
        assert_eq!(opens.get(), 2, "同一物理盘在一次 list 会话内只应打开一次");
    }

    #[test]
    fn label_id_positive_negative_malformed() {
        let pos: Vec<u8> = b"$$$1402259934$$$"
            .iter()
            .chain([0u8; 18].iter())
            .copied()
            .collect();
        assert_eq!(lba4_label_id_from(&pos), Some("1402259934".to_string()));
        let neg: Vec<u8> = b"$$$-1833210541$$$"
            .iter()
            .chain([0u8; 15].iter())
            .copied()
            .collect();
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
        assert!(wildcard_match(
            "disk*_122880000_vid0dd8_pid2005_x_*.bin",
            "disk6_122880000_vid0dd8_pid2005_x_onlyid1402259934_20260910_172300.bin"
        ));
        assert!(!wildcard_match("disk*_999_*", "disk6_122880000_x"));
        assert!(wildcard_match("abc", "abc"));
        assert!(!wildcard_match("abc", "abcd"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("a**b", "ab"));
        assert!(!wildcard_match("a*b", "a")); // 尾段放不下
    }

    #[test]
    fn read_lba_file_naming() {
        let d = std::env::temp_dir().join(format!("edpcli_test_{}_lbafile", std::process::id()));
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
            resolve_backup_dir_impl(
                Some("/f"),
                Some("/e".into()),
                Some("/c".into()),
                cwd.clone()
            ),
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
            parse_conf_backup_dir("# 注释\nbackup_dir = /Users/x/.edpcli-backup\n"),
            Some("/Users/x/.edpcli-backup".to_string())
        );
        assert_eq!(
            parse_conf_backup_dir("backup_dir=/a/b"),
            Some("/a/b".to_string())
        );
        assert_eq!(parse_conf_backup_dir("backup_dir =   \n"), None); // 空值
        assert_eq!(parse_conf_backup_dir("other = 1\nnoise\n"), None);
        assert_eq!(parse_conf_backup_dir(""), None);
    }

    #[test]
    fn platform_user_home_is_available_for_current_session() {
        assert!(crate::platform::invoking_user_home().is_some());
    }

    #[test]
    fn backup_dir_argv_suffix_bridges_env() {
        // 提权边界不依赖环境继承：env 值须转为显式旗标（绝对路径）随 argv 过界。
        let absolute_dir = std::env::temp_dir().join("edpcli-absolute-backup");
        let abs = backup_dir_argv_suffix(Some(absolute_dir.to_string_lossy().into_owned()));
        assert_eq!(abs.len(), 2);
        assert_eq!(abs[0], "--backup-dir");
        assert_eq!(PathBuf::from(&abs[1]), absolute_dir);
        // 相对值按 CWD 绝对化
        let rel = backup_dir_argv_suffix(Some("bk".into()));
        assert_eq!(rel.len(), 2);
        let rel_path = PathBuf::from(&rel[1]);
        assert!(rel_path.is_absolute(), "{}", rel[1]);
        assert_eq!(
            rel_path.file_name().and_then(|name| name.to_str()),
            Some("bk")
        );
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
        fn now_epoch(&self) -> i64 {
            1789660800
        }
        fn fmt_ts(&self, epoch: i64) -> String {
            format!("fixed_{}", epoch)
        }
        fn fmt_human(&self, epoch: i64) -> String {
            format!("h_{}", epoch)
        }
    }

    #[test]
    fn clock_fmt_uses_local_time_without_system_date_process() {
        let c = SystemClock;
        let ts = c.fmt_ts(1789660800);
        assert_eq!(ts.len(), 15, "{}", ts); // YYYYmmdd_HHMMSS
        let h = c.fmt_human(1789660800);
        assert_eq!(h.len(), 16, "{}", h); // YYYY-mm-dd HH:MM
        let _ = FixedClock.fmt_ts(1);
    }
}
