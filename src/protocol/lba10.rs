use super::{profile::Lba10Eesi, types::*};
use crate::crypto::{a6b0_full, a7f0_full};

#[derive(Clone, Debug)]
pub enum Lba10View {
    Absent {
        wire: WireSector,
    },
    Enabled {
        wire: WireSector,
        plain_prefix: [u8; 0x80],
        device_crc: u32,
        suspension_flag: u32,
        share_label: CStringSlot<16>,
        encrypt_label: CStringSlot<16>,
        extension: Backing<88>,
        tail: Backing<384>,
    },
}

impl Lba10View {
    pub fn reconstruct(&self) -> [u8; 512] {
        match self {
            Self::Absent { wire } | Self::Enabled { wire, .. } => wire.0,
        }
    }

    pub fn reencode(&self) -> [u8; 512] {
        match self {
            Self::Absent { wire } => wire.0,
            Self::Enabled {
                wire,
                plain_prefix,
                device_crc,
                ..
            } => {
                let mut out = wire.0;
                out[..0x80].copy_from_slice(&a7f0_full(plain_prefix, &device_crc.to_le_bytes(), 0));
                out
            }
        }
    }
}

pub fn parse_lba10(raw: &[u8; 512], device_crc: u32, profile: Lba10Eesi) -> Result<Lba10View> {
    match profile {
        Lba10Eesi::Unknown => Err(ProtocolError::UnsupportedProfile { axis: "lba10_eesi" }),
        Lba10Eesi::AbsentZero => {
            if raw.iter().any(|byte| *byte != 0) {
                return Err(ProtocolError::InvalidField {
                    lba: 10,
                    field: "absent_zero",
                });
            }
            Ok(Lba10View::Absent {
                wire: WireSector(*raw),
            })
        }
        Lba10Eesi::EesiEnabled => {
            let plain_prefix: [u8; 0x80] = a6b0_full(&raw[..0x80], &device_crc.to_le_bytes(), 0)
                .try_into()
                .unwrap();
            if &plain_prefix[..4] != b"EESI" {
                return Err(ProtocolError::InvalidField {
                    lba: 10,
                    field: "eesi_magic",
                });
            }
            Ok(Lba10View::Enabled {
                wire: WireSector(*raw),
                device_crc,
                suspension_flag: u32le(&plain_prefix, 0x04),
                share_label: CStringSlot(plain_prefix[0x08..0x18].try_into().unwrap()),
                encrypt_label: CStringSlot(plain_prefix[0x18..0x28].try_into().unwrap()),
                extension: Backing(plain_prefix[0x28..0x80].try_into().unwrap()),
                tail: Backing(raw[0x80..].try_into().unwrap()),
                plain_prefix,
            })
        }
    }
}
