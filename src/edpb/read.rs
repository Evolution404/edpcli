use super::*;

fn read_exact_at(
    file: &mut File,
    offset: u64,
    len: usize,
    control: Option<&crate::ports::ReadControl>,
) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("EDPB seek failed: {e}"))?;
    let mut out = Vec::new();
    out.try_reserve_exact(len)
        .map_err(|error| format!("EDPB allocation failed: {error}"))?;
    out.resize(len, 0);
    crate::bounded_read::ControlledRead {
        reader: file,
        control,
    }
    .read_exact(&mut out)
    .map_err(|e| format!("EDPB read failed: {e}"))?;
    Ok(out)
}

/// An immutable, bounded snapshot of validated artifacts and their manifest.
/// All consumers use the bytes verified through one open file handle.
#[derive(Debug)]
pub struct VerifiedBackupReader {
    verified: VerifiedContainer,
    artifacts: std::collections::BTreeMap<String, Vec<u8>>,
}

impl VerifiedBackupReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        if path.extension().and_then(|value| value.to_str()) != Some(EXTENSION) {
            return Err(format!("not an .{EXTENSION} backup: {}", path.display()));
        }
        let mut file = File::open(path).map_err(|error| format!("open EDPB failed: {error}"))?;
        verify_snapshot(&mut file, None)
    }

    pub fn open_controlled(
        path: &Path,
        control: &crate::ports::ReadControl,
    ) -> Result<Self, String> {
        control.check().map_err(|error| error.to_string())?;
        if path.extension().and_then(|value| value.to_str()) != Some(EXTENSION) {
            return Err("not an .edpb backup".into());
        }
        let mut file = File::open(path).map_err(|error| format!("open EDPB failed: {error}"))?;
        verify_snapshot(&mut file, Some(control))
    }

    pub fn verified(&self) -> &VerifiedContainer {
        &self.verified
    }

    pub fn read_artifact(&self, artifact_id: &str) -> Result<&[u8], String> {
        self.artifacts
            .get(artifact_id)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("EDPB does not contain Artifact {artifact_id}"))
    }

    pub fn read_raw_protocol(&self) -> Result<&[u8], String> {
        self.read_artifact(RAW_PROTOCOL_ARTIFACT_ID)
    }
}

pub fn verify_file(path: &Path) -> Result<VerifiedContainer, String> {
    Ok(VerifiedBackupReader::open(path)?.verified)
}

