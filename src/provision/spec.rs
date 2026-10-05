use encoding_rs::GBK;

use crate::domain::hardware::{HardwareProbe, NativeTransport};
use crate::identify::windows_pnp_identity_from_probe;

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
        let pnp_identity = windows_pnp_identity_from_probe(probe)?;
        let device_id = pnp_identity.write_device_id;
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
pub struct Lba8Identity {
    pub glab: String,
    pub indus: String,
    pub orgcd: String,
    pub org: String,
    pub unit: String,
    pub alarm: String,
    pub autonum: String,
    pub rmark: String,
    pub vol0: String,
    pub vol1: String,
    pub vol2: String,
    pub volc0: String,
    pub volc1: String,
    pub volc2: String,
}

impl Default for Lba8Identity {
    fn default() -> Self {
        let profile = ProvisionProfile::canonical_v1();
        Self {
            glab: profile.glab().to_string(),
            indus: String::new(),
            orgcd: String::new(),
            org: String::new(),
            unit: String::new(),
            alarm: String::new(),
            autonum: profile.autonum().to_string(),
            rmark: String::new(),
            vol0: String::new(),
            vol1: String::new(),
            vol2: String::new(),
            volc0: String::new(),
            volc1: String::new(),
            volc2: String::new(),
        }
    }
}

impl Lba8Identity {
    pub fn validate(&self) -> Result<(), String> {
        validate_gbk_field("GLab", &self.glab, 128)?;
        validate_gbk_field("Autonum", &self.autonum, 15)?;
        for (name, value) in [
            ("Indus", self.indus.as_str()),
            ("Orgcd", self.orgcd.as_str()),
            ("Org", self.org.as_str()),
            ("Unit", self.unit.as_str()),
            ("Alarm", self.alarm.as_str()),
            ("Rmark", self.rmark.as_str()),
            ("VOL0", self.vol0.as_str()),
            ("VOL1", self.vol1.as_str()),
            ("VOL2", self.vol2.as_str()),
            ("VOLC0", self.volc0.as_str()),
            ("VOLC1", self.volc1.as_str()),
            ("VOLC2", self.volc2.as_str()),
        ] {
            validate_optional_gbk_field(name, value, 128)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionMetadata {
    onlyid: OnlyId,
    user: String,
    dept: String,
    label: String,
    lba8_identity: Lba8Identity,
}

const MAX_USER_GBK_BYTES: usize = 155;
const MAX_DEPT_GBK_BYTES: usize = 187;

fn encoded_gbk(name: &str, value: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    if value.chars().any(char::is_control) || value.contains('|') || value.contains('=') {
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
    Ok(encoded.into_owned())
}

fn validate_gbk_field(name: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    encoded_gbk(name, value, max_bytes).map(|_| ())
}

fn validate_optional_gbk_field(name: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    encoded_gbk(name, value, max_bytes).map(|_| ())
}

pub(crate) fn lba8_elabel_body(metadata: &ProvisionMetadata) -> Result<Vec<u8>, String> {
    let advanced = metadata.lba8_identity();
    let mut body = Vec::new();
    let mut append =
        |key: &[u8], name: &str, value: &str, max_bytes: usize| -> Result<(), String> {
            body.extend_from_slice(key);
            body.extend_from_slice(&encoded_gbk(name, value, max_bytes)?);
            Ok(())
        };

    append(b"<ELABEL>GLab=", "GLab", &advanced.glab, 128)?;
    append(b"||Indus=", "Indus", &advanced.indus, 128)?;
    append(b"||Orgcd=", "Orgcd", &advanced.orgcd, 128)?;
    append(b"||Org=", "Org", &advanced.org, 128)?;
    append(b"||Unit=", "Unit", &advanced.unit, 128)?;
    append(b"||Dept=", "Dept", metadata.dept(), MAX_DEPT_GBK_BYTES)?;
    append(b"||User=", "User", metadata.user(), MAX_USER_GBK_BYTES)?;
    append(b"||Alarm=", "Alarm", &advanced.alarm, 128)?;
    append(b"||Autonum=", "Autonum", &advanced.autonum, 15)?;
    append(b"||Label=", "Label", metadata.label(), 31)?;
    append(b"||Rmark=", "Rmark", &advanced.rmark, 128)?;
    append(b"||VOL0=", "VOL0", &advanced.vol0, 128)?;
    append(b"||VOL1=", "VOL1", &advanced.vol1, 128)?;
    append(b"||VOL2=", "VOL2", &advanced.vol2, 128)?;
    append(b"||VOLC0=", "VOLC0", &advanced.volc0, 128)?;
    append(b"||VOLC1=", "VOLC1", &advanced.volc1, 128)?;
    append(b"||VOLC2=", "VOLC2", &advanced.volc2, 128)?;
    body.extend_from_slice(b"||");
    Ok(body)
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
        // Current first-party SAFE6 writer stores User in a 156-byte source
        // array (155 bytes + NUL) and Dept in a 188-byte source array
        // (187 bytes + NUL). Values that exceed the inline LBA6 slots continue
        // in LBA9; rejecting them here would diverge from the official writer.
        validate_gbk_field("User", &user, MAX_USER_GBK_BYTES)?;
        validate_gbk_field("Dept", &dept, MAX_DEPT_GBK_BYTES)?;
        validate_gbk_field("Label", &label, 31)?;
        Ok(Self {
            onlyid,
            user,
            dept,
            label,
            lba8_identity: Lba8Identity::default(),
        })
    }

    pub fn with_lba8_identity(mut self, identity: Lba8Identity) -> Result<Self, String> {
        identity.validate()?;
        self.lba8_identity = identity;
        Ok(self)
    }

    pub fn lba8_identity(&self) -> &Lba8Identity {
        &self.lba8_identity
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
        metadata.lba8_identity().validate()?;
        let llgb_payload_len = lba8_elabel_body(&metadata)?.len();
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
