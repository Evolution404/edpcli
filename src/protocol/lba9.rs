use super::{
    lba0::MbrPartition,
    lba6::{DeptInline, Lba6View},
    profile::DeptLayout,
    types::*,
};
use crate::crypto::{a6b0_full, a7f0_full};
#[derive(Clone, Debug)]
pub struct Eetu {
    pub begin_time: u64,
    pub end_time: u64,
    pub use_count: u32,
    pub reverse: Backing<102>,
    pub zero_tail: [u8; 2],
}
#[derive(Clone, Debug)]
pub enum EetuState {
    Absent,
    Present(Eetu),
    Unknown(Backing<128>),
}
#[derive(Clone, Debug)]
pub struct Sapf {
    pub partition: MbrPartition,
    pub compatibility: [u8; 12],
    pub tail: Backing<224>,
}
#[derive(Clone, Debug)]
pub struct Eppe {
    pub min_length: u32,
    pub prefix: Backing<128>,
    pub zero_tail: [u8; 120],
}
#[derive(Clone, Debug)]
pub enum UpperPayload {
    Zero,
    Sapf(Sapf),
    LongUser {
        continuation: CStringSlot<128>,
        tail: Backing<128>,
    },
    Eppe(Eppe),
    Unknown(Backing<256>),
}
#[derive(Clone, Debug)]
pub struct Lba9View {
    wire: WireSector,
    plain: [u8; 512],
    crc: u32,
    pub dept_profile: DeptLayout,
    pub dept_continuation: CStringSlot<128>,
    pub eetu: EetuState,
    pub upper: UpperPayload,
}
impl Lba9View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }
    pub fn reencode(&self) -> [u8; 512] {
        let mut out = self.plain;
        let key = self.crc.to_le_bytes();
        if matches!(self.eetu, EetuState::Present(_)) {
            out[..128].copy_from_slice(&a7f0_full(&self.plain[..128], &key, 0));
        }
        if matches!(self.upper, UpperPayload::Sapf(_)) {
            for b in &mut out[256..288] {
                *b ^= 0x88;
            }
        }
        if matches!(self.upper, UpperPayload::Eppe(_)) {
            out[384..].copy_from_slice(&a7f0_full(&self.plain[384..], &key, 0));
        }
        out
    }
}
pub fn parse_lba9(
    raw: &[u8; 512],
    crc: u32,
    dept_profile: DeptLayout,
    long_user: bool,
) -> Result<Lba9View> {
    if dept_profile == DeptLayout::Unknown {
        return Err(ProtocolError::UnsupportedProfile {
            axis: "dept_layout",
        });
    }
    let mut plain = *raw;
    let key = crc.to_le_bytes();
    let eetu_plain = a6b0_full(&raw[..128], &key, 0);
    let eetu = if raw[..128].iter().all(|b| *b == 0) {
        EetuState::Absent
    } else if &eetu_plain[..4] == b"EETU" {
        plain[..128].copy_from_slice(&eetu_plain);
        EetuState::Present(Eetu {
            begin_time: u64le(&plain, 4),
            end_time: u64le(&plain, 12),
            use_count: u32le(&plain, 20),
            reverse: Backing(plain[24..126].try_into().unwrap()),
            zero_tail: plain[126..128].try_into().unwrap(),
        })
    } else {
        EetuState::Unknown(Backing(raw[..128].try_into().unwrap()))
    };
    let sapf: Vec<_> = raw[256..288].iter().map(|b| b ^ 0x88).collect();
    let eppe = a6b0_full(&raw[384..], &key, 0);
    let upper = if long_user {
        UpperPayload::LongUser {
            continuation: CStringSlot(raw[256..384].try_into().unwrap()),
            tail: Backing(raw[384..].try_into().unwrap()),
        }
    } else if &sapf[..4] == b"SAPF" {
        plain[256..288].copy_from_slice(&sapf);
        let p = &sapf[4..20];
        UpperPayload::Sapf(Sapf {
            partition: MbrPartition {
                boot_indicator: p[0],
                start_chs: p[1..4].try_into().unwrap(),
                partition_type: p[4],
                end_chs: p[5..8].try_into().unwrap(),
                start_lba: u32le(p, 8),
                sector_count: u32le(p, 12),
            },
            compatibility: sapf[20..32].try_into().unwrap(),
            tail: Backing(raw[288..].try_into().unwrap()),
        })
    } else if &eppe[..4] == b"EPPE" {
        let min_length = u32le(&eppe, 4);
        if !(6..=19).contains(&min_length) {
            return Err(ProtocolError::InvalidField {
                lba: 9,
                field: "eppe_min_length",
            });
        }
        plain[384..].copy_from_slice(&eppe);
        UpperPayload::Eppe(Eppe {
            min_length,
            prefix: Backing(raw[256..384].try_into().unwrap()),
            zero_tail: eppe[8..].try_into().unwrap(),
        })
    } else if raw[256..].iter().all(|b| *b == 0) {
        UpperPayload::Zero
    } else {
        UpperPayload::Unknown(Backing(raw[256..].try_into().unwrap()))
    };
    Ok(Lba9View {
        wire: WireSector(*raw),
        plain,
        crc,
        dept_profile,
        dept_continuation: CStringSlot(raw[128..256].try_into().unwrap()),
        eetu,
        upper,
    })
}
pub fn reconstruct_dept(six: &Lba6View, nine: &Lba9View) -> Result<Vec<u8>> {
    if six.dept_profile() != nine.dept_profile {
        return Err(ProtocolError::InvalidField {
            lba: 9,
            field: "dept_profile",
        });
    }
    match &six.dept {
        DeptInline::Short(s) => Ok(s.value().to_vec()),
        DeptInline::Join59(_) | DeptInline::Join60(_) => {
            if !nine.dept_continuation.is_terminated() {
                return Err(ProtocolError::InvalidField {
                    lba: 9,
                    field: "dept_terminator",
                });
            }
            let mut out = six.dept_inline_value().to_vec();
            out.extend_from_slice(nine.dept_continuation.value());
            Ok(out)
        }
    }
}
pub fn reconstruct_user(six: &Lba6View, nine: &Lba9View) -> Result<Vec<u8>> {
    if !six.has_long_user() {
        return Ok(six.user.value().to_vec());
    }
    let UpperPayload::LongUser { continuation, .. } = &nine.upper else {
        return Err(ProtocolError::InvalidField {
            lba: 9,
            field: "user_continuation",
        });
    };
    if !continuation.is_terminated() {
        return Err(ProtocolError::InvalidField {
            lba: 9,
            field: "user_terminator",
        });
    }
    let mut out = six.user.bytes()[4..].to_vec();
    out.extend_from_slice(continuation.value());
    Ok(out)
}
