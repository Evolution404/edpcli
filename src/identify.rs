//! device_id 识别 (macOS ioreg INQUIRY + 传输模式; LBA7 EDPF magic 判真)。

use std::time::Duration;

use crate::crypto::{crc32_bare, xor_rolling};
use crate::platform::{HardwareProbe, NativeTransport};
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

fn transport_from_native(probe: &HardwareProbe) -> Transport {
    match probe.transport {
        NativeTransport::Uas => Transport::Uas,
        NativeTransport::Bot => Transport::Bot,
        NativeTransport::Unknown => Transport::Unknown,
    }
}

fn detect_transport_ioreg(runner: &dyn CmdRunner, disk: u32) -> Transport {
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

pub fn detect_transport(runner: &dyn CmdRunner, disk: u32) -> Transport {
    if let Some(probe) = runner.hardware_probe(disk) {
        let transport = transport_from_native(&probe);
        if transport != Transport::Unknown {
            return transport;
        }
    }
    detect_transport_ioreg(runner, disk)
}

fn push_candidate_pair(
    cs: &mut Vec<String>,
    vendor: &str,
    product: &str,
    revision: &str,
    transport: Transport,
) {
    let long_id = build_device_id(vendor, product, revision, Transport::Bot);
    let short_id = build_device_id(vendor, product, revision, Transport::Uas);
    let ordered = if transport == Transport::Uas {
        [short_id, long_id]
    } else {
        [long_id, short_id]
    };
    for candidate in ordered {
        if !candidate.is_empty() && !cs.contains(&candidate) {
            cs.push(candidate);
        }
    }
}

pub fn generate_candidates(runner: &dyn CmdRunner, disk: u32) -> Vec<String> {
    let mut cs: Vec<String> = Vec::new();
    let native = runner.hardware_probe(disk);
    let transport = native
        .as_ref()
        .map(transport_from_native)
        .filter(|transport| *transport != Transport::Unknown)
        .unwrap_or_else(|| detect_transport_ioreg(runner, disk));

    if let Some(inquiry) = native.as_ref().and_then(|probe| probe.inquiry.as_ref()) {
        if !inquiry.vendor.is_empty() {
            push_candidate_pair(
                &mut cs,
                &inquiry.vendor,
                &inquiry.product,
                &inquiry.revision,
                transport,
            );
            return cs;
        }
    }

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
            push_candidate_pair(&mut cs, &v, &p, &rev, transport);
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
    use std::cell::Cell;
    use std::io;

    use super::*;
    use crate::platform::{HardwareProbe, InquiryInfo, NativeTransport};

    struct NativeOnlyRunner {
        probe: HardwareProbe,
        subprocess_calls: Cell<usize>,
    }

    impl CmdRunner for NativeOnlyRunner {
        fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
            self.subprocess_calls.set(self.subprocess_calls.get() + 1);
            Err(io::Error::other("native path should not spawn ioreg"))
        }

        fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
            Some(self.probe.clone())
        }
    }

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

    #[test]
    fn native_probe_builds_candidates_without_ioreg() {
        let runner = NativeOnlyRunner {
            probe: HardwareProbe {
                vid: Some(0x3535),
                pid: Some(0x6300),
                transport: NativeTransport::Bot,
                inquiry: Some(InquiryInfo {
                    vendor: "AIGO    ".into(),
                    product: "U335".into(),
                    revision: "PMAP".into(),
                }),
            },
            subprocess_calls: Cell::new(0),
        };
        assert_eq!(
            generate_candidates(&runner, 6),
            vec![
                "disk&ven_aigo&prod_u335&rev_pmap".to_string(),
                "disk&ven_aigo&prod_u335".to_string(),
            ]
        );
        assert_eq!(runner.subprocess_calls.get(), 0);
    }
}
