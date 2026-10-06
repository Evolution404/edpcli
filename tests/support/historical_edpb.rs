//! Historical EDPB fixtures belong to tests; production only writes the current schema.

use std::{fs, path::Path};

use edpcli::{
    application::media_identity::MediaIdentitySnapshot,
    edpb::{self, CoreCapture, Manifest, FOOTER_SIZE},
};
use sha2::{Digest, Sha256};

fn rewrite_manifest(path: &Path, manifest: &Manifest) -> Result<(), String> {
    let mut bytes = fs::read(path).map_err(|error| error.to_string())?;
    let offset = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
    let mut footer = bytes[bytes.len() - FOOTER_SIZE..].to_vec();
    let json = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    let hash = Sha256::digest(&json);
    let footer_offset = offset + json.len();
    let file_size = footer_offset + FOOTER_SIZE;
    bytes[24..32].copy_from_slice(&(json.len() as u64).to_le_bytes());
    bytes[32..40].copy_from_slice(&(footer_offset as u64).to_le_bytes());
    bytes[48..80].copy_from_slice(&hash);
    footer[24..32].copy_from_slice(&(json.len() as u64).to_le_bytes());
    footer[32..40].copy_from_slice(&(file_size as u64).to_le_bytes());
    footer[40..72].copy_from_slice(&hash);
    bytes.truncate(offset);
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&footer);
    fs::write(path, bytes).map_err(|error| error.to_string())
}

pub fn v1_core_with_notes(
    path: &Path,
    capture: &CoreCapture<'_>,
    notes: &[String],
) -> Result<Manifest, String> {
    let mut manifest = edpb::write_core_backup(path, capture)?;
    manifest.schema = "edpb.manifest.v1".into();
    manifest.identity = None;
    manifest.backup_purpose = None;
    manifest.restore_contract = None;
    manifest.provenance.notes.extend_from_slice(notes);
    rewrite_manifest(path, &manifest)?;
    Ok(manifest)
}

pub fn v2_core_with_identity(
    path: &Path,
    capture: &CoreCapture<'_>,
    snapshot: &MediaIdentitySnapshot,
) -> Result<Manifest, String> {
    let mut manifest = edpb::write_core_backup(path, capture)?;
    manifest.schema = "edpb.manifest.v2".into();
    manifest.backup_purpose = None;
    manifest.restore_contract = None;
    let mut identity = edpb::manifest_identity_from_snapshot(snapshot);
    identity.hardware.serial = None;
    identity.hardware.serial_sha256 = snapshot.hardware.serial_sha256.clone();
    manifest.identity = Some(identity);
    rewrite_manifest(path, &manifest)?;
    Ok(manifest)
}
