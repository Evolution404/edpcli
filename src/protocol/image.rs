use super::{
    lba0::{self, Lba0View},
    lba1::{self, GptHeaderState, Lba1View},
    lba10::{self, Lba10View},
    lba11::{self, Lba11View},
    lba12::{self, Lba12View},
    lba2::{self, Lba2View},
    lba3::{self, Lba3View},
    lba4::{self, Lba4Context, Lba4View},
    lba5::{self, Lba5View},
    lba6::{self, Lba6View, MbrUnderlay},
    lba7::{self, Entry2, Lba7View},
    lba8::{self, Lba8Context, Lba8View},
    lba9::{self, EetuState, Lba9View, UpperPayload},
    profile::*,
    types::*,
};
use crate::crypto::crc32_bare;

pub const PROTOCOL_IMAGE_BYTES: usize = 13 * 512;

#[derive(Clone, Debug)]
pub struct ProtocolImageContext<'a> {
    pub profile: ProtocolProfile,
    pub device_id: &'a str,
    pub vid: &'a str,
    pub pid: &'a str,
    pub disk_size_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct ProtocolImageView {
    pub lba0: Lba0View,
    pub lba1: Lba1View,
    pub lba2: Lba2View,
    pub lba3: Lba3View,
    pub lba4: Lba4View,
    pub lba5: Lba5View,
    pub lba6: Lba6View,
    pub lba7: Lba7View,
    pub lba8: Lba8View,
    pub lba9: Lba9View,
    pub lba10: Lba10View,
    pub lba11: Lba11View,
    pub lba12: Lba12View,
    pub dept: Vec<u8>,
    pub user: Vec<u8>,
}

fn require_known_profile(profile: &ProtocolProfile) -> Result<()> {
    let unknown = [
        ("lba0_bootstrap", profile.lba0_bootstrap.as_str()),
        (
            "lba0_sector_size_overlay",
            profile.lba0_sector_size_overlay.as_str(),
        ),
        ("gpt_layout", profile.gpt_layout.as_str()),
        ("lba3_metadata", profile.lba3_metadata.as_str()),
        ("lba4_encoding", profile.lba4_encoding.as_str()),
        ("dept_layout", profile.dept_layout.as_str()),
        ("lba6_mbr_underlay", profile.lba6_mbr_underlay.as_str()),
        ("lba7_entry_count", profile.lba7_entry_count.as_str()),
        (
            "lba7_passinfo_version",
            profile.lba7_passinfo_version.as_str(),
        ),
        ("lba8_usb_only_info", profile.lba8_usb_only_info.as_str()),
        ("lba9_eetu", profile.lba9_eetu.as_str()),
        ("lba9_overlay", profile.lba9_overlay.as_str()),
        ("lba10_eesi", profile.lba10_eesi.as_str()),
        ("lba11_capacity", profile.lba11_capacity.as_str()),
        ("lba12_mode", profile.lba12_mode.as_str()),
        (
            "lba4_second_key_source",
            profile.lba4_second_key_source.as_str(),
        ),
        ("lba4_hserial_source", profile.lba4_hserial_source.as_str()),
        (
            "host_hardinfo_source",
            profile.host_hardinfo_source.as_str(),
        ),
    ];
    if let Some((axis, _)) = unknown.into_iter().find(|(_, state)| *state == "unknown") {
        return Err(ProtocolError::UnsupportedProfile { axis });
    }
    Ok(())
}

fn sector(raw: &[u8; PROTOCOL_IMAGE_BYTES], lba: usize) -> &[u8; 512] {
    raw[lba * 512..(lba + 1) * 512].try_into().unwrap()
}

fn validate_lba9_profile(view: &Lba9View, profile: &ProtocolProfile) -> Result<()> {
    let eetu_ok = matches!(
        (&view.eetu, profile.lba9_eetu),
        (EetuState::Absent, Lba9Eetu::AbsentZero) | (EetuState::Present(_), Lba9Eetu::Eetu)
    );
    if !eetu_ok {
        return Err(ProtocolError::InvalidField {
            lba: 9,
            field: "lba9_eetu_profile",
        });
    }
    let overlay_ok = matches!(
        (&view.upper, profile.lba9_overlay),
        (UpperPayload::Zero, Lba9Overlay::Zero)
            | (UpperPayload::Sapf(_), Lba9Overlay::Sapf)
            | (UpperPayload::LongUser { .. }, Lba9Overlay::LongUser)
            | (UpperPayload::Eppe(_), Lba9Overlay::Eppe)
    );
    if !overlay_ok {
        return Err(ProtocolError::InvalidField {
            lba: 9,
            field: "lba9_overlay_profile",
        });
    }
    Ok(())
}

