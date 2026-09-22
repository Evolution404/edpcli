use super::{layout, profile::DeptLayout, types::*};
use crate::crypto::{lba6_checksum, lba6_decode, xor_rolling, LBA6_K0};

#[derive(Clone, Debug)]
pub enum DeptInline {
    Short(CStringSlot<64>),
    Join59([u8; 59]),
    Join60([u8; 60]),
}
#[derive(Clone, Debug)]
pub struct MbrSnapshot {
    pub prefix: [u8; 6],
    pub start_lba: u32,
    pub sector_count: u32,
}
#[derive(Clone, Debug)]
pub enum MbrUnderlay {
    ZeroBacking,
    LegacySnapshot(MbrSnapshot),
    Unknown(OpaquePreserve<14>),
}
#[derive(Clone, Debug)]
pub struct Lba6View {
    checksum_doubled: bool,
    wire: WireSector,
    plain: [u8; 512],
    pub dept: DeptInline,
    pub template_040_04f: [u8; 16],
    pub user: CStringSlot<32>,
    pub autonum: CStringSlot<16>,
    pub office: CStringSlot<64>,
    pub template_0c0_0ff: [u8; 64],
    pub device_crc: u32,
    pub device_crc_guard: u32,
    pub template_108_187: [u8; 128],
    pub label: CStringSlot<56>,
    pub gserial: CStringSlot<16>,
    pub beizhu: CStringSlot<16>,
    pub mbr_underlay: MbrUnderlay,
    pub compatibility_tail: [u8; 2],
    pub encrypt_generation_flag: u32,
    pub zero_tail: [u8; 8],
    pub checksum: u32,
}
impl Lba6View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }
    pub fn decoded(&self) -> &[u8; 512] {
        &self.plain
    }
    /// Replays the decoded capture through rolling encryption and recomputes the checksum.
    pub fn reencode(&self) -> [u8; 512] {
        let mut out = encode_safe6(&self.plain);
        if self.checksum_doubled {
            out[508..].copy_from_slice(&self.checksum.to_le_bytes());
        }
        out
    }
    pub fn dept_profile(&self) -> DeptLayout {
        match self.dept {
            DeptInline::Short(_) => DeptLayout::Short,
            DeptInline::Join59(_) => DeptLayout::Join59,
            DeptInline::Join60(_) => DeptLayout::Join60,
        }
    }
    pub fn dept_inline_value(&self) -> &[u8] {
        match &self.dept {
            DeptInline::Short(s) => s.value(),
            DeptInline::Join59(p) => p,
            DeptInline::Join60(p) => p,
        }
    }
    pub fn has_long_user(&self) -> bool {
        self.user.bytes()[..4] == *b"*^$@"
    }
}
pub fn parse_lba6(raw: &[u8; 512]) -> Result<Lba6View> {
    let bad = |field| ProtocolError::InvalidField { lba: 6, field };
    let checksum = u32le(raw, 508);
    let calculated = lba6_checksum(&raw[..508]);
    if checksum != calculated && checksum != calculated.wrapping_mul(2) {
        return Err(bad("checksum"));
    }
    let plain: [u8; 512] = lba6_decode(raw).try_into().unwrap();
    let f = |id| layout::bytes(&plain, id, "all");
    let crc = u32le(f("lba6.device_crc"), 0);
    let guard = u32le(f("lba6.device_crc_guard"), 0);

    let dept = if &plain[..4] == b"*^$@" {
        if plain[63] == 0 {
            DeptInline::Join59(plain[4..63].try_into().unwrap())
        } else {
            DeptInline::Join60(plain[4..64].try_into().unwrap())
        }
    } else {
        DeptInline::Short(CStringSlot(plain[..64].try_into().unwrap()))
    };
    let underlay = &plain[0x1e0..0x1ee];
    let mbr_underlay = if underlay.iter().all(|b| *b == 0) {
        MbrUnderlay::ZeroBacking
    } else if underlay[..6] == [0xc1, 0xff, 7, 0xef, 0xff, 0xff] {
        MbrUnderlay::LegacySnapshot(MbrSnapshot {
            prefix: underlay[..6].try_into().unwrap(),
            start_lba: u32le(underlay, 6),
            sector_count: u32le(underlay, 10),
        })
    } else {
        MbrUnderlay::Unknown(OpaquePreserve(underlay.try_into().unwrap()))
    };
    Ok(Lba6View {
        checksum_doubled: checksum != calculated,
        wire: WireSector(*raw),
        plain,
        dept,
        template_040_04f: f("lba6.template_040_04f").try_into().unwrap(),
        user: CStringSlot(f("lba6.user_slot").try_into().unwrap()),
        autonum: CStringSlot(f("lba6.autonum_slot").try_into().unwrap()),
        office: CStringSlot(f("lba6.office_slot").try_into().unwrap()),
        template_0c0_0ff: f("lba6.template_0c0_0ff").try_into().unwrap(),
        device_crc: crc,
        device_crc_guard: guard,
        template_108_187: f("lba6.template_108_187").try_into().unwrap(),
        label: CStringSlot(f("lba6.label_slot").try_into().unwrap()),
        gserial: CStringSlot(plain[0x1c0..0x1d0].try_into().unwrap()),
        beizhu: CStringSlot(plain[0x1d0..0x1e0].try_into().unwrap()),
        mbr_underlay,
        compatibility_tail: f("lba6.compat_1ee_1ef").try_into().unwrap(),
        encrypt_generation_flag: u32le(f("lba6.encrypt_generation_flag"), 0),
        zero_tail: f("lba6.zero_tail").try_into().unwrap(),
        checksum,
    })
}
pub fn encode_safe6(plain: &[u8; 512]) -> [u8; 512] {
    let cipher = xor_rolling(&plain[..508], LBA6_K0);
    let mut out = [0; 512];
    out[..508].copy_from_slice(&cipher);
    out[508..].copy_from_slice(&lba6_checksum(&cipher).to_le_bytes());
    out
}
