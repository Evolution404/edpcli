use std::io;

use encoding_rs::GBK;

use crate::common::SECTOR;
use crate::crypto::{a6b0_full, crc32_bare, lba6_checksum, lba6_decode, xor_rolling};
use crate::inspect::{analyze_sector, InspectMeta};
use crate::metainfo::{ownership_from_lba8, summarize};
use crate::sectors::looks_nopwd;

use super::{ProvisionImage, ProvisionSpec, PROVISION_IMAGE_LEN};

const SHARE_START: u64 = 63;
const TYPE4_SECTORS: u64 = 6;
const LBA12_TABLE_LEN: usize = 0x170;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionValidation {
    profile_id: String,
    device_id: String,
    onlyid: String,
    is_nopwd: bool,
}

impl ProvisionValidation {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn onlyid(&self) -> &str {
        &self.onlyid
    }

    pub fn is_nopwd(&self) -> bool {
        self.is_nopwd
    }
}

pub struct ProvisionValidator;

impl ProvisionValidator {
    pub fn validate(
        spec: &ProvisionSpec,
        image: &ProvisionImage,
    ) -> Result<ProvisionValidation, String> {
        Self::validate_bytes(spec, image.as_bytes())
    }

    pub fn validate_bytes(
        spec: &ProvisionSpec,
        bytes: &[u8],
    ) -> Result<ProvisionValidation, String> {
        if bytes.len() != PROVISION_IMAGE_LEN {
            return Err(format!(
                "provision image must be exactly {PROVISION_IMAGE_LEN} bytes, got {}",
                bytes.len()
            ));
        }
        validate_reserved(bytes)?;
        validate_mbr(spec, sector(bytes, 0))?;
        let inspect_meta = inspect_meta(spec);
        validate_lba4(spec, sector(bytes, 4), &inspect_meta)?;
        validate_lba6(spec, sector(bytes, 6))?;
        validate_lba7(spec, sector(bytes, 7))?;
        validate_lba8(spec, sector(bytes, 8), &inspect_meta)?;
        validate_lba11(spec, sector(bytes, 11), &inspect_meta)?;
        validate_lba12(spec, sector(bytes, 12))?;

        let snapshot = |lba: u32| -> crate::common::EdpCliResult<Vec<u8>> {
            Ok(sector(bytes, lba as usize).to_vec())
        };
        let is_nopwd = looks_nopwd(&snapshot, spec.target().device_id())
            .map_err(|err| format!("nopwd validation failed: {}", err.msg))?;
        if !is_nopwd {
            return Err("generated image does not satisfy existing nopwd detector".into());
        }

        let summary = summarize(&inspect_meta, |lba| {
            Ok::<Vec<u8>, io::Error>(sector(bytes, lba as usize).to_vec())
        })
        .map_err(|err| format!("metainfo validation failed: {err}"))?;
        if summary.onlyid.as_deref() != Some(spec.metadata().onlyid().text()) {
            return Err("metainfo onlyid does not match ProvisionSpec".into());
        }
        if summary.pdkb_device_id.as_deref() != Some(spec.target().device_id()) {
            return Err("metainfo PDKB device_id does not match target device_id".into());
        }
        if summary.is_nopwd != Some(true) {
            return Err("metainfo does not report nopwd=true".into());
        }

        Ok(ProvisionValidation {
            profile_id: spec.profile().id().to_string(),
            device_id: spec.target().device_id().to_string(),
            onlyid: spec.metadata().onlyid().text().to_string(),
            is_nopwd,
        })
    }
}

fn sector(bytes: &[u8], lba: usize) -> &[u8] {
    &bytes[lba * SECTOR..(lba + 1) * SECTOR]
}

fn inspect_meta(spec: &ProvisionSpec) -> InspectMeta {
    InspectMeta {
        device_id: Some(spec.target().device_id().to_string()),
        vid: Some(spec.target().vid_hex()),
        pid: Some(spec.target().pid_hex()),
        size_bytes: Some(spec.target().total_sectors() * SECTOR as u64),
        onlyid: Some(spec.metadata().onlyid().text().to_string()),
    }
}

