use super::types::*;

pub const PASS_INFO_BYTES: usize = 14;

/// First-party EDP logical partition type values.
///
/// These values are consumed by `EdpEDiskCtrl.dll::InitDiskInfo`:
/// 1 -> boot, 2 -> share/exchange, 4 -> encrypted/private.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EdpPartitionType {
    Boot = 1,
    Share = 2,
    Encrypt = 4,
}

impl EdpPartitionType {
    pub const fn from_raw(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Boot),
            2 => Some(Self::Share),
            4 => Some(Self::Encrypt),
            _ => None,
        }
    }

    pub const fn raw(self) -> u32 {
        self as u32
    }

    pub const fn role(self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::Share => "share",
            Self::Encrypt => "encrypt",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PassInfo {
    pub version: u16,
    pub force_change_share: u8,
    pub max_share_password_errors: u8,
    pub current_share_password_errors: u8,
    pub force_change_encrypt: u8,
    pub max_encrypt_password_errors: u8,
    pub current_encrypt_password_errors: u8,
    pub no_password_set: u8,
    pub no_password_no_check_ip: u8,
    pub no_usb_check_password_safe: u8,
    pub reset_file_key: u8,
    pub share_backup_prompt_period: u8,
    pub encrypt_backup_prompt_period: u8,
}

impl PassInfo {
    pub fn decode_stored(stored: &[u8; PASS_INFO_BYTES]) -> Self {
        let mut plain = *stored;
        for offset in [0usize, 3, 6] {
            plain[offset] ^= 0x88;
        }
        Self {
            version: u16::from_le_bytes([plain[0], plain[1]]),
            force_change_share: plain[2],
            max_share_password_errors: plain[3],
            current_share_password_errors: plain[4],
            force_change_encrypt: plain[5],
            max_encrypt_password_errors: plain[6],
            current_encrypt_password_errors: plain[7],
            no_password_set: plain[8],
            no_password_no_check_ip: plain[9],
            no_usb_check_password_safe: plain[10],
            reset_file_key: plain[11],
            share_backup_prompt_period: plain[12],
            encrypt_backup_prompt_period: plain[13],
        }
    }

    pub fn encode_stored(self) -> [u8; PASS_INFO_BYTES] {
        let mut out = [0u8; PASS_INFO_BYTES];
        out[..2].copy_from_slice(&self.version.to_le_bytes());
        out[2] = self.force_change_share;
        out[3] = self.max_share_password_errors;
        out[4] = self.current_share_password_errors;
        out[5] = self.force_change_encrypt;
        out[6] = self.max_encrypt_password_errors;
        out[7] = self.current_encrypt_password_errors;
        out[8] = self.no_password_set;
        out[9] = self.no_password_no_check_ip;
        out[10] = self.no_usb_check_password_safe;
        out[11] = self.reset_file_key;
        out[12] = self.share_backup_prompt_period;
        out[13] = self.encrypt_backup_prompt_period;
        for offset in [0usize, 3, 6] {
            out[offset] ^= 0x88;
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EdpfEntry64 {
    pub version: u32,
    pub partition_count: u32,
    pub partition_type: u32,
    pub need_disturb: u32,
    pub need_encrypt: u32,
    pub start_sector: u64,
    pub sector_size: u64,
    pub partition_size: u64,
    pub user_key_crc: u32,
    pub file_key_crc: u32,
    pub encrypted_file_key: [u8; 8],
}

impl EdpfEntry64 {
    pub fn parse(raw: &[u8; 0x40]) -> Result<Self> {
        if &raw[..4] != b"EDPF" {
            return Err(ProtocolError::InvalidField {
                lba: 7,
                field: "edpf_magic",
            });
        }
        Ok(Self {
            version: u32le(raw, 0x04),
            partition_count: u32le(raw, 0x08),
            partition_type: u32le(raw, 0x0c),
            need_disturb: u32le(raw, 0x10),
            need_encrypt: u32le(raw, 0x14),
            start_sector: u64le(raw, 0x18),
            sector_size: u64le(raw, 0x20),
            partition_size: u64le(raw, 0x28),
            user_key_crc: u32le(raw, 0x30),
            file_key_crc: u32le(raw, 0x34),
            encrypted_file_key: raw[0x38..0x40].try_into().unwrap(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EdpfEntry96 {
    pub version: u32,
    pub partition_count: u32,
    pub partition_type: u32,
    pub need_disturb: u32,
    pub need_encrypt: u32,
    pub start_sector: u64,
    pub sector_size: u64,
    pub partition_size: u64,
    pub user_key_crc: u32,
    pub file_key_crc: u32,
    pub encrypted_file_key: [u8; 16],
    pub compatibility_key: [u8; 16],
    pub encrypt_mode: u8,
    pub reserved: [u8; 7],
}

impl EdpfEntry96 {
    pub fn parse(raw: &[u8; 0x60]) -> Result<Self> {
        if &raw[..4] != b"EDPF" {
            return Err(ProtocolError::InvalidField {
                lba: 12,
                field: "edpf_magic",
            });
        }
        Ok(Self {
            version: u32le(raw, 0x04),
            partition_count: u32le(raw, 0x08),
            partition_type: u32le(raw, 0x0c),
            need_disturb: u32le(raw, 0x10),
            need_encrypt: u32le(raw, 0x14),
            start_sector: u64le(raw, 0x18),
            sector_size: u64le(raw, 0x20),
            partition_size: u64le(raw, 0x28),
            user_key_crc: u32le(raw, 0x30),
            file_key_crc: u32le(raw, 0x34),
            encrypted_file_key: raw[0x38..0x48].try_into().unwrap(),
            compatibility_key: raw[0x48..0x58].try_into().unwrap(),
            encrypt_mode: raw[0x58],
            reserved: raw[0x59..0x60].try_into().unwrap(),
        })
    }
}
