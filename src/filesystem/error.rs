use super::FilesystemKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemErrorKind {
    Unsupported,
    InvalidGeometry,
    InvalidBootSector,
    InvalidMetadata,
    InvalidVolumeLabel,
    ReadFailure,
    FormatUnsupported,
    CorruptFilesystem,
    ScanBudgetExceeded,
    AmbiguousDetection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemError {
    pub kind: FilesystemErrorKind,
    pub filesystem: Option<FilesystemKind>,
    pub message: String,
}

impl FilesystemError {
    pub fn new(kind: FilesystemErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            filesystem: None,
            message: message.into(),
        }
    }

    pub fn for_filesystem(
        filesystem: FilesystemKind,
        kind: FilesystemErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            filesystem: Some(filesystem),
            message: message.into(),
        }
    }

    pub fn unsupported(filesystem: FilesystemKind, operation: &str) -> Self {
        Self::for_filesystem(
            filesystem,
            FilesystemErrorKind::Unsupported,
            format!("{} 不支持 {operation}", filesystem.display_name()),
        )
    }

    pub fn format_unsupported(filesystem: FilesystemKind) -> Self {
        Self::for_filesystem(
            filesystem,
            FilesystemErrorKind::FormatUnsupported,
            format!("当前未实现 {} 格式化", filesystem.display_name()),
        )
    }
}

impl std::fmt::Display for FilesystemError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FilesystemError {}
