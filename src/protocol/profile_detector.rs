//! Unified registry for the 18 orthogonal protocol profile axes.
//!
//! Detection is fail-closed: a detector returns a unique state only when the
//! current wire bytes and required external context prove it. Producer
//! provenance axes stay candidate-only because the current evidence explicitly
//! forbids inferring writer lineage from byte shape alone.

use super::{
    image::PROTOCOL_IMAGE_BYTES, lba0, lba1, lba10, lba11, lba12, lba2, lba3, lba4, lba6, lba7,
    lba8, lba9, profile::*,
};
use crate::protocol::crypto::crc32_bare;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileDetectorKind {
    Wire,
    Contextual,
    ProvenanceOnly,
}

impl ProfileDetectorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wire => "wire",
            Self::Contextual => "contextual",
            Self::ProvenanceOnly => "provenance-only",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileDetection {
    Unique(&'static str),
    Candidates(Vec<&'static str>),
    MissingContext(Vec<&'static str>),
    Unknown,
}

impl ProfileDetection {
    fn from_candidates(mut states: Vec<&'static str>) -> Self {
        states.sort_unstable();
        states.dedup();
        match states.len() {
            0 => Self::Unknown,
            1 => Self::Unique(states[0]),
            _ => Self::Candidates(states),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileDetectionContext<'a> {
    pub device_id: Option<&'a str>,
    pub vid: Option<&'a str>,
    pub pid: Option<&'a str>,
    pub disk_size_bytes: Option<u64>,
    pub onlyid: Option<u32>,
}

impl ProfileDetectionContext<'_> {
    fn device_crc(self) -> Option<u32> {
        self.device_id.map(|value| crc32_bare(value.as_bytes()))
    }
}

pub type ProfileDetectorFn =
    fn(&[u8; PROTOCOL_IMAGE_BYTES], ProfileDetectionContext<'_>) -> ProfileDetection;

#[derive(Clone, Copy)]
pub struct ProfileAxisDetector {
    pub axis: &'static str,
    pub states: &'static [&'static str],
    pub kind: ProfileDetectorKind,
    pub symbol: &'static str,
    pub detect: ProfileDetectorFn,
}

impl std::fmt::Debug for ProfileAxisDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileAxisDetector")
            .field("axis", &self.axis)
            .field("states", &self.states)
            .field("kind", &self.kind)
            .field("symbol", &self.symbol)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileAxisDetection {
    pub axis: &'static str,
    pub detection: ProfileDetection,
}

fn sector(raw: &[u8; PROTOCOL_IMAGE_BYTES], lba: usize) -> &[u8; 512] {
    raw[lba * 512..(lba + 1) * 512].try_into().unwrap()
}

fn enum_state(state: &'static str) -> ProfileDetection {
    if state == "unknown" {
        ProfileDetection::Unknown
    } else {
        ProfileDetection::Unique(state)
    }
}

fn provenance_lba4(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    states: &'static [&'static str],
) -> ProfileDetection {
    let context = lba4::Lba4Context {
        encoding: Lba4Encoding::Unknown,
        second_key_source: Lba4SecondKeySource::Unknown,
        hserial_source: Lba4HserialSource::Unknown,
        host_hardinfo_source: HostHardinfoSource::Unknown,
    };
    if lba4::parse_lba4(sector(raw, 4), context).is_ok() {
        ProfileDetection::Candidates(states.to_vec())
    } else {
        ProfileDetection::Unknown
    }
}

pub fn detect_lba0_bootstrap(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    lba0::parse_lba0(sector(raw, 0))
        .map(|view| enum_state(view.bootstrap.as_str()))
        .unwrap_or(ProfileDetection::Unknown)
}

pub fn detect_lba0_sector_size_overlay(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    lba0::parse_lba0(sector(raw, 0))
        .map(|view| enum_state(view.sector_size.as_str()))
        .unwrap_or(ProfileDetection::Unknown)
}

pub fn detect_gpt_layout(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    let Ok(view) = lba1::parse_lba1(sector(raw, 1)) else {
        return ProfileDetection::Unknown;
    };
    let state = match view.header {
        lba1::GptHeaderState::Absent => GptLayout::Absent,
        lba1::GptHeaderState::Enabled(_) => GptLayout::Enabled,
    };
    if lba2::parse_lba2(sector(raw, 2), state).is_err() {
        return ProfileDetection::Unknown;
    }
    enum_state(state.as_str())
}

pub fn detect_lba3_metadata(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    enum_state(lba3::parse_lba3(sector(raw, 3)).profile.as_str())
}

pub fn detect_lba4_encoding(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    provenance_lba4(raw, &["post-xor", "ordinary-rolling"])
}

pub fn detect_dept_layout(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    lba6::parse_lba6(sector(raw, 6))
        .map(|view| enum_state(view.dept_profile().as_str()))
        .unwrap_or(ProfileDetection::Unknown)
}

pub fn detect_lba6_mbr_underlay(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    let Ok(view) = lba6::parse_lba6(sector(raw, 6)) else {
        return ProfileDetection::Unknown;
    };
    match view.mbr_underlay {
        lba6::MbrUnderlay::ZeroBacking => ProfileDetection::Unique("zero-underlay"),
        lba6::MbrUnderlay::LegacySnapshot(_) => ProfileDetection::Unique("legacy-mbr-snapshot"),
        lba6::MbrUnderlay::Unknown(_) => ProfileDetection::Unknown,
    }
}

pub(crate) fn lba7_profile_candidates(
    raw: &[u8; 512],
    device_crc: u32,
) -> Vec<(Lba7EntryCount, Lba7PassinfoVersion)> {
    let mut matches = Vec::new();
    for entry_count in [Lba7EntryCount::TwoEntry, Lba7EntryCount::ThreeEntry] {
        for passinfo in [
            Lba7PassinfoVersion::LegacyV0064,
            Lba7PassinfoVersion::CurrentV0206,
        ] {
            if lba7::parse_lba7(raw, device_crc, entry_count, passinfo).is_ok() {
                matches.push((entry_count, passinfo));
            }
        }
    }
    matches
}

fn lba7_candidates(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> Result<Vec<(Lba7EntryCount, Lba7PassinfoVersion)>, ProfileDetection> {
    let Some(crc) = context.device_crc() else {
        return Err(ProfileDetection::MissingContext(vec!["device_id"]));
    };
    Ok(lba7_profile_candidates(sector(raw, 7), crc))
}

pub fn detect_lba7_entry_count(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    match lba7_candidates(raw, context) {
        Ok(matches) => ProfileDetection::from_candidates(
            matches
                .into_iter()
                .map(|(state, _)| state.as_str())
                .collect(),
        ),
        Err(result) => result,
    }
}

pub fn detect_lba7_passinfo_version(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    match lba7_candidates(raw, context) {
        Ok(matches) => ProfileDetection::from_candidates(
            matches
                .into_iter()
                .map(|(_, state)| state.as_str())
                .collect(),
        ),
        Err(result) => result,
    }
}

pub(crate) fn lba8_profile_candidates(
    raw: &[u8; 512],
    device_crc: u32,
    onlyid: Option<u32>,
) -> Vec<(Lba8UsbOnlyInfo, HostHardinfoSource)> {
    let mut matches = Vec::new();
    for usb_only_info in [
        Lba8UsbOnlyInfo::Current,
        Lba8UsbOnlyInfo::Transitional2019,
        Lba8UsbOnlyInfo::StrictLegacyAbsent,
    ] {
        for host_hardinfo_source in [
            HostHardinfoSource::CurrentZero,
            HostHardinfoSource::LegacyHostIdentity,
        ] {
            let parse_context = lba8::Lba8Context {
                usb_only_info,
                host_hardinfo_source,
                main_onlyid: onlyid,
            };
            if lba8::parse_lba8(raw, device_crc, parse_context).is_ok() {
                matches.push((usb_only_info, host_hardinfo_source));
            }
        }
    }
    matches
}

fn lba8_candidates(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> Result<Vec<(Lba8UsbOnlyInfo, HostHardinfoSource)>, ProfileDetection> {
    let Some(crc) = context.device_crc() else {
        return Err(ProfileDetection::MissingContext(vec!["device_id"]));
    };
    let onlyid = context.onlyid.or_else(|| {
        let parse_context = lba4::Lba4Context {
            encoding: Lba4Encoding::Unknown,
            second_key_source: Lba4SecondKeySource::Unknown,
            hserial_source: Lba4HserialSource::Unknown,
            host_hardinfo_source: HostHardinfoSource::Unknown,
        };
        lba4::parse_lba4(sector(raw, 4), parse_context)
            .ok()
            .map(|view| view.onlyid)
    });
    Ok(lba8_profile_candidates(sector(raw, 8), crc, onlyid))
}

pub fn detect_lba8_usb_only_info(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    match lba8_candidates(raw, context) {
        Ok(matches) => ProfileDetection::from_candidates(
            matches
                .into_iter()
                .map(|(state, _)| state.as_str())
                .collect(),
        ),
        Err(result) => result,
    }
}

fn lba9_view(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> Result<lba9::Lba9View, ProfileDetection> {
    let Some(crc) = context.device_crc() else {
        return Err(ProfileDetection::MissingContext(vec!["device_id"]));
    };
    let six = lba6::parse_lba6(sector(raw, 6)).map_err(|_| ProfileDetection::Unknown)?;
    lba9::parse_lba9(sector(raw, 9), crc, six.dept_profile(), six.has_long_user())
        .map_err(|_| ProfileDetection::Unknown)
}

pub fn detect_lba9_eetu(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    match lba9_view(raw, context) {
        Ok(view) => match view.eetu {
            lba9::EetuState::Absent => ProfileDetection::Unique("absent-zero"),
            lba9::EetuState::Present(_) => ProfileDetection::Unique("eetu"),
            lba9::EetuState::Unknown(_) => ProfileDetection::Unknown,
        },
        Err(result) => result,
    }
}

pub fn detect_lba9_overlay(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    match lba9_view(raw, context) {
        Ok(view) => match view.upper {
            lba9::UpperPayload::Zero => ProfileDetection::Unique("zero"),
            lba9::UpperPayload::Sapf(_) => ProfileDetection::Unique("sapf"),
            lba9::UpperPayload::LongUser { .. } => ProfileDetection::Unique("long-user"),
            lba9::UpperPayload::Eppe(_) => ProfileDetection::Unique("eppe"),
            lba9::UpperPayload::Unknown(_) => ProfileDetection::Unknown,
        },
        Err(result) => result,
    }
}

pub fn detect_lba10_eesi(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    if sector(raw, 10).iter().all(|byte| *byte == 0) {
        return ProfileDetection::Unique("absent-zero");
    }
    let Some(crc) = context.device_crc() else {
        return ProfileDetection::MissingContext(vec!["device_id"]);
    };
    if lba10::parse_lba10(sector(raw, 10), crc, Lba10Eesi::EesiEnabled).is_ok() {
        ProfileDetection::Unique("eesi-enabled")
    } else {
        ProfileDetection::Unknown
    }
}

pub(crate) fn lba11_profile_candidates(
    raw: &[u8; 512],
    vid: &str,
    pid: &str,
    disk_size_bytes: u64,
) -> Vec<Lba11Capacity> {
    [Lba11Capacity::DiskSize, Lba11Capacity::RepairChs]
        .into_iter()
        .filter(|profile| lba11::parse_lba11(raw, vid, pid, disk_size_bytes, *profile).is_ok())
        .collect()
}

pub fn detect_lba11_capacity(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    let (Some(vid), Some(pid), Some(size)) = (context.vid, context.pid, context.disk_size_bytes)
    else {
        let mut missing = Vec::new();
        if context.vid.is_none() {
            missing.push("vid");
        }
        if context.pid.is_none() {
            missing.push("pid");
        }
        if context.disk_size_bytes.is_none() {
            missing.push("disk_size_bytes");
        }
        return ProfileDetection::MissingContext(missing);
    };
    ProfileDetection::from_candidates(
        lba11_profile_candidates(sector(raw, 11), vid, pid, size)
            .into_iter()
            .map(Lba11Capacity::as_str)
            .collect(),
    )
}

pub(crate) fn lba12_profile_candidates(raw: &[u8; 512], device_crc: u32) -> Vec<Lba12Mode> {
    [
        Lba12Mode::LegacyV0064,
        Lba12Mode::Mode1,
        Lba12Mode::Mode2,
        Lba12Mode::Mode3,
    ]
    .into_iter()
    .filter(|mode| lba12::parse_lba12(raw, device_crc, *mode).is_ok())
    .collect()
}

pub fn detect_lba12_mode(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    let Some(crc) = context.device_crc() else {
        return ProfileDetection::MissingContext(vec!["device_id"]);
    };
    ProfileDetection::from_candidates(
        lba12_profile_candidates(sector(raw, 12), crc)
            .into_iter()
            .map(Lba12Mode::as_str)
            .collect(),
    )
}

pub fn detect_lba4_second_key_source(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    provenance_lba4(raw, &["current-main-onlyid", "legacy-guid-crc"])
}

pub fn detect_lba4_hserial_source(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    provenance_lba4(raw, &["current-zero", "legacy-caller-vector"])
}

pub fn detect_host_hardinfo_source(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    _: ProfileDetectionContext<'_>,
) -> ProfileDetection {
    provenance_lba4(raw, &["current-zero", "legacy-host-identity"])
}

macro_rules! detector {
    ($axis:literal, [$($state:literal),+ $(,)?], $kind:ident, $fn:ident) => {
        ProfileAxisDetector {
            axis: $axis,
            states: &[$($state),+],
            kind: ProfileDetectorKind::$kind,
            symbol: concat!("edpcli::protocol::profile_detector::", stringify!($fn)),
            detect: $fn,
        }
    };
}

pub const PROFILE_AXIS_DETECTORS: &[ProfileAxisDetector] = &[
    detector!(
        "lba0_bootstrap",
        ["zero", "usb-main-bsec", "netac-mbr"],
        Wire,
        detect_lba0_bootstrap
    ),
    detector!(
        "lba0_sector_size_overlay",
        ["absent", "sector-size-512"],
        Wire,
        detect_lba0_sector_size_overlay
    ),
    detector!("gpt_layout", ["absent", "enabled"], Wire, detect_gpt_layout),
    detector!(
        "lba3_metadata",
        ["zero", "kingston-mp-a", "historical-mp-b"],
        Wire,
        detect_lba3_metadata
    ),
    detector!(
        "lba4_encoding",
        ["post-xor", "ordinary-rolling"],
        ProvenanceOnly,
        detect_lba4_encoding
    ),
    detector!(
        "dept_layout",
        ["short", "join59", "join60"],
        Wire,
        detect_dept_layout
    ),
    detector!(
        "lba6_mbr_underlay",
        ["zero-underlay", "legacy-mbr-snapshot"],
        Wire,
        detect_lba6_mbr_underlay
    ),
    detector!(
        "lba7_entry_count",
        ["two-entry", "three-entry"],
        Contextual,
        detect_lba7_entry_count
    ),
    detector!(
        "lba7_passinfo_version",
        ["legacy-v0064", "current-v0206"],
        Contextual,
        detect_lba7_passinfo_version
    ),
    detector!(
        "lba8_usb_only_info",
        ["current", "transitional-2019", "strict-legacy-absent"],
        Contextual,
        detect_lba8_usb_only_info
    ),
    detector!(
        "lba9_eetu",
        ["absent-zero", "eetu"],
        Contextual,
        detect_lba9_eetu
    ),
    detector!(
        "lba9_overlay",
        ["zero", "sapf", "long-user", "eppe"],
        Contextual,
        detect_lba9_overlay
    ),
    detector!(
        "lba10_eesi",
        ["absent-zero", "eesi-enabled"],
        Contextual,
        detect_lba10_eesi
    ),
    detector!(
        "lba11_capacity",
        ["disk-size", "repair-chs"],
        Contextual,
        detect_lba11_capacity
    ),
    detector!(
        "lba12_mode",
        ["legacy-v0064", "mode1", "mode2", "mode3"],
        Contextual,
        detect_lba12_mode
    ),
    detector!(
        "lba4_second_key_source",
        ["current-main-onlyid", "legacy-guid-crc"],
        ProvenanceOnly,
        detect_lba4_second_key_source
    ),
    detector!(
        "lba4_hserial_source",
        ["current-zero", "legacy-caller-vector"],
        ProvenanceOnly,
        detect_lba4_hserial_source
    ),
    detector!(
        "host_hardinfo_source",
        ["current-zero", "legacy-host-identity"],
        ProvenanceOnly,
        detect_host_hardinfo_source
    ),
];

pub fn profile_axis_detector(axis: &str) -> Option<&'static ProfileAxisDetector> {
    PROFILE_AXIS_DETECTORS
        .iter()
        .find(|entry| entry.axis == axis)
}

pub fn detect_profile_axes(
    raw: &[u8; PROTOCOL_IMAGE_BYTES],
    context: ProfileDetectionContext<'_>,
) -> Vec<ProfileAxisDetection> {
    PROFILE_AXIS_DETECTORS
        .iter()
        .map(|entry| ProfileAxisDetection {
            axis: entry.axis,
            detection: (entry.detect)(raw, context),
        })
        .collect()
}
