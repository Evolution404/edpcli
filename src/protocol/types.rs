//! Wire bytes, decoded representations and protocol diagnostics are separate types.
use std::fmt;

pub const SECTOR_BYTES: usize = 512;
pub const IMAGE_BYTES: usize = 13 * SECTOR_BYTES;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidLength { expected: usize, actual: usize },
    InvalidField { lba: u8, field: &'static str },
    UnsupportedProfile { axis: &'static str },
    MissingContext(&'static str),
}
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProtocolError {}
pub type Result<T> = std::result::Result<T, ProtocolError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireSector(pub [u8; SECTOR_BYTES]);
impl TryFrom<&[u8]> for WireSector {
    type Error = ProtocolError;
    fn try_from(bytes: &[u8]) -> Result<Self> {
        Ok(Self(bytes.try_into().map_err(|_| {
            ProtocolError::InvalidLength {
                expected: SECTOR_BYTES,
                actual: bytes.len(),
            }
        })?))
    }
}

/// Exact bytes whose private structure is deliberately outside the EDP contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaquePreserve<const N: usize>(pub(crate) [u8; N]);
impl<const N: usize> OpaquePreserve<N> {
    pub fn bytes(&self) -> &[u8; N] {
        &self.0
    }
}
/// Storage ignored by the relevant consumer. Nonzero contents are not errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backing<const N: usize>(pub(crate) [u8; N]);
impl<const N: usize> Backing<N> {
    pub fn bytes(&self) -> &[u8; N] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticType {
    Scalar,
    CString,
    PackedStruct,
    EncryptedRegion,
    Checksum,
    ProfileSelector,
    OpaquePreserve,
    UnownedBacking,
    CompatibilityField,
    Snapshot,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ownership {
    Owner,
    Overlay,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionKind {
    Stable,
    AddedRemoved,
    EncodingChanged,
    OwnershipChanged,
    ProducerChanged,
}

pub(crate) fn u32le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
