use super::*;

pub fn raw_path(disk: u32) -> String {
    crate::platform::raw_disk_path(disk)
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileMediaKind {
    /// Only a confirmed regular file may use bounded contiguous I/O.
    Regular,
    /// Unknown, block, character, raw or other media: never batch.
    RawOrUnknown,
}

impl FileMediaKind {
    fn classify(file: &File, path: &str) -> io::Result<Self> {
        // Require positive proof from the opened handle; names alone cannot
        // establish that a device is a safe disposable ordinary file.
        // Never require filesystem metadata from an already known raw device:
        // some operating systems do not expose it on a raw disk handle.
        if crate::platform::is_raw_device_path(path) {
            return Ok(Self::RawOrUnknown);
        }
        Ok(if file.metadata().is_ok_and(|m| m.file_type().is_file()) {
            Self::Regular
        } else {
            Self::RawOrUnknown
        })
    }
}

pub struct FileDev {
    path: String,
    file: File,
    writable: bool,
    media_kind: FileMediaKind,
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
        let file = File::open(path)?;
        let media_kind = FileMediaKind::classify(&file, path)?;
        Ok(FileDev {
            path: path.to_string(),
            file,
            writable: false,
            media_kind,
        })
    }

    pub fn open_rdwr(path: &str, wait: Duration) -> io::Result<Self> {
        let file = try_open_rdwr(path, wait)?;
        let media_kind = FileMediaKind::classify(&file, path)?;
        Ok(FileDev {
            path: path.to_string(),
            file,
            writable: true,
            media_kind,
        })
    }

    /// Switching to write access must never silently change the opened target.
    /// Validate the newly opened handle *before* replacing the original one.
    pub fn reopen_rdwr(&mut self, wait: Duration) -> io::Result<()> {
        if self.writable {
            return Ok(());
        }
        let reopened = try_open_rdwr(&self.path, wait)?;
        let media_kind = FileMediaKind::classify(&reopened, &self.path)?;
        if media_kind != self.media_kind
            || !crate::platform::same_open_file_identity(&self.file, &reopened)?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "重新打开介质对应不同的文件或设备，拒绝写入",
            ));
        }
        self.file = reopened;
        self.media_kind = media_kind;
        self.writable = true;
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    fn read_exact_sector(&mut self, lba: u64, out: &mut [u8]) -> io::Result<()> {
        let base = lba
            .checked_mul(SECTOR as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "LBA 字节偏移溢出"))?;
        self.file.seek(SeekFrom::Start(base))?;
        self.file.read_exact(out).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                io::Error::new(error.kind(), format!("LBA{lba} 读取提前 EOF"))
            } else {
                error
            }
        })
    }

    /// inspect 专用的只读 u64 LBA 读取路径。
    ///
    /// 写盘安全链仍使用 SectorDev 的 u32 接口，避免这次只读重构扩大写路径风险。
    /// 偏移计算采用 checked arithmetic，任何溢出直接失败。
    pub fn read_sector_u64(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; SECTOR];
        self.read_exact_sector(lba, &mut buf)?;
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
        self.read_exact_sector(u64::from(lba), &mut buf)?;
        Ok(buf)
    }

    fn read_sector_into(&mut self, lba: u32, out: &mut [u8; SECTOR]) -> io::Result<()> {
        self.read_exact_sector(u64::from(lba), out)
    }

    fn max_contiguous_sectors(&self) -> usize {
        // Never enable batched writes for raw USB/disk devices without a
        // separate hardware failure-injection acceptance gate.
        if self.media_kind == FileMediaKind::RawOrUnknown {
            1
        } else {
            128
        }
    }

    fn read_contiguous_sectors_into(
        &mut self,
        first_lba: u32,
        sectors: &mut [[u8; SECTOR]],
    ) -> io::Result<()> {
        if self.media_kind == FileMediaKind::RawOrUnknown {
            for (index, sector) in sectors.iter_mut().enumerate() {
                let lba = first_lba
                    .checked_add(index as u32)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "LBA 溢出"))?;
                self.read_sector_into(lba, sector)?;
            }
            return Ok(());
        }
        self.read_exact_sector(u64::from(first_lba), sectors.as_flattened_mut())
    }

    fn write_contiguous_sectors(
        &mut self,
        first_lba: u32,
        sectors: &[[u8; SECTOR]],
    ) -> io::Result<()> {
        if self.media_kind == FileMediaKind::RawOrUnknown {
            for (index, sector) in sectors.iter().enumerate() {
                let lba = first_lba
                    .checked_add(index as u32)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "LBA 溢出"))?;
                self.write_sector(lba, sector)?;
            }
            return Ok(());
        }
        if !self.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} 以只读打开(未到写阶段)", self.path),
            ));
        }
        self.file
            .seek(SeekFrom::Start(u64::from(first_lba) * SECTOR as u64))?;
        self.file.write_all(sectors.as_flattened())
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
        if self.media_kind == FileMediaKind::RawOrUnknown {
            crate::platform::sync_raw_device(&self.file)
        } else {
            self.file.sync_all()
        }
    }
}

