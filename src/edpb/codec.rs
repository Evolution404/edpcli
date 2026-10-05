use super::*;

pub(super) fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub(super) use crate::common::hex_lower as hex;

pub(super) fn put_u16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn put_u64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn get_u16(src: &[u8], offset: usize) -> Result<u16, String> {
    src.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| "EDPB structure truncated at u16".to_string())
}

pub(super) fn get_u32(src: &[u8], offset: usize) -> Result<u32, String> {
    src.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| "EDPB structure truncated at u32".to_string())
}

pub(super) fn get_u64(src: &[u8], offset: usize) -> Result<u64, String> {
    src.get(offset..offset + 8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| "EDPB structure truncated at u64".to_string())
}

pub(super) fn make_header(
    created_epoch: i64,
    manifest_offset: u64,
    manifest_len: u64,
    footer_offset: u64,
    manifest_sha: &[u8; 32],
) -> [u8; HEADER_SIZE] {
    let mut out = [0u8; HEADER_SIZE];
    out[..8].copy_from_slice(FILE_MAGIC);
    put_u16(&mut out, 8, FORMAT_MAJOR);
    put_u16(&mut out, 10, FORMAT_MINOR);
    put_u32(&mut out, 12, HEADER_SIZE as u32);
    put_u64(&mut out, 16, manifest_offset);
    put_u64(&mut out, 24, manifest_len);
    put_u64(&mut out, 32, footer_offset);
    put_u64(&mut out, 40, created_epoch.max(0) as u64);
    out[48..80].copy_from_slice(manifest_sha);
    out
}

pub(super) fn make_chunk_header(data: &[u8]) -> [u8; CHUNK_HEADER_SIZE] {
    let mut out = [0u8; CHUNK_HEADER_SIZE];
    out[..8].copy_from_slice(CHUNK_MAGIC);
    put_u16(&mut out, 8, 1);
    put_u16(&mut out, 10, 0);
    put_u64(&mut out, 16, data.len() as u64);
    put_u64(&mut out, 24, data.len() as u64);
    out[32..64].copy_from_slice(&sha256_bytes(data));
    out
}

pub(super) fn make_footer(
    manifest_offset: u64,
    manifest_len: u64,
    file_size: u64,
    manifest_sha: &[u8; 32],
) -> [u8; FOOTER_SIZE] {
    let mut out = [0u8; FOOTER_SIZE];
    out[..8].copy_from_slice(FOOTER_MAGIC);
    put_u16(&mut out, 8, FORMAT_MAJOR);
    put_u16(&mut out, 10, FORMAT_MINOR);
    put_u64(&mut out, 16, manifest_offset);
    put_u64(&mut out, 24, manifest_len);
    put_u64(&mut out, 32, file_size);
    out[40..72].copy_from_slice(manifest_sha);
    out
}
