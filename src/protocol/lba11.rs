use super::{profile::Lba11Capacity, types::*};
use crate::crypto::{a6b0_full, a7f0_full, crc32_bare};

const CHS_UNIT: u64 = 255 * 63 * 512;

#[derive(Clone, Debug)]
pub struct Lba11View {
    wire: WireSector,
    plain_pdkb: [u8; 256],
    crypto_key: u32,
    pub profile: Lba11Capacity,
    pub capacity_bytes: u64,
    pub random252: [u8; 252],
    pub uid: CStringSlot<252>,
}

impl Lba11View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }

    pub fn decoded_pdkb(&self) -> &[u8; 256] {
        &self.plain_pdkb
    }

    pub fn reencode(&self) -> [u8; 512] {
        let mut out = self.wire.0;
        out[0x100..].copy_from_slice(&a7f0_full(
            &self.plain_pdkb,
            &self.crypto_key.to_le_bytes(),
            0,
        ));
        out
    }
}

fn ascii4(value: &str, field: &'static str) -> Result<[u8; 4]> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProtocolError::InvalidField { lba: 11, field });
    }
    Ok(value.as_bytes().try_into().unwrap())
}

fn selected_capacity(disk_size_bytes: u64, profile: Lba11Capacity) -> Result<u64> {
    match profile {
        Lba11Capacity::DiskSize => Ok(disk_size_bytes),
        Lba11Capacity::RepairChs => Ok(disk_size_bytes / CHS_UNIT * CHS_UNIT),
        Lba11Capacity::Unknown => Err(ProtocolError::UnsupportedProfile {
            axis: "lba11_capacity",
        }),
    }
}

pub fn parse_lba11(
    raw: &[u8; 512],
    vid: &str,
    pid: &str,
    disk_size_bytes: u64,
    profile: Lba11Capacity,
) -> Result<Lba11View> {
    if &raw[..4] != b"DRKB" {
        return Err(ProtocolError::InvalidField {
            lba: 11,
            field: "drkb_magic",
        });
    }
    let vid = ascii4(vid, "vid_ascii4")?;
    let pid = ascii4(pid, "pid_ascii4")?;
    let capacity_bytes = selected_capacity(disk_size_bytes, profile)?;

    let mut key_input = Vec::with_capacity(0x110);
    key_input.extend_from_slice(&raw[..0x100]);
    key_input.extend_from_slice(&vid);
    key_input.extend_from_slice(&pid);
    key_input.extend_from_slice(&capacity_bytes.to_le_bytes());
    let crypto_key = crc32_bare(&key_input);
    let plain_pdkb: [u8; 256] = a6b0_full(&raw[0x100..], &crypto_key.to_le_bytes(), 0)
        .try_into()
        .unwrap();
    if &plain_pdkb[..4] != b"PDKB" {
        return Err(ProtocolError::InvalidField {
            lba: 11,
            field: "pdkb_magic",
        });
    }
    let uid = CStringSlot(plain_pdkb[4..].try_into().unwrap());
    if !uid.is_terminated() || uid.backing().iter().any(|byte| *byte != 0) {
        return Err(ProtocolError::InvalidField {
            lba: 11,
            field: "uid_zero_fill",
        });
    }
    Ok(Lba11View {
        wire: WireSector(*raw),
        plain_pdkb,
        crypto_key,
        profile,
        capacity_bytes,
        random252: raw[4..0x100].try_into().unwrap(),
        uid,
    })
}
