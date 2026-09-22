use super::{
    edpf::{EdpfEntry64, PassInfo},
    profile::{Lba7EntryCount, Lba7PassinfoVersion},
    types::*,
};
use crate::crypto::xor_rolling;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Entry2 {
    Absent,
    Present(EdpfEntry64),
}

#[derive(Clone, Debug)]
pub struct Lba7View {
    wire: WireSector,
    stored_plain: [u8; 512],
    k0: u32,
    pub entries_0_1: [EdpfEntry64; 2],
    pub entry2: Entry2,
    pub pass_info: PassInfo,
    pub post_table_zero: [u8; 306],
}

impl Lba7View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }

    /// Plaintext after the sector rolling-XOR layer. PassInfo bytes still retain
    /// their three byte-level ^0x88 storage transforms.
    pub fn stored_plain(&self) -> &[u8; 512] {
        &self.stored_plain
    }

    pub fn reencode(&self) -> [u8; 512] {
        xor_rolling(&self.stored_plain, self.k0)
            .try_into()
            .expect("LBA7 sector length")
    }
}

pub fn parse_lba7(
    raw: &[u8; 512],
    device_crc: u32,
    entry_count: Lba7EntryCount,
    passinfo_version: Lba7PassinfoVersion,
) -> Result<Lba7View> {
    if entry_count == Lba7EntryCount::Unknown {
        return Err(ProtocolError::UnsupportedProfile {
            axis: "lba7_entry_count",
        });
    }
    if passinfo_version == Lba7PassinfoVersion::Unknown {
        return Err(ProtocolError::UnsupportedProfile {
            axis: "lba7_passinfo_version",
        });
    }
    let k0 = (device_crc & 0xffff) ^ (device_crc >> 16);
    let stored_plain: [u8; 512] = xor_rolling(raw, k0).try_into().unwrap();
    let e0 = EdpfEntry64::parse((&stored_plain[0x00..0x40]).try_into().unwrap())?;
    let e1 = EdpfEntry64::parse((&stored_plain[0x40..0x80]).try_into().unwrap())?;
    let expected_count = match entry_count {
        Lba7EntryCount::TwoEntry => 2,
        Lba7EntryCount::ThreeEntry => 3,
        Lba7EntryCount::Unknown => unreachable!(),
    };
    if e0.partition_count != expected_count || e1.partition_count != expected_count {
        return Err(ProtocolError::InvalidField {
            lba: 7,
            field: "partition_count",
        });
    }
    let entry2 = match entry_count {
        Lba7EntryCount::TwoEntry => {
            if stored_plain[0x80..0xc0].iter().any(|byte| *byte != 0) {
                return Err(ProtocolError::InvalidField {
                    lba: 7,
                    field: "entry2_absent",
                });
            }
            Entry2::Absent
        }
        Lba7EntryCount::ThreeEntry => {
            let entry = EdpfEntry64::parse((&stored_plain[0x80..0xc0]).try_into().unwrap())?;
            if entry.partition_count != 3 {
                return Err(ProtocolError::InvalidField {
                    lba: 7,
                    field: "entry2_partition_count",
                });
            }
            Entry2::Present(entry)
        }
        Lba7EntryCount::Unknown => unreachable!(),
    };
    let stored_pass: [u8; 14] = stored_plain[0xc0..0xce].try_into().unwrap();
    let pass_info = PassInfo::decode_stored(&stored_pass);
    let expected_version = match passinfo_version {
        Lba7PassinfoVersion::LegacyV0064 => 0x0064,
        Lba7PassinfoVersion::CurrentV0206 => 0x0206,
        Lba7PassinfoVersion::Unknown => unreachable!(),
    };
    if pass_info.version != expected_version {
        return Err(ProtocolError::InvalidField {
            lba: 7,
            field: "pass_info_version",
        });
    }
    let post_table_zero: [u8; 306] = stored_plain[0xce..].try_into().unwrap();
    if post_table_zero.iter().any(|byte| *byte != 0) {
        return Err(ProtocolError::InvalidField {
            lba: 7,
            field: "post_table_zero",
        });
    }
    Ok(Lba7View {
        wire: WireSector(*raw),
        stored_plain,
        k0,
        entries_0_1: [e0, e1],
        entry2,
        pass_info,
        post_table_zero,
    })
}
