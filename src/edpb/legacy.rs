use super::*;

/// Explicit compatibility writer used only to construct/read historical v1 fixtures.
/// Normal backup creation must use the current v3 writers.
#[doc(hidden)]
pub fn write_legacy_v1_core_backup_with_notes(
    path: &Path,
    capture: &CoreCapture<'_>,
    notes: &[String],
) -> Result<Manifest, String> {
    write_container(
        path,
        capture,
        CaptureLevel::Core,
        &[],
        &[],
        &[],
        &[],
        notes,
        None,
        "edpb.manifest.v1",
    )
}

pub(super) const LEGACY_HARDWARE_SERIAL_NOTE_PREFIX: &str = "hardware_serial_sha256=";

pub(super) fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn legacy_hardware_serial_digest(manifest: &Manifest) -> Result<Option<String>, String> {
    let mut values = manifest
        .provenance
        .notes
        .iter()
        .filter_map(|note| note.strip_prefix(LEGACY_HARDWARE_SERIAL_NOTE_PREFIX));
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() || !valid_sha256_hex(value) {
        return Err("EDPB legacy hardware serial binding is malformed or duplicated".into());
    }
    Ok(Some(value.to_ascii_lowercase()))
}
