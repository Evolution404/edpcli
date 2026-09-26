use encoding_rs::GBK;

use crate::identify::{build_device_id, Transport};
use crate::platform::{HardwareProbe, NativeTransport};

use super::ProvisionProfile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnlyId {
    text: String,
    bits: u32,
}

impl OnlyId {
    pub fn random_candidate() -> Result<Self, String> {
        let mut raw = [0u8; 4];
        getrandom::fill(&mut raw)
            .map_err(|error| format!("failed to generate onlyid candidate: {error}"))?;
        let mut bits = u32::from_le_bytes(raw);
        if bits == 0 {
            bits = 1;
        }
        Self::parse(&bits.to_string())
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        if value.is_empty() || value.starts_with('+') || value.trim() != value {
            return Err(format!("invalid onlyid: {value:?}"));
        }
        let bits = if value.starts_with('-') {
            let signed = value
                .parse::<i32>()
                .map_err(|_| format!("onlyid signed value is outside i32: {value}"))?;
            if signed >= 0 {
                return Err(format!("invalid signed onlyid: {value}"));
            }
            signed as u32
        } else {
            value
                .parse::<u32>()
                .map_err(|_| format!("onlyid unsigned value is outside u32: {value}"))?
        };
        Ok(Self {
            text: value.to_string(),
            bits,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn bits(&self) -> u32 {
        self.bits
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetIdentity {
    device_id: String,
    vid: u16,
    pid: u16,
    total_sectors: u64,
    transport: NativeTransport,
}

impl TargetIdentity {
    pub fn from_probe(probe: &HardwareProbe, total_sectors: u64) -> Result<Self, String> {
        if total_sectors < 32_768 {
            return Err(format!(
                "target is too small for the canonical EDP metadata layout: {total_sectors} sectors"
            ));
        }
        if total_sectors > u64::MAX / crate::common::SECTOR as u64 {
            return Err("target byte capacity overflows u64".into());
        }
        let vid = probe.vid.ok_or("hardware probe is missing USB VID")?;
        let pid = probe.pid.ok_or("hardware probe is missing USB PID")?;
        let inquiry = probe
            .inquiry
            .as_ref()
            .ok_or("hardware probe is missing SCSI inquiry identity")?;
        if inquiry.vendor.trim().is_empty() || inquiry.product.trim().is_empty() {
            return Err("hardware probe has empty vendor/product identity".into());
        }
        let transport = match probe.transport {
            NativeTransport::Uas => Transport::Uas,
            NativeTransport::Bot if !inquiry.revision.trim().is_empty() => Transport::Bot,
            NativeTransport::Bot => {
                return Err("BOT hardware probe is missing revision identity".into())
            }
            NativeTransport::Unknown => {
                return Err("hardware transport is unknown; refusing to invent device_id".into())
            }
        };
        let device_id = build_device_id(
            &inquiry.vendor,
            &inquiry.product,
            &inquiry.revision,
            transport,
        );
        if device_id.is_empty() || device_id.len() > 128 {
            return Err("derived device_id is empty or too long".into());
        }
        Ok(Self {
            device_id,
            vid,
            pid,
            total_sectors,
            transport: probe.transport,
        })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn vid(&self) -> u16 {
        self.vid
    }

    pub fn pid(&self) -> u16 {
        self.pid
    }

    pub fn vid_hex(&self) -> String {
        format!("{:04x}", self.vid)
    }

    pub fn pid_hex(&self) -> String {
        format!("{:04x}", self.pid)
    }

    pub fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    pub fn transport(&self) -> NativeTransport {
        self.transport
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionMetadata {
    onlyid: OnlyId,
    user: String,
    dept: String,
    label: String,
}

fn validate_gbk_field(name: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    if value.as_bytes().contains(&0) || value.contains('|') || value.contains('=') {
        return Err(format!("{name} contains protocol delimiter/control bytes"));
    }
    let (encoded, _, had_errors) = GBK.encode(value);
    if had_errors {
        return Err(format!("{name} is not losslessly representable in GBK"));
    }
    if encoded.len() > max_bytes {
        return Err(format!(
            "{name} is too long after GBK encoding: {} > {max_bytes} bytes",
            encoded.len()
        ));
    }
    Ok(())
}

impl ProvisionMetadata {
    pub fn new(
        onlyid: OnlyId,
        user: impl Into<String>,
        dept: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<Self, String> {
        let user = user.into();
        let dept = dept.into();
        let label = label.into();
        validate_gbk_field("User", &user, 31)?;
        validate_gbk_field("Dept", &dept, 63)?;
        validate_gbk_field("Label", &label, 31)?;
        Ok(Self {
            onlyid,
            user,
            dept,
            label,
        })
    }

    pub fn onlyid(&self) -> &OnlyId {
        &self.onlyid
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn dept(&self) -> &str {
        &self.dept
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionSpec {
    target: TargetIdentity,
    metadata: ProvisionMetadata,
    profile: ProvisionProfile,
}

impl ProvisionSpec {
    pub fn new(
        target: TargetIdentity,
        metadata: ProvisionMetadata,
        profile: ProvisionProfile,
    ) -> Result<Self, String> {
        let (dept, _, dept_errors) = GBK.encode(metadata.dept());
        let (user, _, user_errors) = GBK.encode(metadata.user());
        let (label, _, label_errors) = GBK.encode(metadata.label());
        debug_assert!(!dept_errors && !user_errors && !label_errors);
        let llgb_payload_len = 8
            + "GLab=".len()
            + profile.glab().len()
            + "||Indus=||Orgcd=||Org=||Unit=||Dept=".len()
            + dept.len()
            + "||User=".len()
            + user.len()
            + "||Alarm=||Autonum=".len()
            + profile.autonum().len()
            + "||Label=".len()
            + label.len()
            + "||Rmark=||VOL0=||VOL1=||VOL2=||VOLC0=||VOLC1=||VOLC2=||".len();
        // BuildSector8 stores the ELABEL C string at +0x80. The LLGB +0x04
        // logical length excludes the terminating NUL, so the largest legal
        // body is 0x17F bytes: +0x80 + 0x17F = 0x1FF, leaving byte 0x1FF
        // for the NUL that is covered by the final encrypted block.
        if llgb_payload_len > 0x17F {
            return Err(format!(
                "metadata does not fit canonical LBA8 LLGB payload: {llgb_payload_len} > 383 bytes"
            ));
        }
        Ok(Self {
            target,
            metadata,
            profile,
        })
    }

    pub fn target(&self) -> &TargetIdentity {
        &self.target
    }

    pub fn metadata(&self) -> &ProvisionMetadata {
        &self.metadata
    }

    pub fn profile(&self) -> &ProvisionProfile {
        &self.profile
    }
}