fn verify_snapshot(
    file: &mut File,
    control: Option<&crate::ports::ReadControl>,
) -> Result<VerifiedBackupReader, String> {
    let initial_metadata = file
        .metadata()
        .map_err(|error| format!("read EDPB metadata failed: {error}"))?;
    let file_len = initial_metadata.len();
    if file_len > super::limits::MAX_CONTAINER_BYTES {
        return Err("EDPB container exceeds read budget".into());
    }
    if file_len < (HEADER_SIZE + FOOTER_SIZE) as u64 {
        return Err("EDPB file too short".into());
    }
    let header = read_exact_at(file, 0, HEADER_SIZE, control)?;
    if header.get(..8) != Some(FILE_MAGIC.as_slice()) {
        return Err("EDPB magic mismatch".into());
    }
    let major = get_u16(&header, 8)?;
    let minor = get_u16(&header, 10)?;
    if major != FORMAT_MAJOR {
        return Err(format!("unsupported EDPB major version {major}"));
    }
    if get_u32(&header, 12)? as usize != HEADER_SIZE {
        return Err("EDPB header size mismatch".into());
    }
    let manifest_offset = get_u64(&header, 16)?;
    let manifest_len = get_u64(&header, 24)?;
    let footer_offset = get_u64(&header, 32)?;
    let header_manifest_sha: [u8; 32] = header[48..80]
        .try_into()
        .map_err(|_| "EDPB manifest hash truncated".to_string())?;
    if footer_offset
        .checked_add(FOOTER_SIZE as u64)
        .filter(|end| *end == file_len)
        .is_none()
    {
        return Err("EDPB footer offset or file length mismatch".into());
    }
    let footer = read_exact_at(file, footer_offset, FOOTER_SIZE, control)?;
    if footer.get(..8) != Some(FOOTER_MAGIC.as_slice()) {
        return Err("EDPB footer magic mismatch".into());
    }
    if get_u16(&footer, 8)? != major || get_u16(&footer, 10)? != minor {
        return Err("EDPB header/footer version mismatch".into());
    }
    if get_u64(&footer, 16)? != manifest_offset
        || get_u64(&footer, 24)? != manifest_len
        || get_u64(&footer, 32)? != file_len
    {
        return Err("EDPB header/footer locator mismatch".into());
    }
    let footer_manifest_sha: [u8; 32] = footer[40..72]
        .try_into()
        .map_err(|_| "EDPB footer manifest hash truncated".to_string())?;
    if footer_manifest_sha != header_manifest_sha {
        return Err("EDPB header/footer manifest hash mismatch".into());
    }
    let manifest_end = manifest_offset
        .checked_add(manifest_len)
        .ok_or_else(|| "EDPB manifest range overflow".to_string())?;
    if manifest_offset < HEADER_SIZE as u64 || manifest_end != footer_offset {
        return Err("EDPB manifest range is invalid".into());
    }
    let manifest_size =
        super::limits::bounded_len(manifest_len, super::limits::MAX_MANIFEST_BYTES, "manifest")?;
    let manifest_bytes = read_exact_at(file, manifest_offset, manifest_size, control)?;
    if sha256_bytes(&manifest_bytes) != header_manifest_sha {
        return Err("EDPB manifest SHA-256 mismatch".into());
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("parse EDPB manifest failed: {e}"))?;
    if manifest.artifacts.len() > super::limits::MAX_ARTIFACTS {
        return Err("EDPB artifact count exceeds read budget".into());
    }
    validate_manifest_graph(&manifest)?;
    let mut artifacts = std::collections::BTreeMap::new();
    let mut payload_bytes = 0u64;

    let mut chunk_ranges = Vec::with_capacity(manifest.artifacts.len());
    for artifact in &manifest.artifacts {
        let storage = &artifact.storage;
        if storage.codec != "none" {
            return Err(format!(
                "Artifact {} uses unsupported codec {}",
                artifact.id, storage.codec
            ));
        }
        if storage.frame_offset.checked_add(CHUNK_HEADER_SIZE as u64) != Some(storage.data_offset) {
            return Err(format!("Artifact {} chunk offset mismatch", artifact.id));
        }
        let data_end = storage
            .data_offset
            .checked_add(storage.stored_length)
            .ok_or_else(|| format!("Artifact {} chunk range overflow", artifact.id))?;
        if storage.frame_offset < HEADER_SIZE as u64 || data_end > manifest_offset {
            return Err(format!("Artifact {} chunk out of bounds", artifact.id));
        }
        chunk_ranges.push((storage.frame_offset, data_end, artifact.id.as_str()));
        let frame = read_exact_at(file, storage.frame_offset, CHUNK_HEADER_SIZE, control)?;
        if frame.get(..8) != Some(CHUNK_MAGIC.as_slice()) {
            return Err(format!("Artifact {} chunk magic mismatch", artifact.id));
        }
        if get_u16(&frame, 8)? != 1 || get_u16(&frame, 10)? != 0 {
            return Err(format!(
                "Artifact {} chunk version or codec unsupported",
                artifact.id
            ));
        }
        if get_u64(&frame, 16)? != storage.stored_length
            || get_u64(&frame, 24)? != storage.original_length
        {
            return Err(format!("Artifact {} chunk length mismatch", artifact.id));
        }
        let frame_sha: [u8; 32] = frame[32..64]
            .try_into()
            .map_err(|_| format!("Artifact {} chunk hash truncated", artifact.id))?;
        let length = super::limits::bounded_len(
            storage.stored_length,
            super::limits::MAX_ARTIFACT_BYTES,
            "artifact",
        )?;
        payload_bytes = payload_bytes
            .checked_add(storage.stored_length)
            .filter(|total| *total <= super::limits::MAX_PAYLOAD_BYTES)
            .ok_or("EDPB total payload exceeds read budget")?;
        let data = read_exact_at(file, storage.data_offset, length, control)?;
        let actual_sha = sha256_bytes(&data);
        if actual_sha != frame_sha || hex(&actual_sha) != storage.sha256 {
            return Err(format!("Artifact {} SHA-256 mismatch", artifact.id));
        }
        artifacts.insert(artifact.id.clone(), data);
    }

    chunk_ranges.sort_by_key(|range| range.0);
    for pair in chunk_ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(format!(
                "EDPB chunk storage overlap: {} and {}",
                pair[0].2, pair[1].2
            ));
        }
    }

    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("EDPB full-file seek failed: {e}"))?;
    let file_sha256 = crate::sha256::sha256_reader_hex(
        &mut crate::bounded_read::ControlledRead {
            reader: &mut *file,
            control,
        },
        super::limits::MAX_CONTAINER_BYTES,
    )
    .map_err(|error| format!("EDPB full-file read failed: {error}"))?;
    let after = file
        .metadata()
        .map_err(|error| format!("read EDPB metadata failed: {error}"))?;
    if after.len() != initial_metadata.len()
        || after.modified().ok() != initial_metadata.modified().ok()
    {
        return Err("EDPB changed while verifying".into());
    }
    Ok(VerifiedBackupReader {
        verified: VerifiedContainer {
            manifest,
            file_sha256,
        },
        artifacts,
    })
}

pub fn read_artifact(path: &Path, artifact_id: &str) -> Result<Vec<u8>, String> {
    Ok(VerifiedBackupReader::open(path)?
        .read_artifact(artifact_id)?
        .to_vec())
}

pub fn read_raw_protocol(path: &Path) -> Result<Vec<u8>, String> {
    read_artifact(path, RAW_PROTOCOL_ARTIFACT_ID)
}
