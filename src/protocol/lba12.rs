use super::{
    edpf::{EdpfEntry96, PassInfo},
    profile::Lba12Mode,
    types::*,
};
use crate::crypto::{a6b0_full, a7f0_full};

#[derive(Clone, Debug)]
pub struct Lba12View {
    wire: WireSector,
    plain: [u8; 512],
    device_crc: u32,
    pub mode: Lba12Mode,
    pub entries: Vec<EdpfEntry96>,
    pub pass_info: PassInfo,
    pub zero_padding: [u8; 210],
}

impl Lba12View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }
    pub fn decoded(&self) -> &[u8; 512] {
        &self.plain
    }
    pub fn reencode(&self) -> [u8; 512] {
        a7f0_full(&self.plain, &self.device_crc.to_le_bytes(), 0)
            .try_into()
            .expect("LBA12 sector length")
    }
}

pub fn parse_lba12(raw: &[u8; 512], device_crc: u32, mode: Lba12Mode) -> Result<Lba12View> {
    if mode == Lba12Mode::Unknown {
        return Err(ProtocolError::UnsupportedProfile { axis: "lba12_mode" });
    }
    let plain: [u8; 512] = a6b0_full(raw, &device_crc.to_le_bytes(), 0)
        .try_into()
        .unwrap();
    if &plain[..4] != b"EDPF" {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "edpf_magic",
        });
    }
    let count = u32le(&plain, 0x08) as usize;
    if !(1..=3).contains(&count) {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "partition_count",
        });
    }
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let base = index * 0x60;
        let entry = EdpfEntry96::parse((&plain[base..base + 0x60]).try_into().unwrap())?;
        if entry.partition_count as usize != count {
            return Err(ProtocolError::InvalidField {
                lba: 12,
                field: "entry_partition_count",
            });
        }
        entries.push(entry);
    }
    if plain[count * 0x60..0x120].iter().any(|byte| *byte != 0) {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "unused_entries",
        });
    }

    let stored_pass: [u8; 14] = plain[0x120..0x12e].try_into().unwrap();
    let pass_info = PassInfo::decode_stored(&stored_pass);
    if !matches!(pass_info.version, 0x0064 | 0x0206) {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "pass_info_version",
        });
    }
    // lba12_mode is the wrapped-file-key algorithm profile, not the outer
    // sector cipher and not the independent PassInfo.Version value.
    let expected_mode = match mode {
        Lba12Mode::LegacyV0064 => 0,
        Lba12Mode::Mode1 => 1,
        Lba12Mode::Mode2 => 2,
        Lba12Mode::Mode3 => 3,
        Lba12Mode::Unknown => unreachable!(),
    };
    if entries
        .iter()
        .filter(|entry| entry.need_encrypt != 0)
        .any(|entry| entry.encrypt_mode != expected_mode)
    {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "encrypt_mode",
        });
    }

    let zero_padding: [u8; 210] = plain[0x12e..].try_into().unwrap();
    if zero_padding.iter().any(|byte| *byte != 0) {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "zero_padding",
        });
    }
    Ok(Lba12View {
        wire: WireSector(*raw),
        plain,
        device_crc,
        mode,
        entries,
        pass_info,
        zero_padding,
    })
}
