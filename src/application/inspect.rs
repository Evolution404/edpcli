//! Read-only inspect application service shared by CLI/TUI frontends.

use std::path::Path;

pub use super::evidence::SectorReader;
use super::evidence::{EvidenceError, EvidenceSource};
use crate::common::{METADATA_SECTOR_COUNT, SECTOR};
use crate::inspect_adapter::{self as inspect, InspectMeta};
use crate::ports::CmdRunner;

mod decode;
mod export;
mod model;
mod service;
mod source;

pub use super::inspect_text::render_fields_plain;
/// Isolated native 4Kn AES_CROSS read-only decoder; no device or write handle.
pub use crate::partition_transform::{
    decrypt_mode3_native, transform_native_sector_offline, NativeCipherDirection,
    NativePartitionDataCipher,
};
pub use decode::{decode_sector, sector_meta_text};
use decode::{materialize_protocol_fields, materialize_protocol_fields_with_sector_bytes};
use export::{export_advanced_bytes, export_advanced_meta};
pub use model::*;
pub use service::{load_backup_advanced_inspect, load_disk_advanced_inspect};
#[cfg(test)]
use source::run_advanced_source;
use source::run_evidence_source;
#[cfg(test)]
mod advanced_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectErrorKind {
    InvalidRequest,
    OutOfRange,
    Backup,
    Io,
    Decode,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectError {
    kind: InspectErrorKind,
    message: String,
}

impl InspectError {
    pub fn new(kind: InspectErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> InspectErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::InvalidRequest, message)
    }

    fn out_of_range(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::OutOfRange, message)
    }

    fn io(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::Io, message)
    }

    fn decode(message: impl Into<String>) -> Self {
        Self::new(InspectErrorKind::Decode, message)
    }
}

impl std::fmt::Display for InspectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for InspectError {}

impl From<EvidenceError> for InspectError {
    fn from(error: EvidenceError) -> Self {
        let kind = match &error {
            EvidenceError::BackupVerify { .. }
            | EvidenceError::BackupProtocolRead { .. }
            | EvidenceError::BackupProtocolLength { .. }
            | EvidenceError::BackupMissingGeometry { .. } => InspectErrorKind::Backup,
            EvidenceError::Target(_) | EvidenceError::DiskMissingGeometry { .. } => {
                InspectErrorKind::Target
            }
            EvidenceError::DiskOpen { .. } | EvidenceError::DiskProtocolRead { .. } => {
                InspectErrorKind::Io
            }
        };
        Self::new(kind, error.to_string())
    }
}

pub use crate::inspect_adapter::{
    FieldTransform, InspectDiagnostic, InspectDiagnosticCode, InspectFieldKey, InspectParseState,
    SectorFieldStatus as InspectFieldStatus,
};
