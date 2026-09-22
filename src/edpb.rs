//! EDP Backup Container v1.
//!
//! The container is self-contained: raw evidence, manifest and integrity
//! metadata live in one .edpb file. Legacy .bin files are not runtime input.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const EXTENSION: &str = "edpb";
pub const FORMAT_NAME: &str = "edpb";
pub const FORMAT_MAJOR: u16 = 1;
pub const FORMAT_MINOR: u16 = 0;
pub const HEADER_SIZE: usize = 96;
pub const CHUNK_HEADER_SIZE: usize = 64;
pub const FOOTER_SIZE: usize = 80;
pub const RAW_PROTOCOL_ARTIFACT_ID: &str = "raw.protocol.lba0_12";
pub const RAW_PROTOCOL_EXTENT_ID: &str = "extent.protocol.lba0_12";
pub const PROTOCOL_REGION_ID: &str = "region.protocol";

const FILE_MAGIC: &[u8; 8] = b"EDPB\r\n\x1a\n";
const CHUNK_MAGIC: &[u8; 8] = b"EDPCHNK\n";
const FOOTER_MAGIC: &[u8; 8] = b"EDPBFTR\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureLevel {
    Core,
    Metadata,
    Deep,
    LegacyMigrated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticStatus {
    Identified,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorePolicy {
    Restorable,
    EvidenceOnly,
    DerivedOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCompleteness {
    Complete,
    Partial,
    NotCaptured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerVersion {
    pub major: u16,
    pub minor: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub snapshot_id: String,
    pub created_epoch: i64,
    pub capture_level: CaptureLevel,
    pub device_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub vid: String,
    pub pid: String,
    pub device_id: String,
    pub onlyid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGeometry {
    pub logical_sector_size: u32,
    pub physical_sector_size: Option<u32>,
    pub total_sectors: Option<u64>,
    pub capacity_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub disk_number: Option<u32>,
    pub platform: String,
    pub edpcli_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub role: String,
    pub start_lba: Option<u64>,
    pub sector_count: Option<u64>,
    pub semantic_status: SemanticStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent {
    pub id: String,
    pub region_id: String,
    pub start_lba: u64,
    pub sector_count: u64,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Derivation {
    pub method: String,
    pub source_artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkStorage {
    pub frame_offset: u64,
    pub data_offset: u64,
    pub stored_length: u64,
    pub original_length: u64,
    pub codec: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub kind: String,
    pub media_type: String,
    pub source_extent_ids: Vec<String>,
    pub derivation: Option<Derivation>,
    pub restore_policy: RestorePolicy,
    pub completeness: ArtifactCompleteness,
    pub storage: ChunkStorage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub capture_source: String,
    pub source_format: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub container_version: ContainerVersion,
    pub snapshot: SnapshotInfo,
    pub device: DeviceIdentity,
    pub geometry: DeviceGeometry,
    pub observation: Observation,
    pub regions: Vec<Region>,
    pub extents: Vec<Extent>,
    pub artifacts: Vec<Artifact>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone)]
pub struct CoreCapture<'a> {
    pub snapshot_id: String,
    pub created_epoch: i64,
    pub disk_number: Option<u32>,
    pub vid: String,
    pub pid: String,
    pub device_id: String,
    pub onlyid: Option<String>,
    pub total_sectors: Option<u64>,
    pub logical_sector_size: u32,
    pub edpcli_version: String,
    pub device_state: String,
    pub lba0_12: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct ArtifactInput {
    pub id: String,
    pub kind: String,
    pub media_type: String,
    pub source_extent_ids: Vec<String>,
    pub derivation: Option<Derivation>,
    pub restore_policy: RestorePolicy,
    pub completeness: ArtifactCompleteness,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MetadataCapture<'a> {
    pub core: CoreCapture<'a>,
    pub regions: Vec<Region>,
    pub extents: Vec<Extent>,
    pub artifacts: Vec<ArtifactInput>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct VerifiedContainer {
    pub manifest: Manifest,
    pub file_sha256: String,
}

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("write to String");
    }
    out
}

fn put_u16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(src: &[u8], offset: usize) -> Result<u16, String> {
    src.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| "EDPB structure truncated at u16".to_string())
}

fn get_u32(src: &[u8], offset: usize) -> Result<u32, String> {
    src.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| "EDPB structure truncated at u32".to_string())
}

fn get_u64(src: &[u8], offset: usize) -> Result<u64, String> {
    src.get(offset..offset + 8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| "EDPB structure truncated at u64".to_string())
}

fn make_header(
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

fn make_chunk_header(data: &[u8]) -> [u8; CHUNK_HEADER_SIZE] {
    let mut out = [0u8; CHUNK_HEADER_SIZE];
    out[..8].copy_from_slice(CHUNK_MAGIC);
    put_u16(&mut out, 8, 1);
    put_u16(&mut out, 10, 0);
    put_u64(&mut out, 16, data.len() as u64);
    put_u64(&mut out, 24, data.len() as u64);
    out[32..64].copy_from_slice(&sha256_bytes(data));
    out
}

fn make_footer(
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

fn base_manifest(capture: &CoreCapture<'_>) -> Manifest {
    let capacity_bytes = capture
        .total_sectors
        .and_then(|sectors| sectors.checked_mul(capture.logical_sector_size as u64));
    Manifest {
        schema: "edpb.manifest.v1".into(),
        container_version: ContainerVersion {
            major: FORMAT_MAJOR,
            minor: FORMAT_MINOR,
        },
        snapshot: SnapshotInfo {
            snapshot_id: capture.snapshot_id.clone(),
            created_epoch: capture.created_epoch,
            capture_level: CaptureLevel::Core,
            device_state: capture.device_state.clone(),
        },
        device: DeviceIdentity {
            vid: capture.vid.clone(),
            pid: capture.pid.clone(),
            device_id: capture.device_id.clone(),
            onlyid: capture.onlyid.clone(),
        },
        geometry: DeviceGeometry {
            logical_sector_size: capture.logical_sector_size,
            physical_sector_size: None,
            total_sectors: capture.total_sectors,
            capacity_bytes,
        },
        observation: Observation {
            disk_number: capture.disk_number,
            platform: std::env::consts::OS.to_string(),
            edpcli_version: capture.edpcli_version.clone(),
        },
        regions: vec![Region {
            id: PROTOCOL_REGION_ID.into(),
            role: "protocol".into(),
            start_lba: Some(0),
            sector_count: Some(13),
            semantic_status: SemanticStatus::Identified,
        }],
        extents: vec![Extent {
            id: RAW_PROTOCOL_EXTENT_ID.into(),
            region_id: PROTOCOL_REGION_ID.into(),
            start_lba: 0,
            sector_count: 13,
            purpose: "raw_protocol_snapshot".into(),
        }],
        artifacts: Vec::new(),
        provenance: Provenance {
            capture_source: "physical_device".into(),
            source_format: "raw_device".into(),
            notes: vec!["raw evidence is authoritative; derived data must never replace it".into()],
        },
    }
}

fn validate_core_capture(capture: &CoreCapture<'_>) -> Result<(), String> {
    if capture.logical_sector_size == 0 {
        return Err("logical_sector_size must not be zero".into());
    }
    let expected_len = 13usize
        .checked_mul(capture.logical_sector_size as usize)
        .ok_or_else(|| "LBA0-12 length overflow".to_string())?;
    if capture.lba0_12.len() != expected_len {
        return Err(format!(
            "LBA0-12 length is {} bytes, expected {} bytes",
            capture.lba0_12.len(),
            expected_len
        ));
    }
    Ok(())
}

fn write_container(
    path: &Path,
    capture: &CoreCapture<'_>,
    capture_level: CaptureLevel,
    extra_regions: &[Region],
    extra_extents: &[Extent],
    extra_artifacts: &[ArtifactInput],
    extra_notes: &[String],
) -> Result<Manifest, String> {
    validate_core_capture(capture)?;
    if path.extension().and_then(|v| v.to_str()) != Some(EXTENSION) {
        return Err(format!("EDPB file must use .{EXTENSION} extension"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create backup directory failed: {e}"))?;
    }

    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("create EDPB failed {}: {e}", path.display()))?;

    let result = (|| -> Result<Manifest, String> {
        file.write_all(&[0u8; HEADER_SIZE])
            .map_err(|e| format!("write EDPB header failed: {e}"))?;

        let mut manifest = base_manifest(capture);
        manifest.snapshot.capture_level = capture_level;
        manifest.regions.extend_from_slice(extra_regions);
        manifest.extents.extend_from_slice(extra_extents);
        manifest.provenance.notes.extend_from_slice(extra_notes);

        let mut inputs = Vec::with_capacity(extra_artifacts.len() + 1);
        inputs.push(ArtifactInput {
            id: RAW_PROTOCOL_ARTIFACT_ID.into(),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![RAW_PROTOCOL_EXTENT_ID.into()],
            derivation: None,
            restore_policy: RestorePolicy::Restorable,
            completeness: ArtifactCompleteness::Complete,
            data: capture.lba0_12.to_vec(),
        });
        inputs.extend_from_slice(extra_artifacts);

        for input in inputs {
            let frame_offset = file
                .stream_position()
                .map_err(|e| format!("read EDPB chunk position failed: {e}"))?;
            file.write_all(&make_chunk_header(&input.data))
                .map_err(|e| format!("write EDPB chunk header failed: {e}"))?;
            let data_offset = frame_offset + CHUNK_HEADER_SIZE as u64;
            file.write_all(&input.data)
                .map_err(|e| format!("write EDPB chunk {} failed: {e}", input.id))?;
            manifest.artifacts.push(Artifact {
                id: input.id,
                kind: input.kind,
                media_type: input.media_type,
                source_extent_ids: input.source_extent_ids,
                derivation: input.derivation,
                restore_policy: input.restore_policy,
                completeness: input.completeness,
                storage: ChunkStorage {
                    frame_offset,
                    data_offset,
                    stored_length: input.data.len() as u64,
                    original_length: input.data.len() as u64,
                    codec: "none".into(),
                    sha256: hex(&sha256_bytes(&input.data)),
                },
            });
        }

        let manifest_offset = file
            .stream_position()
            .map_err(|e| format!("read EDPB write position failed: {e}"))?;
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("serialize EDPB manifest failed: {e}"))?;
        let manifest_sha = sha256_bytes(&manifest_bytes);
        file.write_all(&manifest_bytes)
            .map_err(|e| format!("write EDPB manifest failed: {e}"))?;
        let footer_offset = file
            .stream_position()
            .map_err(|e| format!("read EDPB footer position failed: {e}"))?;
        let file_size = footer_offset + FOOTER_SIZE as u64;
        file.write_all(&make_footer(
            manifest_offset,
            manifest_bytes.len() as u64,
            file_size,
            &manifest_sha,
        ))
        .map_err(|e| format!("write EDPB footer failed: {e}"))?;

        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("seek EDPB header failed: {e}"))?;
        file.write_all(&make_header(
            capture.created_epoch,
            manifest_offset,
            manifest_bytes.len() as u64,
            footer_offset,
            &manifest_sha,
        ))
        .map_err(|e| format!("rewrite EDPB header failed: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync EDPB failed: {e}"))?;
        Ok(manifest)
    })();

    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(path);
    }
    result
}

pub fn write_core_backup(path: &Path, capture: &CoreCapture<'_>) -> Result<Manifest, String> {
    write_container(path, capture, CaptureLevel::Core, &[], &[], &[], &[])
}

pub fn write_metadata_backup(
    path: &Path,
    capture: &MetadataCapture<'_>,
) -> Result<Manifest, String> {
    write_container(
        path,
        &capture.core,
        CaptureLevel::Metadata,
        &capture.regions,
        &capture.extents,
        &capture.artifacts,
        &capture.notes,
    )
}

/// Write the Metadata superset with Deep-derived artifacts into one container.
pub fn write_deep_backup(path: &Path, capture: &MetadataCapture<'_>) -> Result<Manifest, String> {
    if capture
        .artifacts
        .iter()
        .any(|a| a.kind != "raw_sectors" && a.restore_policy != RestorePolicy::DerivedOnly)
    {
        return Err("Deep interpretation must be derived_only".into());
    }
    write_container(
        path,
        &capture.core,
        CaptureLevel::Deep,
        &capture.regions,
        &capture.extents,
        &capture.artifacts,
        &capture.notes,
    )
}

fn read_exact_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("EDPB seek failed: {e}"))?;
    let mut out = vec![0u8; len];
    file.read_exact(&mut out)
        .map_err(|e| format!("EDPB read failed: {e}"))?;
    Ok(out)
}

fn validate_manifest_graph(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema != "edpb.manifest.v1" {
        return Err(format!(
            "unsupported EDPB manifest schema: {}",
            manifest.schema
        ));
    }
    if manifest.container_version.major != FORMAT_MAJOR {
        return Err(format!(
            "unsupported EDPB major version: {}",
            manifest.container_version.major
        ));
    }
    let region_ids: BTreeSet<&str> = manifest.regions.iter().map(|v| v.id.as_str()).collect();
    if region_ids.len() != manifest.regions.len() {
        return Err("duplicate EDPB region id".into());
    }
    let extent_ids: BTreeSet<&str> = manifest.extents.iter().map(|v| v.id.as_str()).collect();
    if extent_ids.len() != manifest.extents.len() {
        return Err("duplicate EDPB extent id".into());
    }
    for extent in &manifest.extents {
        if !region_ids.contains(extent.region_id.as_str()) {
            return Err(format!("Extent {} references missing Region", extent.id));
        }
    }
    let artifact_ids: BTreeSet<&str> = manifest.artifacts.iter().map(|v| v.id.as_str()).collect();
    if artifact_ids.len() != manifest.artifacts.len() {
        return Err("duplicate EDPB artifact id".into());
    }
    for artifact in &manifest.artifacts {
        for extent in &artifact.source_extent_ids {
            if !extent_ids.contains(extent.as_str()) {
                return Err(format!(
                    "Artifact {} references missing Extent",
                    artifact.id
                ));
            }
        }
        if let Some(derivation) = &artifact.derivation {
            for source in &derivation.source_artifact_ids {
                if !artifact_ids.contains(source.as_str()) {
                    return Err(format!("Artifact {} has missing source", artifact.id));
                }
            }
        }
    }

    let protocol_region = manifest
        .regions
        .iter()
        .find(|region| region.id == PROTOCOL_REGION_ID)
        .ok_or_else(|| "EDPB manifest missing protocol Region".to_string())?;
    if protocol_region.start_lba != Some(0) || protocol_region.sector_count != Some(13) {
        return Err("EDPB protocol Region geometry mismatch".into());
    }
    let protocol_extent = manifest
        .extents
        .iter()
        .find(|extent| extent.id == RAW_PROTOCOL_EXTENT_ID)
        .ok_or_else(|| "EDPB manifest missing protocol Extent".to_string())?;
    if protocol_extent.start_lba != 0 || protocol_extent.sector_count != 13 {
        return Err("EDPB protocol Extent geometry mismatch".into());
    }
    for extent in &manifest.extents {
        let end = extent
            .start_lba
            .checked_add(extent.sector_count)
            .ok_or_else(|| format!("Extent {} source range overflow", extent.id))?;
        if let Some(total) = manifest.geometry.total_sectors {
            if end > total {
                return Err(format!(
                    "Extent {} exceeds source device geometry",
                    extent.id
                ));
            }
        }
    }
    let raw_protocol = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == RAW_PROTOCOL_ARTIFACT_ID)
        .ok_or_else(|| "EDPB manifest missing raw protocol Artifact".to_string())?;
    let expected_protocol_bytes = 13u64
        .checked_mul(manifest.geometry.logical_sector_size as u64)
        .ok_or_else(|| "EDPB protocol byte length overflow".to_string())?;
    if raw_protocol.storage.original_length != expected_protocol_bytes
        || raw_protocol.restore_policy != RestorePolicy::Restorable
    {
        return Err("EDPB raw protocol Artifact contract mismatch".into());
    }
    for artifact in &manifest.artifacts {
        if artifact.kind == "raw_sectors" && artifact.source_extent_ids.len() == 1 {
            let extent = manifest
                .extents
                .iter()
                .find(|extent| extent.id == artifact.source_extent_ids[0])
                .ok_or_else(|| format!("Artifact {} source Extent missing", artifact.id))?;
            let expected = extent
                .sector_count
                .checked_mul(manifest.geometry.logical_sector_size as u64)
                .ok_or_else(|| format!("Artifact {} raw length overflow", artifact.id))?;
            if artifact.storage.original_length != expected {
                return Err(format!(
                    "Artifact {} raw extent length mismatch",
                    artifact.id
                ));
            }
        }
    }
    Ok(())
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
