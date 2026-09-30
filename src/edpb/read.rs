use super::*;

pub(super) fn read_exact_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("EDPB seek failed: {e}"))?;
    let mut out = vec![0u8; len];
    file.read_exact(&mut out)
        .map_err(|e| format!("EDPB read failed: {e}"))?;
    Ok(out)
}

pub fn verify_file(path: &Path) -> Result<VerifiedContainer, String> {
    if path.extension().and_then(|v| v.to_str()) != Some(EXTENSION) {
        return Err(format!("not an .{EXTENSION} backup: {}", path.display()));
    }
    let mut file = File::open(path).map_err(|e| format!("open EDPB failed: {e}"))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("read EDPB metadata failed: {e}"))?
        .len();
    if file_len < (HEADER_SIZE + FOOTER_SIZE) as u64 {
        return Err("EDPB file too short".into());
    }
    let header = read_exact_at(&mut file, 0, HEADER_SIZE)?;
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
    let footer = read_exact_at(&mut file, footer_offset, FOOTER_SIZE)?;
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
    if manifest_end != footer_offset {
        return Err("EDPB manifest range is invalid".into());
    }
    let manifest_bytes = read_exact_at(&mut file, manifest_offset, manifest_len as usize)?;
    if sha256_bytes(&manifest_bytes) != header_manifest_sha {
        return Err("EDPB manifest SHA-256 mismatch".into());
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("parse EDPB manifest failed: {e}"))?;
    validate_manifest_graph(&manifest)?;

    let mut chunk_ranges = Vec::with_capacity(manifest.artifacts.len());
    for artifact in &manifest.artifacts {
        let storage = &artifact.storage;
        if storage.codec != "none" {
            return Err(format!(
                "Artifact {} uses unsupported codec {}",
                artifact.id, storage.codec
            ));
        }
        if storage.data_offset != storage.frame_offset + CHUNK_HEADER_SIZE as u64 {
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
        let frame = read_exact_at(&mut file, storage.frame_offset, CHUNK_HEADER_SIZE)?;
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
        let data = read_exact_at(
            &mut file,
            storage.data_offset,
            storage.stored_length as usize,
        )?;
        let actual_sha = sha256_bytes(&data);
        if actual_sha != frame_sha || hex(&actual_sha) != storage.sha256 {
            return Err(format!("Artifact {} SHA-256 mismatch", artifact.id));
        }
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
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("EDPB full-file read failed: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(VerifiedContainer {
        manifest,
        file_sha256: hex(&hasher.finalize()),
    })
}

pub fn read_artifact(path: &Path, artifact_id: &str) -> Result<Vec<u8>, String> {
    let verified = verify_file(path)?;
    let artifact = verified
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == artifact_id)
        .ok_or_else(|| format!("EDPB does not contain Artifact {artifact_id}"))?;
    let mut file = File::open(path).map_err(|e| format!("open EDPB failed: {e}"))?;
    read_exact_at(
        &mut file,
        artifact.storage.data_offset,
        artifact.storage.stored_length as usize,
    )
}

pub fn read_raw_protocol(path: &Path) -> Result<Vec<u8>, String> {
    read_artifact(path, RAW_PROTOCOL_ARTIFACT_ID)
}
