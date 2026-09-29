use super::{
    FilesystemError, FilesystemGeometry, FilesystemKind, FilesystemMetadata, FilesystemReader,
    FormatPlan, FormatRequest, FormatVerification,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesystemCapabilities {
    pub detect: bool,
    pub read_metadata: bool,
    pub format: bool,
    pub verify_format: bool,
    pub analyze: bool,
}

impl FilesystemCapabilities {
    pub const fn detect_only() -> Self {
        Self {
            detect: true,
            read_metadata: false,
            format: false,
            verify_format: false,
            analyze: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DetectionConfidence {
    NoMatch,
    Weak,
    Strong,
    Exact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DetectionResult {
    pub kind: FilesystemKind,
    pub confidence: DetectionConfidence,
}

impl DetectionResult {
    pub const fn no_match(kind: FilesystemKind) -> Self {
        Self {
            kind,
            confidence: DetectionConfidence::NoMatch,
        }
    }
}

pub trait FilesystemDriver: Send + Sync {
    fn kind(&self) -> FilesystemKind;

    fn capabilities(&self) -> FilesystemCapabilities;

    fn detect(&self, source: &mut dyn FilesystemReader)
        -> Result<DetectionResult, FilesystemError>;

    fn read_metadata(
        &self,
        _source: &mut dyn FilesystemReader,
    ) -> Result<FilesystemMetadata, FilesystemError> {
        Err(FilesystemError::unsupported(self.kind(), "元数据读取"))
    }

    fn validate_format_request(&self, _request: &FormatRequest) -> Result<(), FilesystemError> {
        Err(FilesystemError::format_unsupported(self.kind()))
    }

    fn build_format_plan(
        &self,
        _geometry: FilesystemGeometry,
        _request: &FormatRequest,
    ) -> Result<FormatPlan, FilesystemError> {
        Err(FilesystemError::format_unsupported(self.kind()))
    }

    fn verify_format(
        &self,
        _source: &mut dyn FilesystemReader,
        _expected: &FilesystemMetadata,
    ) -> Result<FormatVerification, FilesystemError> {
        Err(FilesystemError::format_unsupported(self.kind()))
    }
}
