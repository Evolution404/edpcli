//! device_id 识别（平台硬件探测 + LBA7 EDPF magic 判真）。

use crate::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use crate::ports::CmdRunner;
use crate::protocol::crypto::{crc32_bare, xor_rolling};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Uas,
    Bot,
    Unknown,
}

fn norm(s: &str) -> String {
    s.trim_end_matches(' ').replace(' ', "_").to_lowercase()
}

fn pnp_component(s: &str) -> String {
    s.trim_end_matches(' ').replace(' ', "_")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsPnpIdentity {
    /// Native or reconstructed Windows disk PnP InstanceId prefix + identity segment.
    pub instance_id: String,
    /// Lower-cased `Disk&Ven_...&Prod_...` middle segment before CEMS compatibility trimming.
    pub full_device_id: String,
    /// CEMS compatibility short form. USBSTOR removes the final `&...` suffix.
    pub short_device_id: String,
    /// Current official CEMS writer value: USBSTOR short form, otherwise full form.
    pub write_device_id: String,
}

fn windows_disk_identity_segment(
    inquiry: &InquiryInfo,
    transport: NativeTransport,
) -> Result<String, String> {
    let vendor = pnp_component(&inquiry.vendor);
    let product = pnp_component(&inquiry.product);
    let revision = pnp_component(&inquiry.revision);
    match transport {
        // Windows USBSTOR disk instance paths carry the SCSI inquiry revision in the
        // `Disk&Ven_...&Prod_...&Rev_...` segment. Empty fields are still legal.
        NativeTransport::Bot => Ok(format!("Disk&Ven_{vendor}&Prod_{product}&Rev_{revision}")),
        // UAS disks are exposed through the Windows SCSI disk stack. Observed Windows
        // PnP disk instance paths use the Disk&Ven_/Prod_ segment without requiring Rev.
        NativeTransport::Uas => Ok(format!("Disk&Ven_{vendor}&Prod_{product}")),
        NativeTransport::Unknown => {
            Err("hardware transport is unknown; cannot reconstruct Windows PnP InstanceId".into())
        }
    }
}

pub fn reconstruct_windows_pnp_instance_id(
    inquiry: &InquiryInfo,
    transport: NativeTransport,
) -> Result<String, String> {
    let segment = windows_disk_identity_segment(inquiry, transport)?;
    let enumerator = match transport {
        NativeTransport::Bot => "USBSTOR",
        NativeTransport::Uas => "SCSI",
        NativeTransport::Unknown => unreachable!(),
    };
    Ok(format!(r"{enumerator}\{segment}"))
}

pub fn windows_pnp_identity_from_instance_id(
    instance_id: &str,
) -> Result<WindowsPnpIdentity, String> {
    let mut parts = instance_id.split('\\');
    let enumerator = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or("Windows PnP InstanceId is missing enumerator")?;
    let segment = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or("Windows PnP InstanceId is missing disk identity segment")?;
    let lower = segment.to_ascii_lowercase();
    if !lower.starts_with("disk&ven_") || !lower.contains("&prod_") {
        return Err(format!(
            "Windows PnP InstanceId does not contain a Disk&Ven_/Prod_ identity segment: {instance_id}"
        ));
    }
    let short = if enumerator.eq_ignore_ascii_case("USBSTOR") {
        lower
            .rfind('&')
            .map(|index| lower[..index].to_string())
            .unwrap_or_else(|| lower.clone())
    } else {
        lower.clone()
    };
    let write_device_id = if enumerator.eq_ignore_ascii_case("USBSTOR") {
        short.clone()
    } else {
        lower.clone()
    };
    Ok(WindowsPnpIdentity {
        instance_id: instance_id.to_string(),
        full_device_id: lower,
        short_device_id: short,
        write_device_id,
    })
}

pub fn windows_pnp_identity_from_probe(
    probe: &HardwareProbe,
) -> Result<WindowsPnpIdentity, String> {
    if let Some(instance_id) = probe
        .windows_pnp_instance_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return windows_pnp_identity_from_instance_id(instance_id);
    }
    let inquiry = probe
        .inquiry
        .as_ref()
        .ok_or("hardware probe is missing SCSI inquiry identity for Windows PnP reconstruction")?;
    let instance_id = reconstruct_windows_pnp_instance_id(inquiry, probe.transport)?;
    windows_pnp_identity_from_instance_id(&instance_id)
}

/// Current official CEMS write identity reconstructed from Windows PnP semantics.
pub fn build_device_id(
    vendor: &str,
    product: &str,
    revision: &str,
    transport: Transport,
) -> String {
    let native = match transport {
        Transport::Uas => NativeTransport::Uas,
        Transport::Bot => NativeTransport::Bot,
        Transport::Unknown => NativeTransport::Unknown,
    };
    let inquiry = InquiryInfo {
        vendor: vendor.into(),
        product: product.into(),
        revision: revision.into(),
    };
    reconstruct_windows_pnp_instance_id(&inquiry, native)
        .and_then(|instance_id| windows_pnp_identity_from_instance_id(&instance_id))
        .map(|identity| identity.write_device_id)
        .unwrap_or_default()
}

fn transport_from_native(probe: &HardwareProbe) -> Transport {
    match probe.transport {
        NativeTransport::Uas => Transport::Uas,
        NativeTransport::Bot => Transport::Bot,
        NativeTransport::Unknown => Transport::Unknown,
    }
}

fn push_candidate(cs: &mut Vec<String>, candidate: String) {
    if !candidate.is_empty() && !cs.contains(&candidate) {
        cs.push(candidate);
    }
}

