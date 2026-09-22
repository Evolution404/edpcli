//! Region A / IIR protocol structures.
//!
//! Region A is the six-sector (0xC00-byte) block pointed to by LBA7 entry1/entry2.
//! The first 0x800 bytes are the encrypted SectorManageImp::ReadIIR/WriteIIR table.
//! This module intentionally parses only already-decrypted IIR plaintext; the
//! real-device AES key source is not yet closed and therefore no decrypt function
//! is exposed here.

pub const REGION_A_TOTAL_SIZE: usize = 0xC00;
pub const REGION_A_CHS_TAIL_DISTANCE_BYTES: u64 = 0xE0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionALayout {
    pub chs_bytes: u64,
    pub start_byte_offset: u64,
    pub start_lba: u64,
    pub size_bytes: u64,
    pub size_sectors: u64,
}

/// Reproduces the Region A locator used by cemsusbregsiter.dll.
///
/// The official registration path obtains a classic DISK_GEOMETRY, computes
/// Cylinders * TracksPerCylinder * SectorsPerTrack * BytesPerSector, subtracts
/// 0xE0000 bytes, then divides by the sector size for the EDPF StartSector.
/// The 0xC00-byte Region A extent is rounded up to a whole sector.
pub fn locate_region_a_from_geometry(
    cylinders: u64,
    tracks_per_cylinder: u32,
    sectors_per_track: u32,
    bytes_per_sector: u32,
) -> Option<RegionALayout> {
    let sector_size = u64::from(bytes_per_sector);
    if sector_size == 0 {
        return None;
    }

    let chs_bytes = cylinders
        .checked_mul(u64::from(tracks_per_cylinder))?
        .checked_mul(u64::from(sectors_per_track))?
        .checked_mul(sector_size)?;
    let start_byte_offset = chs_bytes.checked_sub(REGION_A_CHS_TAIL_DISTANCE_BYTES)?;
    let size_bytes = (REGION_A_TOTAL_SIZE as u64)
        .checked_add(sector_size - 1)?
        .checked_div(sector_size)?
        .checked_mul(sector_size)?;

    Some(RegionALayout {
        chs_bytes,
        start_byte_offset,
        start_lba: start_byte_offset / sector_size,
        size_bytes,
        size_sectors: size_bytes / sector_size,
    })
}

pub const IIR_MAIN_SIZE: usize = 0x800;
pub const IIR_UNKNOWN_TAIL_OFFSET: usize = 0x800;
pub const IIR_UNKNOWN_TAIL_SIZE: usize = 0x400;
pub const IIR_MAIN_CRC_OFFSET: usize = 0x3FE;
pub const IIR_MAIN_CRC_BODY_LEN: usize = 0x3E4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IirCrcSegment {
    pub crc_offset: usize,
    pub body_offset: usize,
    pub body_len: usize,
    pub name: &'static str,
}

