use encoding_rs::GBK;

use crate::common::SECTOR;
use crate::crypto::{a7f0_full, crc32_bare, lba6_checksum, xor_rolling, LBA6_K0};

use super::{ProvisionImage, ProvisionSpec, PROVISION_IMAGE_LEN};

const LBA12_TABLE_LEN: usize = 0x170;
const SHARE_START: u64 = 63;
const TYPE4_SECTORS: u64 = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionEntropy {
    lba4_nonce: [u8; 8],
    pdkb_random: [u8; 256],
}

impl ProvisionEntropy {
    pub fn new(lba4_nonce: [u8; 8], pdkb_random: [u8; 256]) -> Self {
        Self {
            lba4_nonce,
            pdkb_random,
        }
    }

    pub fn lba4_nonce(&self) -> &[u8; 8] {
        &self.lba4_nonce
    }

    pub fn pdkb_random(&self) -> &[u8; 256] {
        &self.pdkb_random
    }
}

#[derive(Clone, Copy, Debug)]
struct Layout {
    share_sectors: u64,
    type4_start: u64,
}

fn layout(spec: &ProvisionSpec) -> Result<Layout, String> {
    let total = spec.target().total_sectors();
    let type4_start = total
        .checked_sub(TYPE4_SECTORS)
        .ok_or("target is too small for type4 reserve")?;
    let share_sectors = type4_start
        .checked_sub(SHARE_START)
        .ok_or("target is too small for Share@63")?;
    if share_sectors > u32::MAX as u64 {
        return Err("target exceeds MBR 32-bit sector-count capacity".into());
    }
    Ok(Layout {
        share_sectors,
        type4_start,
    })
}

fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn gbk(value: &str) -> Result<Vec<u8>, String> {
    let (bytes, _, errors) = GBK.encode(value);
    if errors {
        return Err(format!(
            "text is not losslessly representable in GBK: {value:?}"
        ));
    }
    Ok(bytes.into_owned())
}

fn build_lba0(layout: Layout) -> [u8; SECTOR] {
    let mut out = [0u8; SECTOR];
    let entry = 0x1be;
    out[entry + 4] = 0x07;
    put_u32(&mut out, entry + 8, SHARE_START as u32);
    put_u32(&mut out, entry + 12, layout.share_sectors as u32);
    out[0x1fe..0x200].copy_from_slice(&[0x55, 0xaa]);
    out
}

fn sparse_rolling_encrypt(plain: &[u8], k0: u32) -> Vec<u8> {
    let mut encrypted = xor_rolling(plain, k0);
    for (index, byte) in plain.iter().enumerate() {
        if *byte == 0 {
            encrypted[index] = 0;
        }
    }
    encrypted
}

fn build_lba4(spec: &ProvisionSpec, entropy: &ProvisionEntropy) -> Result<[u8; SECTOR], String> {
    let mut plain = [0u8; SECTOR];
    let tag = ["$$$", spec.metadata().onlyid().text(), "$$$"].concat();
    if tag.len() > 0x18 {
        return Err("onlyid tag does not fit LBA4 clear header".into());
    }
    plain[..tag.len()].copy_from_slice(tag.as_bytes());
    plain[0x18..0x20].copy_from_slice(entropy.lba4_nonce());
    plain[0x20..0x24].copy_from_slice(&[0x29, 0x1d, 0x00, 0x00]);
    plain[0x24..0x28].copy_from_slice(&0x7bu32.to_le_bytes());
    plain[0x28..0x2c].copy_from_slice(&0x4ddu32.to_le_bytes());
    plain[0x2c..0x30].copy_from_slice(&0x79u32.to_le_bytes());
    plain[0x30..0x34].copy_from_slice(&0x7cu32.to_le_bytes());
    plain[0x35..0x39].copy_from_slice(&spec.profile().lba4_profile_word());
    plain[0x39..0x3d].copy_from_slice(b"LLGB");
    plain[0x3d] = 1;
    plain[0x41..0x45].copy_from_slice(&[0x08, 0x04, 0x0c, 0x01]);
    plain[0x1fc..0x200].copy_from_slice(b"LLGB");

    let bits = spec.metadata().onlyid().bits();
    let k0 = (bits & 0xffff) ^ (bits >> 16);
    let encrypted = sparse_rolling_encrypt(&plain[0x18..], k0);
    let mut out = plain;
    out[0x18..].copy_from_slice(&encrypted);
    Ok(out)
}