fn push_candidate_pair(
    cs: &mut Vec<String>,
    vendor: &str,
    product: &str,
    revision: &str,
    transport: Transport,
) {
    // Read compatibility keeps both historical forms. The current writer uses the
    // PnP-derived value above; older media can still contain the full &rev_ form.
    let base = format!("disk&ven_{}&prod_{}", norm(vendor), norm(product));
    let revision = norm(revision);
    let long_id = if revision.is_empty() {
        format!("{base}&rev_")
    } else {
        format!("{base}&rev_{revision}")
    };
    let short_id = base;
    let ordered = if transport == Transport::Uas {
        [short_id, long_id]
    } else {
        [long_id, short_id]
    };
    for candidate in ordered {
        push_candidate(cs, candidate);
    }
}

pub fn generate_candidates(runner: &dyn CmdRunner, disk: u32) -> Vec<String> {
    let mut cs: Vec<String> = Vec::new();
    let native = runner.hardware_probe(disk);
    let fallback = native
        .as_ref()
        .is_none_or(|probe| {
            probe.transport == NativeTransport::Unknown
                || probe
                    .inquiry
                    .as_ref()
                    .is_none_or(|inquiry| inquiry.vendor.is_empty())
        })
        .then(|| crate::platform::fallback_hardware_probe(runner, disk))
        .flatten();
    let transport = native
        .as_ref()
        .map(transport_from_native)
        .filter(|transport| *transport != Transport::Unknown)
        .or_else(|| {
            fallback
                .as_ref()
                .map(transport_from_native)
                .filter(|transport| *transport != Transport::Unknown)
        })
        .unwrap_or(Transport::Unknown);

    if let Some(probe) = native.as_ref() {
        if let Ok(identity) = windows_pnp_identity_from_probe(probe) {
            push_candidate(&mut cs, identity.write_device_id);
            push_candidate(&mut cs, identity.full_device_id);
            push_candidate(&mut cs, identity.short_device_id);
        }
    }

    let inquiry = native
        .as_ref()
        .and_then(|probe| probe.inquiry.as_ref())
        .filter(|inquiry| !inquiry.vendor.is_empty())
        .or_else(|| fallback.as_ref().and_then(|probe| probe.inquiry.as_ref()));
    if let Some(inquiry) = inquiry {
        if !inquiry.vendor.is_empty() {
            push_candidate_pair(
                &mut cs,
                &inquiry.vendor,
                &inquiry.product,
                &inquiry.revision,
                transport,
            );
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
            return IdentifyResult {
                device_id: Some(c),
                crc: Some(crc),
                k0: Some(k0),
            };
        }
    }
    IdentifyResult {
        device_id: None,
        crc: None,
        k0: None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io;
    use std::time::Duration;

    use super::*;
    use crate::platform::{HardwareProbe, InquiryInfo, NativeTransport};

    struct NativeOnlyRunner {
        probe: HardwareProbe,
        subprocess_calls: Cell<usize>,
    }

    impl CmdRunner for NativeOnlyRunner {
        fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
            self.subprocess_calls.set(self.subprocess_calls.get() + 1);
            Err(io::Error::other(
                "native path should not spawn fallback subprocesses",
            ))
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
    fn build_device_id_uses_current_official_pnp_write_semantics() {
        assert_eq!(
            build_device_id("AIGO", "U335", "PMAP", Transport::Bot),
            "disk&ven_aigo&prod_u335"
        );
        assert_eq!(
            build_device_id("AIGO", "U335", "PMAP", Transport::Uas),
            "disk&ven_aigo&prod_u335"
        );
        assert_eq!(
            build_device_id("HIKSEMI", "", "1.00", Transport::Uas),
            "disk&ven_hiksemi&prod_"
        );
        assert_eq!(build_device_id("V", "P", "R1", Transport::Unknown), "");
    }

    #[test]
    fn windows_pnp_model_matches_cems_usbstor_short_and_scsi_full_rules() {
        let bot = windows_pnp_identity_from_instance_id(
            r"USBSTOR\Disk&Ven_SanDisk&Prod_Ultra_USB_3.0&Rev_1.00\SERIAL&0",
        )
        .unwrap();
        assert_eq!(
            bot.full_device_id,
            "disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00"
        );
        assert_eq!(bot.write_device_id, "disk&ven_sandisk&prod_ultra_usb_3.0");

        let uas =
            windows_pnp_identity_from_instance_id(r"SCSI\Disk&Ven_HIKSEMI&Prod_\6&1234&0&000000")
                .unwrap();
        assert_eq!(uas.full_device_id, "disk&ven_hiksemi&prod_");
        assert_eq!(uas.write_device_id, "disk&ven_hiksemi&prod_");
    }

    #[test]
    fn reconstructed_windows_pnp_identity_preserves_empty_product() {
        let inquiry = InquiryInfo {
            vendor: "HIKSEMI".into(),
            product: "".into(),
            revision: "1.00".into(),
        };
        assert_eq!(
            reconstruct_windows_pnp_instance_id(&inquiry, NativeTransport::Uas).unwrap(),
            r"SCSI\Disk&Ven_HIKSEMI&Prod_"
        );
        assert_eq!(
            reconstruct_windows_pnp_instance_id(&inquiry, NativeTransport::Bot).unwrap(),
            r"USBSTOR\Disk&Ven_HIKSEMI&Prod_&Rev_1.00"
        );
    }

    #[test]
    fn native_probe_builds_candidates_without_fallback_subprocess() {
        let runner = NativeOnlyRunner {
            probe: HardwareProbe {
                vid: Some(0x3535),
                pid: Some(0x6300),
                transport: NativeTransport::Bot,
                windows_pnp_instance_id: None,
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
                "disk&ven_aigo&prod_u335".to_string(),
                "disk&ven_aigo&prod_u335&rev_pmap".to_string(),
            ]
        );
        assert_eq!(runner.subprocess_calls.get(), 0);
    }
}
