use super::FilesystemError;

pub trait FilesystemReader {
    fn sector_size(&self) -> u32;
    fn sector_count(&self) -> u64;

    fn read_sector(&mut self, relative_lba: u64) -> Result<[u8; 512], FilesystemError>;
}

/// 仅提供分区相对 LBA0 的只读视图，供 boot-sector 检测使用。
pub struct BootSectorReader<'a> {
    boot: &'a [u8],
    sector_count: u64,
}

impl<'a> BootSectorReader<'a> {
    pub const fn new(boot: &'a [u8], sector_count: u64) -> Self {
        Self { boot, sector_count }
    }
}

impl FilesystemReader for BootSectorReader<'_> {
    fn sector_size(&self) -> u32 {
        512
    }

    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sector(&mut self, relative_lba: u64) -> Result<[u8; 512], FilesystemError> {
        if relative_lba != 0 || self.boot.len() != 512 {
            return Err(FilesystemError::new(
                super::FilesystemErrorKind::ReadFailure,
                "首扇区视图只提供 512B 的分区相对 LBA0",
            ));
        }
        self.boot.try_into().map_err(|_| {
            FilesystemError::new(
                super::FilesystemErrorKind::ReadFailure,
                "首扇区长度不是 512B",
            )
        })
    }
}