fn validate_reserved(bytes: &[u8]) -> Result<(), String> {
    for lba in [1usize, 2, 3, 5, 9, 10] {
        if sector(bytes, lba).iter().any(|byte| *byte != 0) {
            return Err(format!("LBA{lba} violates canonical zero-sector policy"));
        }
    }
    Ok(())
}

fn layout(spec: &ProvisionSpec) -> Result<(u64, u64), String> {
    let type4_start = spec
        .target()
        .total_sectors()
        .checked_sub(TYPE4_SECTORS)
        .ok_or("target is too small for type4 reserve")?;
    let share_sectors = type4_start
        .checked_sub(SHARE_START)
        .ok_or("target is too small for Share@63")?;
    Ok((share_sectors, type4_start))
}

fn validate_mbr(spec: &ProvisionSpec, raw: &[u8]) -> Result<(), String> {
    let (share_sectors, _) = layout(spec)?;
    let mut expected = [0u8; SECTOR];
    expected[0x1be + 4] = 0x07;
    expected[0x1be + 8..0x1be + 12].copy_from_slice(&(SHARE_START as u32).to_le_bytes());
    let share_u32 =
        u32::try_from(share_sectors).map_err(|_| "MBR Share sector count overflows u32")?;
    expected[0x1be + 12..0x1be + 16].copy_from_slice(&share_u32.to_le_bytes());
    expected[0x1fe..0x200].copy_from_slice(&[0x55, 0xaa]);
    if raw != expected {
        return Err("MBR does not match canonical provision layout".into());
    }
    Ok(())
}

fn validate_lba4(spec: &ProvisionSpec, raw: &[u8], meta: &InspectMeta) -> Result<(), String> {
    let view = analyze_sector(4, raw, meta);
    let tag = ["$$$", spec.metadata().onlyid().text(), "$$$"].concat();
    if raw.get(..tag.len()) != Some(tag.as_bytes()) {
        return Err("LBA4 onlyid clear tag mismatch".into());
    }
    if !view
        .method
        .contains(&format!("labelOnlyId={}", spec.metadata().onlyid().text()))
    {
        return Err("LBA4 onlyid decoder round-trip failed".into());
    }
    if view.decoded.get(0x39..0x3d) != Some(b"LLGB") {
        return Err("LBA4 LLGB magic mismatch".into());
    }
    if view.decoded.get(0x35..0x39) != Some(&spec.profile().lba4_profile_word()) {
        return Err("LBA4 canonical profile word mismatch".into());
    }
    if view.decoded.get(0x1fc..0x200) != Some(b"LLGB") {
        return Err("LBA4 trailing LLGB marker mismatch".into());
    }
    Ok(())
}

fn gbk(value: &str) -> Result<Vec<u8>, String> {
    let (bytes, _, errors) = GBK.encode(value);
    if errors {
        return Err(format!("GBK encoding failed for {value:?}"));
    }
    Ok(bytes.into_owned())
}

fn expected_lba6_plain(spec: &ProvisionSpec) -> Result<[u8; SECTOR], String> {
    let mut expected = spec.profile().safe6_template();
    let dept = gbk(spec.metadata().dept())?;
    let user = gbk(spec.metadata().user())?;
    if !(4..=6).contains(&user.len()) {
        return Err("User does not fit canonical v1 SAFE6 profile".into());
    }
    expected[..0x40].fill(0);
    expected[..dept.len()].copy_from_slice(&dept);
    expected[dept.len()] = 0;
    expected[0x55..0x58].copy_from_slice(&[0x73, 0x2a, 0xfe]);
    expected[0x50..0x50 + user.len()].copy_from_slice(&user);
    expected[0x50 + user.len()] = 0;
    expected[0x70..0x78].copy_from_slice(spec.profile().autonum().as_bytes());
    let crc = crc32_bare(spec.target().device_id().as_bytes());
    expected[0x100..0x104].copy_from_slice(&crc.to_le_bytes());
    expected[0x104..0x108].copy_from_slice(&crc.wrapping_shl(1).to_le_bytes());
    let gserial = spec.profile().safe6_gserial().as_bytes();
    let beizhu = spec.profile().safe6_beizhu().as_bytes();
    if gserial.len() > 15 || beizhu.len() > 15 {
        return Err("SAFE6 GSerial/BeiZhu exceeds 15-byte on-disk slot".into());
    }
    expected[0x1c0..0x1d0].fill(0);
    expected[0x1c0..0x1c0 + gserial.len()].copy_from_slice(gserial);
    expected[0x1d0..0x1e0].fill(0);
    expected[0x1d0..0x1d0 + beizhu.len()].copy_from_slice(beizhu);
    expected[0x1e0..0x1f0].fill(0);
    expected[0x1f0..0x1f4]
        .copy_from_slice(&u32::from(spec.profile().safe6_encrypt()).to_le_bytes());
    Ok(expected)
}

