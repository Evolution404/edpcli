use super::types::*;

#[derive(Clone, Debug)]
pub struct GptHeader {
    pub revision: u32,
    pub header_size: u32,
    pub header_crc32: u32,
    pub reserved: u32,
    pub current_lba: u64,
    pub backup_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub disk_guid: [u8; 16],
    pub partition_entries_lba: u64,
    pub entry_count: u32,
    pub entry_size: u32,
    pub partition_array_crc32: u32,
}
impl GptHeader {
    /// Requires the entire declared array. LBA2 alone usually cannot prove this CRC.
    pub fn validate_partition_array(&self, array: &[u8]) -> Result<()> {
        let expected = (self.entry_count as usize)
            .checked_mul(self.entry_size as usize)
            .ok_or(ProtocolError::InvalidField {
                lba: 1,
                field: "entry_array_size",
            })?;
        if array.len() != expected {
            return Err(ProtocolError::InvalidLength {
                expected,
                actual: array.len(),
            });
        }
        if crc32_ieee(array) != self.partition_array_crc32 {
            return Err(ProtocolError::InvalidField {
                lba: 2,
                field: "partition_array_crc32",
            });
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub enum GptHeaderState {
    Absent,
    Enabled(GptHeader),
}
#[derive(Clone, Debug)]
pub struct Lba1View {
    wire: WireSector,
    pub header: GptHeaderState,
}
impl Lba1View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }
}

pub fn parse_lba1(raw: &[u8; 512]) -> Result<Lba1View> {
    if raw.iter().all(|b| *b == 0) {
        return Ok(Lba1View {
            wire: WireSector(*raw),
            header: GptHeaderState::Absent,
        });
    }
    let bad = |field| ProtocolError::InvalidField { lba: 1, field };
    if &raw[..8] != b"EFI PART" {
        return Err(bad("signature"));
    }
    let size = u32le(raw, 12) as usize;
    if !(92..=512).contains(&size) {
        return Err(bad("header_size"));
    }
    let mut crc_bytes = raw[..size].to_vec();
    crc_bytes[16..20].fill(0);
    if crc32_ieee(&crc_bytes) != u32le(raw, 16) {
        return Err(bad("header_crc32"));
    }
    if u32le(raw, 84) != 128 {
        return Err(ProtocolError::UnsupportedProfile {
            axis: "gpt_entry_size",
        });
    }
    let header = GptHeader {
        revision: u32le(raw, 8),
        header_size: size as u32,
        header_crc32: u32le(raw, 16),
        reserved: u32le(raw, 20),
        current_lba: u64le(raw, 24),
        backup_lba: u64le(raw, 32),
        first_usable_lba: u64le(raw, 40),
        last_usable_lba: u64le(raw, 48),
        disk_guid: raw[56..72].try_into().unwrap(),
        partition_entries_lba: u64le(raw, 72),
        entry_count: u32le(raw, 80),
        entry_size: u32le(raw, 84),
        partition_array_crc32: u32le(raw, 88),
    };
    if header.current_lba != 1
        || header.first_usable_lba > header.last_usable_lba
        || header.entry_count == 0
    {
        return Err(bad("header_geometry"));
    }
    Ok(Lba1View {
        wire: WireSector(*raw),
        header: GptHeaderState::Enabled(header),
    })
}
pub(crate) fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for b in data {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
