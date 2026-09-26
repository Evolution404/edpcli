use super::*;

#[derive(Debug, Clone, Default)]
pub struct InspectMeta {
    pub device_id: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub size_bytes: Option<u64>,
    pub onlyid: Option<String>,
}

impl crate::protocol::semantic::SemanticContextSource for InspectMeta {
    fn semantic_context(&self) -> crate::protocol::semantic::SemanticContext {
        semantic_context(self)
    }
}

pub(super) fn crc_key(meta: &InspectMeta) -> Option<(u32, [u8; 4])> {
    let did = meta.device_id.as_deref()?;
    let crc = crc32_bare(did.as_bytes());
    Some((crc, crc.to_le_bytes()))
}

pub(super) fn meta_onlyid(meta: &InspectMeta) -> Option<u32> {
    let text = meta.onlyid.as_deref()?;
    text.parse::<u32>()
        .ok()
        .or_else(|| text.parse::<i32>().ok().map(|value| value as u32))
}

pub(super) fn semantic_context(meta: &InspectMeta) -> crate::protocol::semantic::SemanticContext {
    crate::protocol::semantic::SemanticContext {
        device_id: meta.device_id.clone(),
        vid: meta.vid.clone(),
        pid: meta.pid.clone(),
        size_bytes: meta.size_bytes,
        onlyid: meta.onlyid.clone(),
    }
}

pub(super) fn infer_lba7(
    raw: &[u8; SECTOR],
    device_crc: u32,
) -> Option<(lba7::Lba7View, Lba7EntryCount, Lba7PassinfoVersion)> {
    crate::protocol::semantic::infer_lba7(raw, device_crc)
}

pub(super) fn infer_lba8(
    raw: &[u8; SECTOR],
    device_crc: u32,
    onlyid: Option<u32>,
) -> Option<(
    lba8::Lba8View,
    Vec<Lba8UsbOnlyInfo>,
    Vec<HostHardinfoSource>,
)> {
    crate::protocol::semantic::infer_lba8(raw, device_crc, onlyid)
}

pub(super) fn infer_lba11(
    raw: &[u8; SECTOR],
    meta: &InspectMeta,
) -> Option<(lba11::Lba11View, Vec<Lba11Capacity>)> {
    crate::protocol::semantic::infer_lba11(raw, &semantic_context(meta))
}

pub(super) fn infer_lba12(
    raw: &[u8; SECTOR],
    device_crc: u32,
) -> Option<(lba12::Lba12View, Vec<Lba12Mode>)> {
    crate::protocol::semantic::infer_lba12(raw, device_crc)
}
