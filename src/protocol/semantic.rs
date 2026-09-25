//! Typed cross-domain protocol semantics.
//!
//! This layer exposes decoded protocol facts without presentation labels, colors,
//! field groups, or CLI/TUI formatting. Inspect maps these facts into views.

use encoding_rs::GBK;

use super::{
    edpf::{EdpfEntry64, EdpfEntry96},
    lba11, lba12, lba6, lba7, lba8,
    profile::{
        HostHardinfoSource, Lba11Capacity, Lba12Mode, Lba7EntryCount, Lba7PassinfoVersion,
        Lba8UsbOnlyInfo,
    },
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticContext {
    pub device_id: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub size_bytes: Option<u64>,
    pub onlyid: Option<String>,
}

pub trait SemanticContextSource {
    fn semantic_context(&self) -> SemanticContext;
}

impl SemanticContextSource for SemanticContext {
    fn semantic_context(&self) -> SemanticContext {
        self.clone()
    }
}

impl SemanticContext {
    pub fn device_crc(&self) -> Option<u32> {
        self.device_id
            .as_deref()
            .map(|value| crate::crypto::crc32_bare(value.as_bytes()))
    }

    pub fn onlyid_bits(&self) -> Option<u32> {
        let text = self.onlyid.as_deref()?;
        if text.starts_with('-') {
            text.parse::<i32>().ok().map(|value| value as u32)
        } else {
            text.parse::<u32>().ok()
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OwnershipSemantics {
    pub glab: Option<String>,
    pub dept: Option<String>,
    pub user: Option<String>,
    pub label: Option<String>,
    pub rmark: Option<String>,
    pub autonum: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionSemantics {
    pub source: &'static str,
    pub index: usize,
    pub partition_type: u32,
    pub need_disturb: u32,
    pub need_encrypt: u32,
    pub start_sector: u64,
    pub sector_size: u64,
    pub partition_size: u64,
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or(bytes);
    if bytes.is_empty() {
        return None;
    }
    let (decoded, had_errors) = GBK.decode_without_bom_handling(bytes);
    let value = if had_errors {
        decoded.replace('\u{FFFD}', "")
    } else {
        decoded.into_owned()
    };
    (!value.is_empty()).then_some(value)
}

pub fn parse_elabel(body: &[u8]) -> OwnershipSemantics {
    let text = body.strip_prefix(b"<ELABEL>").unwrap_or(body);
    let mut out = OwnershipSemantics::default();
    for part in text
        .split(|byte| *byte == b'|')
        .filter(|part| !part.is_empty())
    {
        let Some(eq) = part.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        let Some(key) = decode_text(&part[..eq]) else {
            continue;
        };
        let value = decode_text(&part[eq + 1..])
            .map(|value| value.strip_prefix("*^$@").unwrap_or(&value).to_string());
        match key.as_str() {
            "GLab" => out.glab = value,
            "Dept" => out.dept = value,
            "User" => out.user = value,
            "Label" => out.label = value,
            "Rmark" => out.rmark = value,
            "Autonum" => out.autonum = value,
            _ => {}
        }
    }
    out
}

pub fn lba6_view(raw: &[u8]) -> Option<lba6::Lba6View> {
    let raw: &[u8; 512] = raw.try_into().ok()?;
    lba6::parse_lba6(raw).ok()
}

pub fn safe6_label(raw: &[u8]) -> Option<String> {
    decode_text(lba6_view(raw)?.label.value())
}

pub fn safe6_user(raw: &[u8]) -> Option<String> {
    decode_text(lba6_view(raw)?.user.value())
}

pub fn safe6_gserial(raw: &[u8]) -> Option<String> {
    decode_text(lba6_view(raw)?.gserial.value())
}

pub fn infer_lba7(
    raw: &[u8; 512],
    device_crc: u32,
) -> Option<(lba7::Lba7View, Lba7EntryCount, Lba7PassinfoVersion)> {
    let mut matches = Vec::new();
    for entry_count in [Lba7EntryCount::TwoEntry, Lba7EntryCount::ThreeEntry] {
        for passinfo in [
            Lba7PassinfoVersion::LegacyV0064,
            Lba7PassinfoVersion::CurrentV0206,
        ] {
            if let Ok(view) = lba7::parse_lba7(raw, device_crc, entry_count, passinfo) {
                matches.push((view, entry_count, passinfo));
            }
        }
    }
    (matches.len() == 1).then(|| matches.remove(0))
}

pub fn infer_lba8(
    raw: &[u8; 512],
    device_crc: u32,
    onlyid: Option<u32>,
) -> Option<(
    lba8::Lba8View,
    Vec<Lba8UsbOnlyInfo>,
    Vec<HostHardinfoSource>,
)> {
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
            let context = lba8::Lba8Context {
                usb_only_info,
                host_hardinfo_source,
                main_onlyid: onlyid,
            };
            if let Ok(view) = lba8::parse_lba8(raw, device_crc, context) {
                matches.push((view, usb_only_info, host_hardinfo_source));
            }
        }
    }
    let first = matches.first()?.0.clone();
    let mut usb_profiles = Vec::new();
    let mut host_profiles = Vec::new();
    for (_, usb, host) in matches {
        if !usb_profiles.contains(&usb) {
            usb_profiles.push(usb);
        }
        if !host_profiles.contains(&host) {
            host_profiles.push(host);
        }
    }
    Some((first, usb_profiles, host_profiles))
}

pub fn ownership_from_lba8(raw: &[u8], context: &SemanticContext) -> Option<OwnershipSemantics> {
    let raw: &[u8; 512] = raw.try_into().ok()?;
    let (view, _, _) = infer_lba8(raw, context.device_crc()?, context.onlyid_bits())?;
    Some(parse_elabel(&view.elabel_body))
}

pub fn infer_lba11(
    raw: &[u8; 512],
    context: &SemanticContext,
) -> Option<(lba11::Lba11View, Vec<Lba11Capacity>)> {
    let vid = context.vid.as_deref()?;
    let pid = context.pid.as_deref()?;
    let size = context.size_bytes?;
    let mut matches = Vec::new();
    for profile in [Lba11Capacity::DiskSize, Lba11Capacity::RepairChs] {
        if let Ok(view) = lba11::parse_lba11(raw, vid, pid, size, profile) {
            matches.push((view, profile));
        }
    }
    let first = matches.first()?.0.clone();
    Some((
        first,
        matches.into_iter().map(|(_, profile)| profile).collect(),
    ))
}

pub fn pdkb_device_id(raw: &[u8], context: &SemanticContext) -> Option<String> {
    let raw: &[u8; 512] = raw.try_into().ok()?;
    let (view, _) = infer_lba11(raw, context)?;
    decode_text(view.uid.value())
}

pub fn infer_lba12(raw: &[u8; 512], device_crc: u32) -> Option<(lba12::Lba12View, Vec<Lba12Mode>)> {
    let mut matches = Vec::new();
    for mode in [
        Lba12Mode::LegacyV0064,
        Lba12Mode::Mode1,
        Lba12Mode::Mode2,
        Lba12Mode::Mode3,
    ] {
        if let Ok(view) = lba12::parse_lba12(raw, device_crc, mode) {
            matches.push((view, mode));
        }
    }
    let first = matches.first()?.0.clone();
    Some((first, matches.into_iter().map(|(_, mode)| mode).collect()))
}

fn partition64(source: &'static str, index: usize, entry: &EdpfEntry64) -> PartitionSemantics {
    PartitionSemantics {
        source,
        index,
        partition_type: entry.partition_type,
        need_disturb: entry.need_disturb,
        need_encrypt: entry.need_encrypt,
        start_sector: entry.start_sector,
        sector_size: entry.sector_size,
        partition_size: entry.partition_size,
    }
}

fn partition96(source: &'static str, index: usize, entry: &EdpfEntry96) -> PartitionSemantics {
    PartitionSemantics {
        source,
        index,
        partition_type: entry.partition_type,
        need_disturb: entry.need_disturb,
        need_encrypt: entry.need_encrypt,
        start_sector: entry.start_sector,
        sector_size: entry.sector_size,
        partition_size: entry.partition_size,
    }
}

pub fn lba7_partitions(raw: &[u8], context: &SemanticContext) -> Vec<PartitionSemantics> {
    let Some(crc) = context.device_crc() else {
        return Vec::new();
    };
    let Ok(raw) = <&[u8; 512]>::try_from(raw) else {
        return Vec::new();
    };
    let Some((view, _, _)) = infer_lba7(raw, crc) else {
        return Vec::new();
    };
    let mut out = view
        .entries_0_1
        .iter()
        .enumerate()
        .map(|(index, entry)| partition64("LBA7", index, entry))
        .collect::<Vec<_>>();
    if let lba7::Entry2::Present(entry) = &view.entry2 {
        out.push(partition64("LBA7", 2, entry));
    }
    out
}

pub fn lba12_partitions(raw: &[u8], context: &SemanticContext) -> Vec<PartitionSemantics> {
    let Some(crc) = context.device_crc() else {
        return Vec::new();
    };
    let Ok(raw) = <&[u8; 512]>::try_from(raw) else {
        return Vec::new();
    };
    let Some((view, _)) = infer_lba12(raw, crc) else {
        return Vec::new();
    };
    view.entries
        .iter()
        .enumerate()
        .map(|(index, entry)| partition96("LBA12", index, entry))
        .collect()
}
