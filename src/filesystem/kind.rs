#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FilesystemKind {
    Fat12,
    Fat16,
    Fat32,
    ExFat,
    Ntfs,
}

impl FilesystemKind {
    pub const fn config_token(self) -> &'static str {
        match self {
            Self::Fat12 => "fat12",
            Self::Fat16 => "fat16",
            Self::Fat32 => "fat32",
            Self::ExFat => "exfat",
            Self::Ntfs => "ntfs",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Fat12 => "FAT12",
            Self::Fat16 => "FAT16",
            Self::Fat32 => "FAT32",
            Self::ExFat => "exFAT",
            Self::Ntfs => "NTFS",
        }
    }
}