fn validate_cross_lba(view: &ProtocolImageView, context: &ProtocolImageContext<'_>) -> Result<()> {
    let profile = &context.profile;
    if view.lba0.bootstrap != profile.lba0_bootstrap {
        return Err(ProtocolError::InvalidField {
            lba: 0,
            field: "bootstrap_profile",
        });
    }
    if view.lba0.sector_size != profile.lba0_sector_size_overlay {
        return Err(ProtocolError::InvalidField {
            lba: 0,
            field: "sector_size_profile",
        });
    }
    let gpt_matches = matches!(
        (&view.lba1.header, profile.gpt_layout),
        (GptHeaderState::Absent, GptLayout::Absent)
            | (GptHeaderState::Enabled(_), GptLayout::Enabled)
    );
    if !gpt_matches || view.lba2.profile != profile.gpt_layout {
        return Err(ProtocolError::InvalidField {
            lba: 1,
            field: "gpt_profile",
        });
    }
    if view.lba3.profile != profile.lba3_metadata {
        return Err(ProtocolError::InvalidField {
            lba: 3,
            field: "metadata_profile",
        });
    }
    if view.lba6.dept_profile() != profile.dept_layout {
        return Err(ProtocolError::InvalidField {
            lba: 6,
            field: "dept_profile",
        });
    }
    let mbr_profile_matches = matches!(
        (&view.lba6.mbr_underlay, profile.lba6_mbr_underlay),
        (MbrUnderlay::ZeroBacking, Lba6MbrUnderlay::ZeroUnderlay)
            | (
                MbrUnderlay::LegacySnapshot(_),
                Lba6MbrUnderlay::LegacyMbrSnapshot
            )
    );
    if !mbr_profile_matches {
        return Err(ProtocolError::InvalidField {
            lba: 6,
            field: "mbr_underlay_profile",
        });
    }

    let device_crc = crc32_bare(context.device_id.as_bytes());
    if view.lba6.device_crc != device_crc {
        return Err(ProtocolError::InvalidField {
            lba: 6,
            field: "device_crc_cross_lba",
        });
    }
    if view.lba4.node.host_hardinfo != view.lba8.host_hardinfo {
        return Err(ProtocolError::InvalidField {
            lba: 8,
            field: "host_hardinfo_cross_lba",
        });
    }
    validate_lba9_profile(&view.lba9, profile)?;

    let lba7_count = match view.lba7.entry2 {
        Entry2::Absent => 2usize,
        Entry2::Present(_) => 3usize,
    };
    if view.lba12.entries.len() != lba7_count {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "edpf_count_cross_lba",
        });
    }
    let mut old_types = vec![
        view.lba7.entries_0_1[0].partition_type,
        view.lba7.entries_0_1[1].partition_type,
    ];
    if let Entry2::Present(entry) = view.lba7.entry2 {
        old_types.push(entry.partition_type);
    }
    if old_types
        .iter()
        .zip(&view.lba12.entries)
        .any(|(old, current)| *old != current.partition_type)
    {
        return Err(ProtocolError::InvalidField {
            lba: 12,
            field: "edpf_type_cross_lba",
        });
    }

    if let MbrUnderlay::LegacySnapshot(snapshot) = &view.lba6.mbr_underlay {
        let type4 = view
            .lba12
            .entries
            .iter()
            .find(|entry| entry.partition_type == 4)
            .ok_or(ProtocolError::InvalidField {
                lba: 12,
                field: "missing_type4",
            })?;
        if type4.start_sector != snapshot.start_lba as u64
            || type4.partition_size / 512 != snapshot.sector_count as u64
        {
            return Err(ProtocolError::InvalidField {
                lba: 12,
                field: "legacy_mbr_snapshot_cross_lba",
            });
        }
    }
    if view.lba11.uid.value() != context.device_id.as_bytes() {
        return Err(ProtocolError::InvalidField {
            lba: 11,
            field: "device_id_cross_lba",
        });
    }
    Ok(())
}

pub fn parse_protocol_image(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProtocolImageContext<'_>,
) -> Result<ProtocolImageView> {
    require_known_profile(&context.profile)?;
    let profile = &context.profile;
    let device_crc = crc32_bare(context.device_id.as_bytes());

    let lba0 = lba0::parse_lba0(sector(raw, 0))?;
    let lba1 = lba1::parse_lba1(sector(raw, 1))?;
    let lba2 = lba2::parse_lba2(sector(raw, 2), profile.gpt_layout)?;
    let lba3 = lba3::parse_lba3(sector(raw, 3));
    let lba4 = lba4::parse_lba4(
        sector(raw, 4),
        Lba4Context {
            encoding: profile.lba4_encoding,
            second_key_source: profile.lba4_second_key_source,
            hserial_source: profile.lba4_hserial_source,
            host_hardinfo_source: profile.host_hardinfo_source,
        },
    )?;
    let lba5 = lba5::parse_lba5(sector(raw, 5));
    let lba6 = lba6::parse_lba6(sector(raw, 6))?;
    let lba7 = lba7::parse_lba7(
        sector(raw, 7),
        device_crc,
        profile.lba7_entry_count,
        profile.lba7_passinfo_version,
    )?;
    let lba8 = lba8::parse_lba8(
        sector(raw, 8),
        device_crc,
        Lba8Context {
            usb_only_info: profile.lba8_usb_only_info,
            host_hardinfo_source: profile.host_hardinfo_source,
            main_onlyid: Some(lba4.onlyid),
        },
    )?;
    let lba9 = lba9::parse_lba9(
        sector(raw, 9),
        device_crc,
        profile.dept_layout,
        profile.lba9_overlay == Lba9Overlay::LongUser,
    )?;
    let lba10 = lba10::parse_lba10(sector(raw, 10), device_crc, profile.lba10_eesi)?;
    let lba11 = lba11::parse_lba11(
        sector(raw, 11),
        context.vid,
        context.pid,
        context.disk_size_bytes,
        profile.lba11_capacity,
    )?;
    let lba12 = lba12::parse_lba12(sector(raw, 12), device_crc, profile.lba12_mode)?;
    let dept = lba9::reconstruct_dept(&lba6, &lba9)?;
    let user = lba9::reconstruct_user(&lba6, &lba9)?;
    let view = ProtocolImageView {
        lba0,
        lba1,
        lba2,
        lba3,
        lba4,
        lba5,
        lba6,
        lba7,
        lba8,
        lba9,
        lba10,
        lba11,
        lba12,
        dept,
        user,
    };
    validate_cross_lba(&view, &context)?;
    Ok(view)
}
