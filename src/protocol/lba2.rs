use super::{profile::GptLayout, types::*};
#[derive(Clone, Debug)]
pub struct GptPartition {
    pub type_guid: [u8; 16],
    pub unique_guid: [u8; 16],
    pub first_lba: u64,
    pub last_lba: u64,
    pub attributes: u64,
    pub name_utf16: [u16; 36],
}
#[derive(Clone, Debug)]
pub enum GptEntry {
    Unused { residual: Backing<112> },
    Used(GptPartition),
}
#[derive(Clone, Debug)]
pub struct Lba2View {
    wire: WireSector,
    pub profile: GptLayout,
    pub entries: Option<[GptEntry; 4]>,
}
impl Lba2View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }
}
pub fn parse_lba2(raw: &[u8; 512], profile: GptLayout) -> Result<Lba2View> {
    let entries = match profile {
        GptLayout::Absent => {
            if raw.iter().any(|b| *b != 0) {
                return Err(ProtocolError::InvalidField {
                    lba: 2,
                    field: "absent_gpt",
                });
            }
            None
        }
        GptLayout::Enabled => Some(std::array::from_fn(|i| {
            let p = &raw[i * 128..(i + 1) * 128];
            if p[..16].iter().all(|b| *b == 0) {
                GptEntry::Unused {
                    residual: Backing(p[16..].try_into().unwrap()),
                }
            } else {
                GptEntry::Used(GptPartition {
                    type_guid: p[..16].try_into().unwrap(),
                    unique_guid: p[16..32].try_into().unwrap(),
                    first_lba: u64le(p, 32),
                    last_lba: u64le(p, 40),
                    attributes: u64le(p, 48),
                    name_utf16: std::array::from_fn(|j| {
                        u16::from_le_bytes(p[56 + j * 2..58 + j * 2].try_into().unwrap())
                    }),
                })
            }
        })),
        GptLayout::Unknown => return Err(ProtocolError::UnsupportedProfile { axis: "gpt_layout" }),
    };
    Ok(Lba2View {
        wire: WireSector(*raw),
        profile,
        entries,
    })
}
