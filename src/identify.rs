//! device_id 识别 (macOS ioreg INQUIRY + 传输模式; LBA7 EDPF magic 判真)。

use std::time::Duration;

use crate::crypto::{crc32_bare, xor_rolling};
use crate::sysinfo::{block_str_field, split_class_blocks, CmdRunner};

const IOREG_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Uas,
    Bot,
    Unknown,
}

fn norm(s: &str) -> String {
    s.trim_end_matches(' ').replace(' ', "_").to_lowercase()
}

pub fn build_device_id(vendor: &str, product: &str, revision: &str, transport: Transport) -> String {
    // Windows InstanceId 中间段: BOT(usbstor)含 &rev_, UAS(uaspstor)通常不含。
    let base = format!("disk&ven_{}&prod_{}", norm(vendor), norm(product));
    if transport == Transport::Bot {
        let r = norm(revision);
        if !r.is_empty() {
            return format!("{}&rev_{}", base, r);
        }
    }
    base
}

fn ioreg_fields(
    runner: &dyn CmdRunner,
    cls: &str,
    disk: u32,
    keys: &[&str],
) -> Vec<(String, String)> {
    // 失败(无权限/超时/无该类) → 空, 调用方按缺失处理
    let Ok(out) = runner.check_output(&["ioreg", "-r", "-c", cls, "-l"], IOREG_TIMEOUT) else {
        return vec![];
    };
    let want = format!("\"BSD Name\" = \"disk{}\"", disk);
    for b in split_class_blocks(&out, cls) {
        if !b.contains(&want) {
            continue;
        }
        // 首个含 BSD Name 的块; 只取存在的键
        return keys
            .iter()
            .filter_map(|k| block_str_field(b, k).map(|v| (k.to_string(), v)))
            .collect();
    }
    vec![]
}

pub fn detect_transport(runner: &dyn CmdRunner, disk: u32) -> Transport {
    let mut present: Vec<&str> = Vec::new();
    for cls in [
        "IOUSBMassStorageUASDriver",
        "IOUSBMassStorageInterfaceNub",
        "IOUSBMassStorageDriver",
    ] {
        if let Ok(out) = runner.check_output(&["ioreg", "-r", "-c", cls, "-l"], IOREG_TIMEOUT) {
            if out.contains(&format!("\"BSD Name\" = \"disk{}\"", disk)) {
                present.push(cls);
            }
        }
    }
    if present.contains(&"IOUSBMassStorageUASDriver") {
        Transport::Uas
    } else if present.contains(&"IOUSBMassStorageInterfaceNub")
        || present.contains(&"IOUSBMassStorageDriver")
    {
        Transport::Bot
    } else {
        Transport::Unknown
    }
}

pub fn generate_candidates(runner: &dyn CmdRunner, disk: u32) -> Vec<String> {
    let mut cs: Vec<String> = Vec::new();
    let transport = detect_transport(runner, disk);
    for cls in ["IOSCSITargetDevice", "IOSCSILogicalUnitNub", "IOSCSIPeripheralDeviceNub"] {
        let d = ioreg_fields(
            runner,
            cls,
            disk,
            &["Vendor Identification", "Product Identification", "Product Revision Level"],
        );
        let get = |k: &str| d.iter().find(|(dk, _)| dk == k).map(|(_, v)| v.clone());
        if let Some(v) = get("Vendor Identification").filter(|v| !v.is_empty()) {
            let p = get("Product Identification").unwrap_or_default();
            let rev = get("Product Revision Level").unwrap_or_default();
            let long_id = build_device_id(&v, &p, &rev, Transport::Bot);
            let short_id = build_device_id(&v, &p, &rev, Transport::Uas);
            let ordered: Vec<String> = if transport == Transport::Uas {
                vec![short_id, long_id]
            } else {
                vec![long_id, short_id]
            };
            for c in ordered {
                if !c.is_empty() && !cs.contains(&c) {
                    cs.push(c);
                }
            }
            break; // 首个给出 Vendor 的类即止
        }
    }
    cs
}

pub struct IdentifyResult {
    pub device_id: Option<String>,
    pub crc: Option<u32>,
    pub k0: Option<u32>,
}

/// 两候选 LBA7 EDPF magic 判真。lba7 为该盘 LBA7 原始 512B。
pub fn identify(runner: &dyn CmdRunner, disk: u32, lba7: &[u8]) -> IdentifyResult {
    for c in generate_candidates(runner, disk) {
        let crc = crc32_bare(c.as_bytes());
        let k0 = (crc & 0xFFFF) ^ (crc >> 16);
        if xor_rolling(lba7, k0)[..4] == *b"EDPF" {
            return IdentifyResult { device_id: Some(c), crc: Some(crc), k0: Some(k0) };
        }
    }
    IdentifyResult { device_id: None, crc: None, k0: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_trailing_space_lower_underscore() {
        assert_eq!(norm("Netac  "), "netac");
        assert_eq!(norm("My Prod"), "my_prod");
        assert_eq!(norm(""), "");
    }

    #[test]
    fn build_device_id_transports() {
        assert_eq!(
            build_device_id("AIGO", "U335", "PMAP", Transport::Bot),
            "disk&ven_aigo&prod_u335&rev_pmap"
        );
        assert_eq!(
            build_device_id("AIGO", "U335", "PMAP", Transport::Uas),
            "disk&ven_aigo&prod_u335"
        );
        assert_eq!(build_device_id("V", "P", "", Transport::Bot), "disk&ven_v&prod_p");
        assert_eq!(build_device_id("V", "P", "R1", Transport::Unknown), "disk&ven_v&prod_p");
    }
}
