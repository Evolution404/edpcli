use super::{
    layout::bytes,
    profile::{Lba0Bootstrap, Lba0SectorSizeOverlay},
    types::*,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MbrPartition {
    pub boot_indicator: u8,
    pub start_chs: [u8; 3],
    pub partition_type: u8,
    pub end_chs: [u8; 3],
    pub start_lba: u32,
    pub sector_count: u32,
}
#[derive(Clone, Debug)]
pub struct Lba0View {
    wire: WireSector,
    pub bootstrap: Lba0Bootstrap,
    pub sector_size: Lba0SectorSizeOverlay,
    pub compat_190_19f: Backing<16>,
    pub compat_1a4_1b4: Backing<17>,
    pub message_pointers: [u8; 3],
    pub disk_signature: u32,
    pub mbr_reserved: Backing<2>,
    pub partitions: [MbrPartition; 4],
}
impl Lba0View {
    /// Lossless reconstruction of the parsed capture, not a new-device writer.
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }
}
pub fn parse_lba0(raw: &[u8; 512]) -> Result<Lba0View> {
    if bytes(raw, "lba0.signature_55aa", "all") != [0x55, 0xaa] {
        return Err(ProtocolError::InvalidField {
            lba: 0,
            field: "signature_55aa",
        });
    }
    let prefix = bytes(raw, "lba0.bootstrap", "zero");
    // S-WIN-CURRENT / S-NETAC-MBR exact template fingerprints, not era inference.
    let bootstrap = if prefix.iter().all(|b| *b == 0) {
        Lba0Bootstrap::Zero
    } else {
        match crate::sha256::sha256_hex(prefix).as_str() {
            "4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed" => {
                Lba0Bootstrap::UsbMainBsec
            }
            "00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec" => {
                Lba0Bootstrap::NetacMbr
            }
            _ => Lba0Bootstrap::Unknown,
        }
    };
    let sector_size = match u32le(bytes(raw, "lba0.sector_size_overlay", "absent"), 0) {
        0 => Lba0SectorSizeOverlay::Absent,
        512 => Lba0SectorSizeOverlay::SectorSize512,
        _ => Lba0SectorSizeOverlay::Unknown,
    };
    let table = bytes(raw, "lba0.partition_table", "all");
    Ok(Lba0View {
        wire: WireSector(*raw),
        bootstrap,
        sector_size,
        compat_190_19f: Backing(bytes(raw, "lba0.compat_190_19f", "all").try_into().unwrap()),
        compat_1a4_1b4: Backing(bytes(raw, "lba0.compat_1a4_1b4", "all").try_into().unwrap()),
        message_pointers: bytes(raw, "lba0.legacy_message_ptrs", "all")
            .try_into()
            .unwrap(),
        disk_signature: u32le(bytes(raw, "lba0.mbr_disk_signature", "all"), 0),
        mbr_reserved: Backing(bytes(raw, "lba0.mbr_reserved", "all").try_into().unwrap()),
        partitions: std::array::from_fn(|i| {
            let p = &table[i * 16..i * 16 + 16];
            MbrPartition {
                boot_indicator: p[0],
                start_chs: p[1..4].try_into().unwrap(),
                partition_type: p[4],
                end_chs: p[5..8].try_into().unwrap(),
                start_lba: u32le(p, 8),
                sector_count: u32le(p, 12),
            }
        }),
    })
}