fn build_lba6(spec: &ProvisionSpec) -> Result<[u8; SECTOR], String> {
    let mut plain = spec.profile().safe6_template();
    let dept = gbk(spec.metadata().dept())?;
    let user = gbk(spec.metadata().user())?;
    if dept.len() > 63 {
        return Err("Dept exceeds LBA6 63-byte field".into());
    }
    if !(4..=6).contains(&user.len()) {
        return Err("canonical v1 SAFE6 profile requires User to encode to 4..=6 GBK bytes".into());
    }
    plain[..0x40].fill(0);
    plain[..dept.len()].copy_from_slice(&dept);
    plain[dept.len()] = 0;

    plain[0x55..0x58].copy_from_slice(&[0x73, 0x2a, 0xfe]);
    plain[0x50..0x50 + user.len()].copy_from_slice(&user);
    plain[0x50 + user.len()] = 0;

    let serial = spec.profile().autonum().as_bytes();
    if serial.len() != 8 {
        return Err("canonical autonum must be exactly 8 ASCII bytes".into());
    }
    plain[0x70..0x78].copy_from_slice(serial);

    let crc = crc32_bare(spec.target().device_id().as_bytes());
    put_u32(&mut plain, 0x100, crc);
    put_u32(&mut plain, 0x104, crc.wrapping_shl(1));
    let gserial = spec.profile().safe6_gserial().as_bytes();
    let beizhu = spec.profile().safe6_beizhu().as_bytes();
    if gserial.len() > 15 || beizhu.len() > 15 {
        return Err("SAFE6 GSerial/BeiZhu exceeds 15-byte on-disk slot".into());
    }
    plain[0x1c0..0x1d0].fill(0);
    plain[0x1c0..0x1c0 + gserial.len()].copy_from_slice(gserial);
    plain[0x1d0..0x1e0].fill(0);
    plain[0x1d0..0x1d0 + beizhu.len()].copy_from_slice(beizhu);
    // The current Windows/Linux writer leaves this template extension region
    // untouched. Its canonical UsbMainBSec bytes are zero.
    plain[0x1e0..0x1f0].fill(0);
    put_u32(&mut plain, 0x1f0, u32::from(spec.profile().safe6_encrypt()));

    let encrypted = xor_rolling(&plain[..0x1fc], LBA6_K0);
    let checksum = lba6_checksum(&encrypted);
    let mut out = [0u8; SECTOR];
    out[..0x1fc].copy_from_slice(&encrypted);
    out[0x1fc..].copy_from_slice(&checksum.to_le_bytes());
    Ok(out)
}

