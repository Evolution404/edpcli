use super::{profile::*, types::*};
use crate::crypto::{a6b0_full, a7f0_full};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lba8Context {
    pub usb_only_info: Lba8UsbOnlyInfo,
    pub host_hardinfo_source: HostHardinfoSource,
    pub main_onlyid: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UsbOnlyInfo {
    Current([u8; 16]),
    Transitional([u8; 16]),
    StrictLegacyAbsent,
}

#[derive(Clone, Debug)]
pub struct Lba8View {
    wire: WireSector,
    mixed_plain: [u8; 512],
    device_crc: u32,
    encrypted_len: usize,
    pub context: Lba8Context,
    pub logical_length: u32,
    pub tool_version: [u8; 4],
    pub lab_version: u32,
    pub write_time: u32,
    pub host_hardinfo: u32,
    pub mac_info: [u8; 6],
    pub usb_only_info: UsbOnlyInfo,
    pub usb_only_suffix: [u8; 16],
    pub elab_offset: u16,
    pub reserved_header: [u8; 64],
    pub elabel_body: Vec<u8>,
    pub encrypted_backing: Vec<u8>,
    pub tail_backing: Vec<u8>,
}

impl Lba8View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }

    /// Prefix bytes are decrypted while bytes after encrypted_len stay in their
    /// physical/raw representation. This mirrors the official partial-sector writer.
    pub fn mixed_plain(&self) -> &[u8; 512] {
        &self.mixed_plain
    }

    pub fn encrypted_len(&self) -> usize {
        self.encrypted_len
    }

    pub fn reencode(&self) -> [u8; 512] {
        let mut out = self.wire.0;
        out[..self.encrypted_len].copy_from_slice(&a7f0_full(
            &self.mixed_plain[..self.encrypted_len],
            &self.device_crc.to_le_bytes(),
            0,
        ));
        out
    }
}

fn expected_usb_text(main_onlyid: u32, second: u32) -> [u8; 16] {
    let text = format!("{main_onlyid:08x}{second:08x}");
    text.as_bytes()
        .try_into()
        .expect("16-byte UsbOnlyInfo text")
}

pub fn parse_lba8(raw: &[u8; 512], device_crc: u32, context: Lba8Context) -> Result<Lba8View> {
    if context.usb_only_info == Lba8UsbOnlyInfo::Unknown {
        return Err(ProtocolError::UnsupportedProfile {
            axis: "lba8_usb_only_info",
        });
    }
    if context.host_hardinfo_source == HostHardinfoSource::Unknown {
        return Err(ProtocolError::UnsupportedProfile {
            axis: "host_hardinfo_source",
        });
    }

    let first = a6b0_full(&raw[..16], &device_crc.to_le_bytes(), 0);
    if first.get(..4) != Some(b"LLGB") {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "magic",
        });
    }
    let logical_length = u32le(&first, 4);
    let logical = logical_length as usize;
    if !(0x80..512).contains(&logical) {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "logical_length",
        });
    }
    let encrypted_len = logical
        .checked_div(16)
        .and_then(|blocks| blocks.checked_add(1))
        .and_then(|blocks| blocks.checked_mul(16))
        .filter(|len| *len <= 512)
        .ok_or(ProtocolError::InvalidField {
            lba: 8,
            field: "encrypted_length",
        })?;

    let mut mixed_plain = *raw;
    mixed_plain[..encrypted_len].copy_from_slice(&a6b0_full(
        &raw[..encrypted_len],
        &device_crc.to_le_bytes(),
        0,
    ));
    if &mixed_plain[..4] != b"LLGB" || mixed_plain[logical] != 0 {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "elabel_terminator",
        });
    }
    let tool_version: [u8; 4] = mixed_plain[0x08..0x0c].try_into().unwrap();
    if tool_version != [1, 0, 0, 1] {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "tool_version",
        });
    }
    let lab_version = u32le(&mixed_plain, 0x0c);
    if lab_version != 0x222 {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "lab_version",
        });
    }
    let host_hardinfo = u32le(&mixed_plain, 0x14);
    if context.host_hardinfo_source == HostHardinfoSource::CurrentZero && host_hardinfo != 0 {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "host_hardinfo_source",
        });
    }
    let mac_info: [u8; 6] = mixed_plain[0x18..0x1e].try_into().unwrap();
    if mac_info != [0; 6] {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "mac_info",
        });
    }
    let usb_raw: [u8; 16] = mixed_plain[0x1e..0x2e].try_into().unwrap();
    let usb_only_info = match context.usb_only_info {
        Lba8UsbOnlyInfo::Current => {
            let onlyid = context.main_onlyid.ok_or(ProtocolError::InvalidField {
                lba: 8,
                field: "main_onlyid",
            })?;
            if usb_raw != expected_usb_text(onlyid, 0) {
                return Err(ProtocolError::InvalidField {
                    lba: 8,
                    field: "usb_only_info_current",
                });
            }
            UsbOnlyInfo::Current(usb_raw)
        }
        Lba8UsbOnlyInfo::Transitional2019 => {
            let onlyid = context.main_onlyid.ok_or(ProtocolError::InvalidField {
                lba: 8,
                field: "main_onlyid",
            })?;
            if usb_raw != expected_usb_text(onlyid, host_hardinfo) {
                return Err(ProtocolError::InvalidField {
                    lba: 8,
                    field: "usb_only_info_transitional",
                });
            }
            UsbOnlyInfo::Transitional(usb_raw)
        }
        Lba8UsbOnlyInfo::StrictLegacyAbsent => {
            if usb_raw != [0; 16] {
                return Err(ProtocolError::InvalidField {
                    lba: 8,
                    field: "usb_only_info_legacy",
                });
            }
            UsbOnlyInfo::StrictLegacyAbsent
        }
        Lba8UsbOnlyInfo::Unknown => unreachable!(),
    };
    let usb_only_suffix: [u8; 16] = mixed_plain[0x2e..0x3e].try_into().unwrap();
    if usb_only_suffix != [0; 16] {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "usb_only_suffix",
        });
    }
    let elab_offset = u16::from_le_bytes(mixed_plain[0x3e..0x40].try_into().unwrap());
    if elab_offset != 0x80 {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "elab_offset",
        });
    }
    let reserved_header: [u8; 64] = mixed_plain[0x40..0x80].try_into().unwrap();
    if reserved_header != [0; 64] {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "reserved_header",
        });
    }
    let elabel_body = mixed_plain[0x80..logical].to_vec();
    if !elabel_body.starts_with(b"<ELABEL>") {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "elabel_body",
        });
    }
    Ok(Lba8View {
        wire: WireSector(*raw),
        mixed_plain,
        device_crc,
        encrypted_len,
        context,
        logical_length,
        tool_version,
        lab_version,
        write_time: u32le(&mixed_plain, 0x10),
        host_hardinfo,
        mac_info,
        usb_only_info,
        usb_only_suffix,
        elab_offset,
        reserved_header,
        elabel_body,
        encrypted_backing: mixed_plain[logical + 1..encrypted_len].to_vec(),
        tail_backing: raw[encrypted_len..].to_vec(),
    })
}
