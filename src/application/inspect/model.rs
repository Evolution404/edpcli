use super::*;

pub const MAX_ADVANCED_INSPECT_SECTORS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectDecoderKind {
    Protocol,
    Lce,
    Partition,
}

pub const DECODER_REGISTRY: &[InspectDecoderKind] = &[
    InspectDecoderKind::Protocol,
    InspectDecoderKind::Lce,
    InspectDecoderKind::Partition,
];

impl InspectDecoderKind {
    pub(super) fn matches(
        self,
        context: &crate::inspect_target::InspectDiskContext,
        lba: u64,
    ) -> bool {
        match self {
            Self::Protocol => {
                context.has_edp_protocol() && lba <= u64::from(crate::common::METADATA_LAST_LBA)
            }
            Self::Lce => context.lce.as_ref().is_some_and(|extent| {
                lba >= extent.start_lba
                    && lba < extent.start_lba.saturating_add(extent.sector_count)
            }),
            Self::Partition => context.has_partition_for_lba(lba),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectMode {
    Raw,
    Decode,
    Meta,
}

impl AdvancedInspectMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Decode => "decode",
            Self::Meta => "meta",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Raw => Self::Decode,
            Self::Decode => Self::Meta,
            Self::Meta => Self::Raw,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Raw => Self::Meta,
            Self::Decode => Self::Raw,
            Self::Meta => Self::Decode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectRequest {
    pub mode: AdvancedInspectMode,
    pub lbas: Vec<u64>,
    pub export_dir: Option<std::path::PathBuf>,
    pub device_id_override: Option<String>,
    pub fail_soft_decode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsoluteByteRange {
    pub start: u64,
    pub end_exclusive: u64,
}

impl AbsoluteByteRange {
    pub fn len(self) -> u64 {
        self.end_exclusive - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end_exclusive
    }

    pub fn start_lba(self) -> u64 {
        self.start / SECTOR as u64
    }

    pub fn end_lba(self) -> u64 {
        if self.end_exclusive == self.start {
            return self.start_lba();
        }
        (self.end_exclusive - 1) / SECTOR as u64
    }

    pub fn spans_sectors(self) -> bool {
        self.start_lba() != self.end_lba()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectFieldType {
    Magic,
    Text,
    Identity,
    Address,
    Size,
    Flag,
    Checksum,
}

impl From<crate::inspect_adapter::FieldStyle> for InspectFieldType {
    fn from(style: crate::inspect_adapter::FieldStyle) -> Self {
        match style {
            crate::inspect_adapter::FieldStyle::Magic => Self::Magic,
            crate::inspect_adapter::FieldStyle::Text => Self::Text,
            crate::inspect_adapter::FieldStyle::Identity => Self::Identity,
            crate::inspect_adapter::FieldStyle::Address => Self::Address,
            crate::inspect_adapter::FieldStyle::Size => Self::Size,
            crate::inspect_adapter::FieldStyle::Flag => Self::Flag,
            crate::inspect_adapter::FieldStyle::Checksum => Self::Checksum,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectField {
    pub key: InspectFieldKey,
    pub range: AbsoluteByteRange,
    pub field_type: InspectFieldType,
    /// PhysicalRaw: bytes read from the source sector before sector decoding.
    pub raw: Vec<u8>,
    /// SectorDecoded: bytes after the sector decoder, before field transforms.
    pub decoded: Vec<u8>,
    /// FieldLogical: bytes after a proven field-level storage transform.
    pub field_logical: Option<Vec<u8>>,
    pub transform: Option<FieldTransform>,
    pub status: InspectFieldStatus,
    pub label: String,
    /// SemanticValue: the canonical parser's typed value rendered for humans.
    pub value: String,
    pub style: crate::inspect_adapter::FieldStyle,
    pub group: Option<String>,
    pub children: Vec<crate::inspect_adapter::FieldChild>,
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectItem {
    pub lba: u64,
    pub regions: Vec<String>,
    pub raw: Vec<u8>,
    pub raw_sha256: String,
    pub raw_nonzero: usize,
    pub decoded: Option<Vec<u8>>,
    /// Proven byte ranges that actually passed through a decoder.
    pub decode_ranges: Vec<crate::inspect_adapter::DecodeRange>,
    pub decoded_sha256: Option<String>,
    pub method: Option<String>,
    pub decode_error: Option<String>,
    pub parse_state: InspectParseState,
    pub diagnostics: Vec<InspectDiagnostic>,
    pub fields: Vec<InspectField>,
    pub notes: Vec<String>,
    pub meta_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectWorkspace {
    pub source: String,
    pub meta: InspectMeta,
    pub mode: AdvancedInspectMode,
    pub items: Vec<AdvancedInspectItem>,
    pub export_dir: Option<std::path::PathBuf>,
    pub topology: super::super::inspect_tree::InspectTopology,
    pub disk_layout: Option<super::super::disk_layout::DiskLayoutModel>,
    pub disk_layout_issue: Option<String>,
}
