use super::{
    edpf::{EdpPartitionType, EdpfEntry64, PassInfo},
    profile::{Lba7EntryCount, Lba7PassinfoVersion},
    types::*,
};
use crate::crypto::xor_rolling;

/// Partition-mode selector emitted by the first-party label tool.
///
/// The mapping is recovered from the original `cemssafeudisklabeltool.exe`
/// radio buttons through `usbtoolbusmanage.dll` into
/// `cemsusbregsiter.dll::CreatePartitions`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Lba7PartitionMode {
    /// 官方界面“缺省三分区”.
    DefaultThreePartition = 0,
    /// 官方界面“启动区和交换区二合一”.
    BootShareCombined = 1,
    /// 官方界面“整盘加密”.
    WholeDiskEncrypted = 2,
    /// 官方界面“内外网通用双分区”.
    IntranetExtranetDualPartition = 3,
}

impl Lba7PartitionMode {
    pub const fn ui_name_zh(self) -> &'static str {
        match self {
            Self::DefaultThreePartition => "缺省三分区",
            Self::BootShareCombined => "启动区和交换区二合一",
            Self::WholeDiskEncrypted => "整盘加密",
            Self::IntranetExtranetDualPartition => "内外网通用双分区",
        }
    }

    pub const fn partition_types(self) -> &'static [EdpPartitionType] {
        match self {
            Self::DefaultThreePartition => &[
                EdpPartitionType::Boot,
                EdpPartitionType::Share,
                EdpPartitionType::Encrypt,
            ],
            Self::BootShareCombined => &[EdpPartitionType::Share, EdpPartitionType::Encrypt],
            Self::WholeDiskEncrypted => &[EdpPartitionType::Boot, EdpPartitionType::Encrypt],
            Self::IntranetExtranetDualPartition => {
                &[EdpPartitionType::Boot, EdpPartitionType::Share]
            }
        }
    }

    pub fn from_partition_types(types: &[u32]) -> Option<Self> {
        [
            Self::DefaultThreePartition,
            Self::BootShareCombined,
            Self::WholeDiskEncrypted,
            Self::IntranetExtranetDualPartition,
        ]
        .into_iter()
        .find(|mode| {
            mode.partition_types()
                .iter()
                .map(|partition_type| partition_type.raw())
                .eq(types.iter().copied())
        })
    }
}

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

    pub fn partition_types(&self) -> Vec<u32> {
        let mut out = vec![
            self.entries_0_1[0].partition_type,
            self.entries_0_1[1].partition_type,
        ];
        if let Entry2::Present(entry) = self.entry2 {
            out.push(entry.partition_type);
        }
        out
    }

    /// Returns a first-party label-tool mode only when the LBA7 type sequence
    /// exactly matches one of the four producer cases. Unknown historical
    /// sequences remain unclassified instead of being guessed.
    pub fn official_partition_mode(&self) -> Option<Lba7PartitionMode> {
        Lba7PartitionMode::from_partition_types(&self.partition_types())
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