fn edpf_entry(stride: usize, ptype: u32, start: u64, size_bytes: u64, material: &[u8]) -> Vec<u8> {
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

fn build_lba7(spec: &ProvisionSpec, layout: Layout) -> [u8; SECTOR] {
    let mut plain = [0u8; SECTOR];
    let share = edpf_entry(
        0x40,
        2,
        SHARE_START,
        layout.share_sectors * SECTOR as u64,
        spec.profile().lba7_material(),
    );
    let type4 = edpf_entry(
        0x40,
        4,
        layout.type4_start,
        TYPE4_SECTORS * SECTOR as u64,
        spec.profile().lba7_material(),
    );
    plain[..0x40].copy_from_slice(&share);
    plain[0x40..0x80].copy_from_slice(&type4);
    plain[0xc0..0xc8].copy_from_slice(spec.profile().lba7_terminator());
    let crc = crc32_bare(spec.target().device_id().as_bytes());
    let k0 = (crc & 0xffff) ^ (crc >> 16);
    xor_rolling(&plain, k0).try_into().expect("sector length")
}

fn build_lba8(spec: &ProvisionSpec) -> Result<[u8; SECTOR], String> {
    let mut plain = [0u8; SECTOR];
    plain[..4].copy_from_slice(b"LLGB");
    plain[0x08..0x10].copy_from_slice(&[0x01, 0x00, 0x00, 0x01, 0x22, 0x02, 0x00, 0x00]);
    plain[0x10..0x14].copy_from_slice(&[0x00, 0x92, 0x53, 0x6a]);
    plain[0x14..0x18].copy_from_slice(&spec.profile().lba4_profile_word());
    plain[0x3e..0x40].copy_from_slice(&[0x80, 0x00]);

    let dept = gbk(spec.metadata().dept())?;
    let user = gbk(spec.metadata().user())?;
    let label = gbk(spec.metadata().label())?;
    let mut body = Vec::new();
    body.extend_from_slice(b"<ELABEL>GLab=");
    body.extend_from_slice(spec.profile().glab().as_bytes());
    body.extend_from_slice(b"||Indus=||Orgcd=||Org=||Unit=||Dept=");
    body.extend_from_slice(&dept);
    body.extend_from_slice(b"||User=");
    body.extend_from_slice(&user);
    body.extend_from_slice(b"||Alarm=||Autonum=");
    body.extend_from_slice(spec.profile().autonum().as_bytes());
    body.extend_from_slice(b"||Label=");
    body.extend_from_slice(&label);
    body.extend_from_slice(b"||Rmark=||VOL0=||VOL1=||VOL2=||VOLC0=||VOLC1=||VOLC2=||");
    if body.len() > SECTOR - 0x80 - 1 {
        return Err(format!("LBA8 LLGB body too large: {} bytes", body.len()));
    }
    let logical_end = 0x80 + body.len();
    put_u32(&mut plain, 0x04, logical_end as u32);
    plain[0x80..logical_end].copy_from_slice(&body);
    plain[logical_end] = 0;
    let encrypted_len = (logical_end / 16 + 1) * 16;

    let crc = crc32_bare(spec.target().device_id().as_bytes());
    let encrypted = a7f0_full(&plain[..encrypted_len], &crc.to_le_bytes(), 0);
    let mut out = [0u8; SECTOR];
    out[..encrypted_len].copy_from_slice(&encrypted);
    Ok(out)
}

fn build_lba11(spec: &ProvisionSpec, entropy: &ProvisionEntropy) -> Result<[u8; SECTOR], String> {
    let device_id = spec.target().device_id().as_bytes();
    if device_id.len() + 5 > 256 {
        return Err("device_id does not fit PDKB plaintext".into());
    }
    let mut key_input = Vec::with_capacity(272);
    key_input.extend_from_slice(entropy.pdkb_random());
    key_input.extend_from_slice(spec.target().vid_hex().as_bytes());
    key_input.extend_from_slice(spec.target().pid_hex().as_bytes());
    key_input.extend_from_slice(&(spec.target().total_sectors() * SECTOR as u64).to_le_bytes());
    let key = crc32_bare(&key_input).to_le_bytes();

    let mut plain = [0u8; 256];
    plain[..4].copy_from_slice(b"PDKB");
    plain[4..4 + device_id.len()].copy_from_slice(device_id);
    let encrypted = a7f0_full(&plain, &key, 0);
    let mut out = [0u8; SECTOR];
    out[..256].copy_from_slice(entropy.pdkb_random());
    out[256..].copy_from_slice(&encrypted);
    Ok(out)
}

fn build_lba12(spec: &ProvisionSpec, layout: Layout) -> [u8; SECTOR] {
    let mut plain = [0u8; SECTOR];
    let share = edpf_entry(
        0x60,
        2,
        SHARE_START,
        layout.share_sectors * SECTOR as u64,
        spec.profile().lba12_material(),
    );
    let type4 = edpf_entry(
        0x60,
        4,
        layout.type4_start,
        TYPE4_SECTORS * SECTOR as u64,
        spec.profile().lba12_material(),
    );
    plain[..0x60].copy_from_slice(&share);
    plain[0x60..0xc0].copy_from_slice(&type4);
    plain[0x120..0x128].copy_from_slice(spec.profile().lba12_terminator());

    let crc = crc32_bare(spec.target().device_id().as_bytes());
    let key = crc.to_le_bytes();
    debug_assert!(plain[LBA12_TABLE_LEN..].iter().all(|byte| *byte == 0));
    a7f0_full(&plain, &key, 0)
        .try_into()
        .expect("LBA12 sector length")
}

pub fn generate_image(
    spec: &ProvisionSpec,
    entropy: &ProvisionEntropy,
) -> Result<ProvisionImage, String> {
    let layout = layout(spec)?;
    let mut image = vec![0u8; PROVISION_IMAGE_LEN];
    let sectors = [
        (0usize, build_lba0(layout).to_vec()),
        (4, build_lba4(spec, entropy)?.to_vec()),
        (6, build_lba6(spec)?.to_vec()),
        (7, build_lba7(spec, layout).to_vec()),
        (8, build_lba8(spec)?.to_vec()),
        (11, build_lba11(spec, entropy)?.to_vec()),
        (12, build_lba12(spec, layout).to_vec()),
    ];
    for (lba, data) in sectors {
        image[lba * SECTOR..(lba + 1) * SECTOR].copy_from_slice(&data);
    }
    ProvisionImage::from_bytes(image)
}
