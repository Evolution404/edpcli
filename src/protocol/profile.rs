//! Orthogonal axes. Unknown never implies another known state or writer lineage.
macro_rules! axis {
    ($name:ident { $($variant:ident => $state:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub enum $name { $($variant,)+ #[default] Unknown }
        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $state,)+ Self::Unknown => "unknown" }
            }
            pub fn from_state(state: &str) -> Option<Self> {
                match state { $($state => Some(Self::$variant),)+ "unknown" => Some(Self::Unknown), _ => None }
            }
        }
    };
}
axis! { Lba0Bootstrap { Zero => "zero", UsbMainBsec => "usb-main-bsec", NetacMbr => "netac-mbr" } }
axis! { Lba0SectorSizeOverlay { Absent => "absent", SectorSize512 => "sector-size-512" } }
axis! { GptLayout { Absent => "absent", Enabled => "enabled" } }
axis! { Lba3Metadata { Zero => "zero", KingstonMpA => "kingston-mp-a", HistoricalMpB => "historical-mp-b" } }
axis! { Lba4Encoding { PostXor => "post-xor", OrdinaryRolling => "ordinary-rolling" } }
axis! { DeptLayout { Short => "short", Join59 => "join59", Join60 => "join60" } }
axis! { Lba6MbrUnderlay { ZeroUnderlay => "zero-underlay", LegacyMbrSnapshot => "legacy-mbr-snapshot" } }
axis! { Lba7EntryCount { TwoEntry => "two-entry", ThreeEntry => "three-entry" } }
axis! { Lba7PassinfoVersion { LegacyV0064 => "legacy-v0064", CurrentV0206 => "current-v0206" } }
axis! { Lba8UsbOnlyInfo { Current => "current", Transitional2019 => "transitional-2019", StrictLegacyAbsent => "strict-legacy-absent" } }
axis! { Lba9Eetu { AbsentZero => "absent-zero", Eetu => "eetu" } }
axis! { Lba9Overlay { Zero => "zero", Sapf => "sapf", LongUser => "long-user", Eppe => "eppe" } }
axis! { Lba10Eesi { AbsentZero => "absent-zero", EesiEnabled => "eesi-enabled" } }
axis! { Lba11Capacity { DiskSize => "disk-size", RepairChs => "repair-chs" } }
axis! { Lba12Mode { LegacyV0064 => "legacy-v0064", Mode1 => "mode1", Mode2 => "mode2", Mode3 => "mode3" } }
axis! { Lba4SecondKeySource { CurrentMainOnlyid => "current-main-onlyid", LegacyGuidCrc => "legacy-guid-crc" } }
axis! { Lba4HserialSource { CurrentZero => "current-zero", LegacyCallerVector => "legacy-caller-vector" } }
axis! { HostHardinfoSource { CurrentZero => "current-zero", LegacyHostIdentity => "legacy-host-identity" } }

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProtocolProfile {
    pub lba0_bootstrap: Lba0Bootstrap,
    pub lba0_sector_size_overlay: Lba0SectorSizeOverlay,
    pub gpt_layout: GptLayout,
    pub lba3_metadata: Lba3Metadata,
    pub lba4_encoding: Lba4Encoding,
    pub dept_layout: DeptLayout,
    pub lba6_mbr_underlay: Lba6MbrUnderlay,
    pub lba7_entry_count: Lba7EntryCount,
    pub lba7_passinfo_version: Lba7PassinfoVersion,
    pub lba8_usb_only_info: Lba8UsbOnlyInfo,
    pub lba9_eetu: Lba9Eetu,
    pub lba9_overlay: Lba9Overlay,
    pub lba10_eesi: Lba10Eesi,
    pub lba11_capacity: Lba11Capacity,
    pub lba12_mode: Lba12Mode,
    pub lba4_second_key_source: Lba4SecondKeySource,
    pub lba4_hserial_source: Lba4HserialSource,
    pub host_hardinfo_source: HostHardinfoSource,
}