fn validate_lba6(spec: &ProvisionSpec, raw: &[u8]) -> Result<(), String> {
    let decoded = lba6_decode(raw);
    let expected = expected_lba6_plain(spec)?;
    if decoded[..0x1fc] != expected[..0x1fc] {
        return Err("LBA6 SAFE6 plaintext/profile mismatch".into());
    }
    let checksum = lba6_checksum(&raw[..0x1fc]);
    if raw[0x1fc..0x200] != checksum.to_le_bytes() {
        return Err("LBA6 checksum mismatch".into());
    }
    Ok(())
}

fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn expected_entry(
    stride: usize,
    ptype: u32,
    start: u64,
    size_bytes: u64,
    material: &[u8],
) -> Vec<u8> {
    let mut entry = vec![0u8; stride];
    entry[..4].copy_from_slice(b"EDPF");
    put_u32(&mut entry, 0x08, 2);
    put_u32(&mut entry, 0x0c, ptype);
    put_u32(&mut entry, 0x10, 1);
    put_u32(&mut entry, 0x14, 1);
    put_u64(&mut entry, 0x18, start);
    put_u64(&mut entry, 0x20, SECTOR as u64);
    put_u64(&mut entry, 0x28, size_bytes);
    entry[0x30..0x30 + material.len()].copy_from_slice(material);
    if stride == 0x60 {
        put_u64(&mut entry, 0x58, 2);
    }
    entry
}

fn validate_lba7(spec: &ProvisionSpec, raw: &[u8]) -> Result<(), String> {
    let (share_sectors, type4_start) = layout(spec)?;
    let crc = crc32_bare(spec.target().device_id().as_bytes());
    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let decoded = xor_rolling(raw, k0);
    let mut expected = [0u8; SECTOR];
    let share = expected_entry(
        0x40,
        2,
        SHARE_START,
        share_sectors * SECTOR as u64,
        spec.profile().lba7_material(),
    );
    let type4 = expected_entry(
        0x40,
        4,
        type4_start,
        TYPE4_SECTORS * SECTOR as u64,
        spec.profile().lba7_material(),
    );
    expected[..0x40].copy_from_slice(&share);
    expected[0x40..0x80].copy_from_slice(&type4);
    expected[0xc0..0xc8].copy_from_slice(spec.profile().lba7_terminator());
    if decoded != expected {
        return Err("LBA7 EDPF/profile mismatch for target device_id".into());
    }
    Ok(())
}

fn expected_lba8_plain(spec: &ProvisionSpec) -> Result<[u8; SECTOR], String> {
    let mut expected = [0u8; SECTOR];
    expected[..4].copy_from_slice(b"LLGB");
    expected[0x08..0x10].copy_from_slice(&[0x01, 0x00, 0x00, 0x01, 0x22, 0x02, 0x00, 0x00]);
    expected[0x10..0x14].copy_from_slice(&[0x00, 0x92, 0x53, 0x6a]);
    expected[0x14..0x18].copy_from_slice(&spec.profile().lba4_profile_word());
    expected[0x3e..0x40].copy_from_slice(&[0x80, 0x00]);

    let mut body = Vec::new();
    body.extend_from_slice(b"<ELABEL>GLab=");
    body.extend_from_slice(spec.profile().glab().as_bytes());
    body.extend_from_slice(b"||Indus=||Orgcd=||Org=||Unit=||Dept=");
    body.extend_from_slice(&gbk(spec.metadata().dept())?);
    body.extend_from_slice(b"||User=");
    body.extend_from_slice(&gbk(spec.metadata().user())?);
    body.extend_from_slice(b"||Alarm=||Autonum=");
    body.extend_from_slice(spec.profile().autonum().as_bytes());
    body.extend_from_slice(b"||Label=");
    body.extend_from_slice(&gbk(spec.metadata().label())?);
    body.extend_from_slice(b"||Rmark=||VOL0=||VOL1=||VOL2=||VOLC0=||VOLC1=||VOLC2=||");
    if body.len() > SECTOR - 0x80 - 1 {
        return Err(format!("LBA8 LLGB body too large: {} bytes", body.len()));
    }
    let logical_end = 0x80 + body.len();
    put_u32(&mut expected, 0x04, logical_end as u32);
    expected[0x80..logical_end].copy_from_slice(&body);
    expected[logical_end] = 0;
    Ok(expected)
}

