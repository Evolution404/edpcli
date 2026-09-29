use super::FilesystemKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemMetadata {
    pub kind: FilesystemKind,
    pub volume_label: Option<String>,
    pub volume_serial: Option<u32>,
}

impl FilesystemMetadata {
    pub fn new(kind: FilesystemKind) -> Self {
        Self {
            kind,
            volume_label: None,
            volume_serial: None,
        }
    }
}
