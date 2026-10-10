//! Read-only source-mode classifier shared by device discovery and every
//! provisioning frontend. Blank media are a valid plain-disk source.
use super::{parse_existing_provision_native, DiskProvisionKind};
use crate::protocol::image::NativeProtocolImage;

pub fn classify_native_source(
    prefix: &[Vec<u8>],
    sector: u32,
    total: u64,
    device_id: &str,
) -> Result<DiskProvisionKind, String> {
    if prefix.len() != 13 || prefix.iter().any(|b| b.len() != sector as usize) {
        return Err("来源13个原生块的长度或数量不完整".into());
    }
    let bytes: Vec<u8> = prefix.iter().flat_map(|b| b.iter().copied()).collect();
    if bytes.iter().all(|&byte| byte == 0) {
        return Ok(DiskProvisionKind::Plain);
    }
    let protocol = NativeProtocolImage::from_native_bytes(sector, bytes)
        .map_err(|e| format!("来源盘原生协议投影失败: {e}"))?;
    if let Some(parsed) = parse_existing_provision_native(&protocol, device_id, total)? {
        return Ok(DiskProvisionKind::from_mode(parsed.profile.source_mode));
    }
    if crate::partition_table::confirmed_plain_protocol_prefix(
        &protocol.protocol_projection(),
        total,
    ) {
        Ok(DiskProvisionKind::Plain)
    } else {
        Err("来源盘不是已确认的Plain或EDP模式，不能当成普通盘破坏性重建".into())
    }
}