fn validate_lba8(spec: &ProvisionSpec, raw: &[u8], meta: &InspectMeta) -> Result<(), String> {
    let crc = crc32_bare(spec.target().device_id().as_bytes());
    let expected = expected_lba8_plain(spec)?;
    let logical_len = u32::from_le_bytes(expected[4..8].try_into().unwrap()) as usize;
    let encrypted_len = (logical_len / 16 + 1) * 16;
    let mut decoded = [0u8; SECTOR];
    let head = a6b0_full(&raw[..encrypted_len], &crc.to_le_bytes(), 0);
    decoded[..encrypted_len].copy_from_slice(&head);
    if decoded != expected {
        return Err("LBA8 LLGB/profile bytes mismatch".into());
    }
    let ownership = ownership_from_lba8(raw, meta).ok_or("LBA8 ownership decoder failed")?;
    if ownership.glab.as_deref() != Some(spec.profile().glab())
        || ownership.user.as_deref() != Some(spec.metadata().user())
        || ownership.dept.as_deref() != Some(spec.metadata().dept())
        || ownership.label.as_deref() != Some(spec.metadata().label())
        || ownership.autonum.as_deref() != Some(spec.profile().autonum())
    {
        return Err("LBA8 ownership round-trip mismatch".into());
    }
    Ok(())
}

fn validate_lba11(spec: &ProvisionSpec, raw: &[u8], meta: &InspectMeta) -> Result<(), String> {
    let view = analyze_sector(11, raw, meta);
    if view.decoded.get(0x100..0x104) != Some(b"PDKB") {
        return Err("LBA11 PDKB decrypt failed for target VID/PID/capacity".into());
    }
    let expected = spec.target().device_id();
    let ok = view
        .fields
        .iter()
        .any(|field| field.label == "PDKB device_id" && field.value == expected);
    if !ok {
        return Err("LBA11 PDKB device_id mismatch".into());
    }
    Ok(())
}

fn validate_lba12(spec: &ProvisionSpec, raw: &[u8]) -> Result<(), String> {
    let (share_sectors, type4_start) = layout(spec)?;
    let crc = crc32_bare(spec.target().device_id().as_bytes());
    let key = crc.to_le_bytes();
    let decoded = a6b0_full(raw, &key, 0);
    let mut expected = [0u8; SECTOR];
    let share = expected_entry(
        0x60,
        2,
        SHARE_START,
        share_sectors * SECTOR as u64,
        spec.profile().lba12_material(),
    );
    let type4 = expected_entry(
        0x60,
        4,
        type4_start,
        TYPE4_SECTORS * SECTOR as u64,
        spec.profile().lba12_material(),
    );
    expected[..0x60].copy_from_slice(&share);
    expected[0x60..0xc0].copy_from_slice(&type4);
    expected[0x120..0x128].copy_from_slice(spec.profile().lba12_terminator());
    if decoded[..LBA12_TABLE_LEN] != expected[..LBA12_TABLE_LEN] {
        return Err("LBA12 EDPF/profile mismatch for target device_id".into());
    }
    if decoded[LBA12_TABLE_LEN..].iter().any(|byte| *byte != 0) {
        return Err("LBA12 decoded tail is not canonical zero plaintext".into());
    }
    Ok(())
}