// ══════════════════════════════════════════════════════════════════
// 1. 原子写入(全有或全无)
// ══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod contiguous_batch_tests {
    use super::*;

    #[test]
    fn batch_io_is_opted_in_only_for_regular_files_not_raw_paths() -> io::Result<()> {
        let path = std::env::temp_dir().join(format!(
            "edpcli-file-batch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        File::create(&path)?.set_len(10 * SECTOR as u64)?;
        let path_text = path
            .to_str()
            .ok_or_else(|| io::Error::other("non-UTF8 temporary path"))?;
        let mut dev = FileDev::open_rdwr(path_text, Duration::ZERO)?;
        assert_eq!(SectorDev::max_contiguous_sectors(&dev), 128);
        let two = [[0x15_u8; SECTOR], [0xb2_u8; SECTOR]];
        dev.write_contiguous_sectors(3, &two)?;
        let mut read = [[0_u8; SECTOR]; 2];
        dev.read_contiguous_sectors_into(3, &mut read)?;
        assert_eq!(read, two);
        // Changing the stored text must NOT change the proven kind of the
        // already-opened handle. This guards against path-driven classification.
        dev.path = crate::platform::raw_disk_path(99);
        assert_eq!(SectorDev::max_contiguous_sectors(&dev), 128);
        dev.write_contiguous_sectors(5, &two)?;
        dev.read_contiguous_sectors_into(5, &mut read)?;
        assert_eq!(read, two);
        drop(dev);
        std::fs::remove_file(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod handle_classification_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempFolder(PathBuf);
    impl TempFolder {
        fn new() -> io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "edpcli-s01-handle-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }
    impl Drop for TempFolder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn path_text(path: &std::path::Path) -> io::Result<&str> {
        path.to_str()
            .ok_or_else(|| io::Error::other("non-UTF8 test path"))
    }

    #[test]
    fn reopened_handle_must_refer_to_the_same_file_not_reused_path() -> io::Result<()> {
        let temp = TempFolder::new()?;
        let target = temp.0.join("disk.img");
        let replaced = temp.0.join("other.img");
        File::create(&target)?.set_len(4096)?;
        File::create(&replaced)?.set_len(4096)?;
        let mut dev = FileDev::open_rdonly(path_text(&target)?)?;
        assert_eq!(dev.max_contiguous_sectors(), 128);
        dev.reopen_rdwr(Duration::ZERO)?;
        assert_eq!(dev.max_contiguous_sectors(), 128);
        // A second read-only descriptor must reject a renamed/replaced path.
        let mut dev = FileDev::open_rdonly(path_text(&target)?)?;
        std::fs::rename(&replaced, &target)?;
        let error = dev
            .reopen_rdwr(Duration::ZERO)
            .err()
            .ok_or_else(|| io::Error::other("path replacement unexpectedly accepted"))?;
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!dev.writable);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_classified_by_open_handle_not_by_symlink_name() -> io::Result<()> {
        use std::os::unix::fs::symlink;
        let temp = TempFolder::new()?;
        let regular = temp.0.join("data.img");
        File::create(&regular)?.set_len(4096)?;
        let file_alias = temp.0.join("regular-alias");
        symlink(&regular, &file_alias)?;
        let mut dev = FileDev::open_rdonly(path_text(&file_alias)?)?;
        assert_eq!(dev.max_contiguous_sectors(), 128);
        dev.reopen_rdwr(Duration::ZERO)?;
        assert_eq!(dev.max_contiguous_sectors(), 128);
        let nonregular_alias = temp.0.join("device-alias");
        symlink("/dev/null", &nonregular_alias)?;
        let device = FileDev::open_rdonly(path_text(&nonregular_alias)?)?;
        assert_eq!(device.max_contiguous_sectors(), 1);
        assert_eq!(device.media_kind, FileMediaKind::RawOrUnknown);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn changed_symlink_target_is_rejected_before_any_write() -> io::Result<()> {
        use std::os::unix::fs::symlink;
        let temp = TempFolder::new()?;
        let first = temp.0.join("one.img");
        let second = temp.0.join("two.img");
        File::create(&first)?.set_len(4096)?;
        File::create(&second)?.set_len(4096)?;
        let alias = temp.0.join("disk-alias");
        symlink(&first, &alias)?;
        let mut dev = FileDev::open_rdonly(path_text(&alias)?)?;
        std::fs::remove_file(&alias)?;
        symlink(&second, &alias)?;
        assert_eq!(
            dev.reopen_rdwr(Duration::ZERO).err().map(|err| err.kind()),
            Some(io::ErrorKind::InvalidData)
        );
        assert!(!dev.writable);
        let alias = temp.0.join("disk-device-alias");
        symlink(&first, &alias)?;
        let mut dev = FileDev::open_rdonly(path_text(&alias)?)?;
        std::fs::remove_file(&alias)?;
        symlink("/dev/null", &alias)?;
        assert_eq!(
            dev.reopen_rdwr(Duration::ZERO).err().map(|err| err.kind()),
            Some(io::ErrorKind::InvalidData)
        );
        assert!(!dev.writable);
        Ok(())
    }
}
