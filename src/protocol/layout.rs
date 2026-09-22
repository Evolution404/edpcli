//! Runtime projection of the single canonical field catalog, embedded at compile time.
use super::types::{EvolutionKind, Ownership, SemanticType};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct Field {
    pub id: &'static str,
    pub lba: u8,
    pub start: usize,
    pub end: usize,
    pub axis: &'static str,
    pub profile: &'static str,
    pub semantic_type: SemanticType,
    pub ownership: Ownership,
    pub evolution: EvolutionKind,
}
impl Field {
    pub fn length(&self) -> usize {
        self.end - self.start + 1
    }
    pub fn bytes<'a>(&self, sector: &'a [u8; 512]) -> &'a [u8] {
        &sector[self.start..=self.end]
    }
}
pub fn fields() -> &'static [Field] {
    static FIELDS: OnceLock<Vec<Field>> = OnceLock::new();
    FIELDS.get_or_init(|| {
        include_str!("../../audit/protocol/field_catalog.tsv")
            .lines()
            .skip(1)
            .map(|line| {
                let c: Vec<_> = line.split('\t').collect();
                use SemanticType::*;
                Field {
                    id: c[0],
                    lba: c[1].parse().expect("catalog LBA"),
                    start: usize::from_str_radix(c[2], 16).expect("catalog offset"),
                    end: usize::from_str_radix(c[3], 16).expect("catalog offset"),
                    axis: c[5],
                    profile: c[6],
                    semantic_type: match c[7] {
                        "Scalar" => Scalar,
                        "CString" => CString,
                        "PackedStruct" => PackedStruct,
                        "EncryptedRegion" => EncryptedRegion,
                        "Checksum" => Checksum,
                        "ProfileSelector" => ProfileSelector,
                        "OpaquePreserve" => OpaquePreserve,
                        "UnownedBacking" => UnownedBacking,
                        "CompatibilityField" => CompatibilityField,
                        "Snapshot" => Snapshot,
                        _ => panic!("invalid canonical semantic type"),
                    },
                    ownership: match c[11] {
                        "owner" => Ownership::Owner,
                        "overlay" => Ownership::Overlay,
                        _ => panic!("invalid ownership"),
                    },
                    evolution: match c[12] {
                        "Stable" => EvolutionKind::Stable,
                        "AddedRemoved" => EvolutionKind::AddedRemoved,
                        "EncodingChanged" => EvolutionKind::EncodingChanged,
                        "OwnershipChanged" => EvolutionKind::OwnershipChanged,
                        "ProducerChanged" => EvolutionKind::ProducerChanged,
                        _ => panic!("invalid evolution kind"),
                    },
                }
            })
            .collect()
    })
}
pub fn field(id: &str, profile: &str) -> Option<&'static Field> {
    fields().iter().find(|f| f.id == id && f.profile == profile)
}
pub(crate) fn bytes<'a>(raw: &'a [u8; 512], id: &str, profile: &str) -> &'a [u8] {
    field(id, profile)
        .expect("parser field must exist in canonical catalog")
        .bytes(raw)
}