pub const IIR_CRC_SEGMENTS: [IirCrcSegment; 19] = [
    IirCrcSegment {
        crc_offset: 0x400,
        body_offset: 0x020,
        body_len: 0x04,
        name: "log_partition_size_be_sectors",
    },
    IirCrcSegment {
        crc_offset: 0x402,
        body_offset: 0x024,
        body_len: 0x04,
        name: "public_partition_size_be_sectors",
    },
    IirCrcSegment {
        crc_offset: 0x404,
        body_offset: 0x028,
        body_len: 0x04,
        name: "share_partition_size_be_sectors",
    },
    IirCrcSegment {
        crc_offset: 0x406,
        body_offset: 0x02C,
        body_len: 0x04,
        name: "private_partition_size_be_sectors",
    },
    IirCrcSegment {
        crc_offset: 0x408,
        body_offset: 0x054,
        body_len: 0x20,
        name: "device_info_slot_a",
    },
    IirCrcSegment {
        crc_offset: 0x40A,
        body_offset: 0x074,
        body_len: 0x20,
        name: "device_info_slot_b",
    },
    IirCrcSegment {
        crc_offset: 0x40C,
        body_offset: 0x094,
        body_len: 0x40,
        name: "key_config_slot_0x094",
    },
    IirCrcSegment {
        crc_offset: 0x40E,
        body_offset: 0x0D4,
        body_len: 0x40,
        name: "key_config_slot_0x0d4",
    },
    IirCrcSegment {
        crc_offset: 0x410,
        body_offset: 0x114,
        body_len: 0x40,
        name: "data_key_slot",
    },
    IirCrcSegment {
        crc_offset: 0x412,
        body_offset: 0x154,
        body_len: 0x40,
        name: "key_config_slot_0x154",
    },
    IirCrcSegment {
        crc_offset: 0x414,
        body_offset: 0x194,
        body_len: 0x40,
        name: "password_digest_slot_a",
    },
    IirCrcSegment {
        crc_offset: 0x416,
        body_offset: 0x1D4,
        body_len: 0x40,
        name: "password_digest_slot_b",
    },
    IirCrcSegment {
        crc_offset: 0x418,
        body_offset: 0x214,
        body_len: 0x04,
        name: "retry_counter_block",
    },
    IirCrcSegment {
        crc_offset: 0x41A,
        body_offset: 0x21C,
        body_len: 0x40,
        name: "device_flag_slot",
    },
    IirCrcSegment {
        crc_offset: 0x41C,
        body_offset: 0x25C,
        body_len: 0x40,
        name: "extended_slot_0x25c",
    },
    IirCrcSegment {
        crc_offset: 0x41E,
        body_offset: 0x29C,
        body_len: 0x40,
        name: "flag_info_head",
    },
    IirCrcSegment {
        crc_offset: 0x420,
        body_offset: 0x2DC,
        body_len: 0xFF,
        name: "variable_info_area",
    },
    IirCrcSegment {
        crc_offset: 0x422,
        body_offset: 0x3DC,
        body_len: 0x04,
        name: "node_status_0x34_mirror",
    },
    IirCrcSegment {
        crc_offset: 0x424,
        body_offset: 0x3E0,
        body_len: 0x04,
        name: "node_status_0x30_mirror",
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IirCrcCheck {
    pub crc_offset: usize,
    pub body_offset: usize,
    pub body_len: usize,
    pub stored: u16,
    pub calculated_inverted: u16,
    pub ok: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IirPartitionSizes {
    pub log_be_sectors: u32,
    pub public_be_sectors: u32,
    pub share_be_sectors: u32,
    pub private_be_sectors: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IirPlainAnalysis {
    pub partition_sizes: IirPartitionSizes,
    pub main_crc: IirCrcCheck,
    pub segment_crcs: Vec<(&'static str, IirCrcCheck)>,
}

impl IirPlainAnalysis {
    pub fn all_crc_ok(&self) -> bool {
        self.main_crc.ok && self.segment_crcs.iter().all(|(_, check)| check.ok)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionAError {
    WrongIirPlainLength { expected: usize, actual: usize },
}

impl std::fmt::Display for RegionAError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongIirPlainLength { expected, actual } => {
                write!(f, "IIR plaintext must be {expected} bytes, got {actual}")
            }
        }
    }
}

impl std::error::Error for RegionAError {}

/// Reflected CRC16 used by the official IIR implementation:
/// initial value 0xFFFF, polynomial 0xA001.
pub fn crc16_reflected(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

pub fn stored_iir_crc(data: &[u8]) -> u16 {
    !crc16_reflected(data)
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}

fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn check_crc(data: &[u8], crc_offset: usize, body_offset: usize, body_len: usize) -> IirCrcCheck {
    let calculated_inverted = stored_iir_crc(&data[body_offset..body_offset + body_len]);
    let stored = read_u16_le(data, crc_offset);
    IirCrcCheck {
        crc_offset,
        body_offset,
        body_len,
        stored,
        calculated_inverted,
        ok: stored == calculated_inverted,
    }
}

pub fn analyze_iir_plain(data: &[u8]) -> Result<IirPlainAnalysis, RegionAError> {
    if data.len() != IIR_MAIN_SIZE {
        return Err(RegionAError::WrongIirPlainLength {
            expected: IIR_MAIN_SIZE,
            actual: data.len(),
        });
    }

    let partition_sizes = IirPartitionSizes {
        log_be_sectors: read_u32_be(data, 0x020),
        public_be_sectors: read_u32_be(data, 0x024),
        share_be_sectors: read_u32_be(data, 0x028),
        private_be_sectors: read_u32_be(data, 0x02C),
    };
    let main_crc = check_crc(data, IIR_MAIN_CRC_OFFSET, 0, IIR_MAIN_CRC_BODY_LEN);
    let segment_crcs = IIR_CRC_SEGMENTS
        .iter()
        .map(|segment| {
            (
                segment.name,
                check_crc(
                    data,
                    segment.crc_offset,
                    segment.body_offset,
                    segment.body_len,
                ),
            )
        })
        .collect();

    Ok(IirPlainAnalysis {
        partition_sizes,
        main_crc,
        segment_crcs,
    })
}
