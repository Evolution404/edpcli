use super::{profile::Lba3Metadata, types::*};
#[derive(Clone, Debug)]
pub struct Lba3View {
    pub profile: Lba3Metadata,
    pub payload: OpaquePreserve<512>,
}
impl Lba3View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.payload.0
    }
}
pub fn parse_lba3(raw: &[u8; 512]) -> Lba3View {
    // Exact observed captures only. No claim about private manufacturer subfields.
    let profile = if raw.iter().all(|b| *b == 0) {
        Lba3Metadata::Zero
    } else {
        match crate::sha256::sha256_hex(raw).as_str() {
            "a71848a2f81c2e78de38feda31e9a3717ada257b3dd4d2981a9af6da50e741f9" => {
                Lba3Metadata::KingstonMpA
            }
            "a1e1961d4ab452b6a2f277ee2027c962ea8bed58c6b85f05da12b247a706580e" => {
                Lba3Metadata::HistoricalMpB
            }
            _ => Lba3Metadata::Unknown,
        }
    };
    Lba3View {
        profile,
        payload: OpaquePreserve(*raw),
    }
}
