//! Stable, UI-neutral Inspect field identity and evidence semantics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldTransform {
    XorByte { offset: usize, mask: u8 },
}

impl FieldTransform {
    pub fn apply(self, bytes: &[u8]) -> Option<Vec<u8>> {
        let mut logical = bytes.to_vec();
        match self {
            Self::XorByte { offset, mask } => *logical.get_mut(offset)? ^= mask,
        }
        Some(logical)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorFieldStatus {
    Known,
    Unknown,
    Reserved,
    Preserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectParseState {
    Parsed,
    Ambiguous,
    MissingContext,
    Unsupported,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectFieldKey {
    Lba8UsbOnlyInfo,
    Lba8HostHardinfo,
    Lba8Elabel,
    Lba8ToolVersion,
    Lba8LabVersion,
    Lba8WriteTime,
    Lba8LogicalLength,
    Lba12MaxSharePasswordErrors,
    Lba12MaxEncryptPasswordErrors,
    ProtocolOffset {
        lba: u32,
        start: u16,
        end: u16,
        ordinal: u16,
    },
    Synthetic,
}

impl InspectFieldKey {
    pub fn protocol(lba: u64, start: usize, end: usize, ordinal: usize) -> Self {
        match (lba, start, end) {
            (8, 0x014, 0x018) => Self::Lba8HostHardinfo,
            (8, 0x01e, 0x02e) => Self::Lba8UsbOnlyInfo,
            (8, 0x080, _) => Self::Lba8Elabel,
            (8, 0x008, 0x00c) => Self::Lba8ToolVersion,
            (8, 0x00c, 0x010) => Self::Lba8LabVersion,
            (8, 0x010, 0x014) => Self::Lba8WriteTime,
            (8, 0x004, 0x008) => Self::Lba8LogicalLength,
            (12, 0x123, 0x124) => Self::Lba12MaxSharePasswordErrors,
            (12, 0x126, 0x127) => Self::Lba12MaxEncryptPasswordErrors,
            _ => Self::ProtocolOffset {
                lba: u32::try_from(lba).expect("protocol LBA is u32"),
                start: u16::try_from(start).expect("sector field offset is u16"),
                end: u16::try_from(end).expect("sector field offset is u16"),
                ordinal: u16::try_from(ordinal).expect("sector field count is u16"),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectDiagnosticCode {
    MissingDeviceId,
    MissingGeometry,
    MissingProtocolContext,
    AmbiguousProfile,
    CanonicalParserRejected,
    UnsupportedSector,
    ShortSector,
}

impl InspectDiagnosticCode {
    pub const fn parse_state(self) -> InspectParseState {
        match self {
            Self::MissingDeviceId | Self::MissingGeometry | Self::MissingProtocolContext => {
                InspectParseState::MissingContext
            }
            Self::AmbiguousProfile => InspectParseState::Ambiguous,
            Self::CanonicalParserRejected | Self::ShortSector => InspectParseState::Invalid,
            Self::UnsupportedSector => InspectParseState::Unsupported,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectDiagnostic {
    pub code: InspectDiagnosticCode,
    pub message: String,
    pub field_key: Option<InspectFieldKey>,
    pub relative_range: Option<(usize, usize)>,
}

impl InspectDiagnostic {
    pub fn new(code: InspectDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            field_key: None,
            relative_range: None,
        }
    }
}
