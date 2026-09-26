use super::*;

pub fn raw_path(disk: u32) -> String {
    crate::platform::raw_disk_path(disk)
}

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
/// 避免重复访问同一裸盘扇区。restore/provision 的安全复核不得使用该缓存。
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
            .ok_or_else(|| io::Error::other("只读设备缓存缺失"))?
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

    /// inspect 专用的只读 u64 LBA 读取路径。
    ///
    /// 写盘安全链仍使用 SectorDev 的 u32 接口，避免这次只读重构扩大写路径风险。
    /// 偏移计算采用 checked arithmetic，任何溢出直接失败。
    pub fn read_sector_u64(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        let base = lba
            .checked_mul(SECTOR as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "LBA 字节偏移溢出"))?;
        let mut buf = vec![0u8; SECTOR];
        self.file.seek(SeekFrom::Start(base))?;
        self.file.read_exact(&mut buf).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                io::Error::new(error.kind(), format!("LBA{lba} 读取提前 EOF"))
            } else {
                error
            }
        })?;
        Ok(buf)
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
