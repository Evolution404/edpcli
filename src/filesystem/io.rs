use super::FilesystemError;

pub trait FilesystemReader {
    fn sector_size(&self) -> u32;
    fn sector_count(&self) -> u64;

    fn read_sector(&mut self, relative_lba: u64) -> Result<[u8; 512], FilesystemError>;
}
