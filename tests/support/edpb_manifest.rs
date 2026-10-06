use std::{fs, path::Path};

use sha2::{Digest, Sha256};

/// Change only a verified fixture's manifest, retaining every chunk and updating
/// both manifest hashes so rejection tests reach schema/identity validation.
pub fn mutate(path: &Path, change: impl FnOnce(&mut serde_json::Value)) {
    let original = fs::read(path).unwrap();
    let offset = u64::from_le_bytes(original[16..24].try_into().unwrap()) as usize;
    let length = u64::from_le_bytes(original[24..32].try_into().unwrap()) as usize;
    let mut manifest = serde_json::from_slice(&original[offset..offset + length]).unwrap();
    change(&mut manifest);
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let footer_offset = offset + bytes.len();
    let mut output = original[..offset].to_vec();
    let mut footer = original[original.len() - 80..].to_vec();
    for part in [&mut output, &mut footer] {
        part[24..32].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
    }
    output[32..40].copy_from_slice(&(footer_offset as u64).to_le_bytes());
    footer[32..40].copy_from_slice(&((footer_offset + 80) as u64).to_le_bytes());
    let hash = Sha256::digest(&bytes);
    output[48..80].copy_from_slice(&hash);
    footer[40..72].copy_from_slice(&hash);
    output.extend_from_slice(&bytes);
    output.extend_from_slice(&footer);
    fs::write(path, output).unwrap();
}
