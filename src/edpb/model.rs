use super::*;

pub const EXTENSION: &str = "edpb";
pub const FORMAT_MAJOR: u16 = 1;
pub const FORMAT_MINOR: u16 = 0;
pub const HEADER_SIZE: usize = 96;
pub const CHUNK_HEADER_SIZE: usize = 64;
pub const FOOTER_SIZE: usize = 80;
pub const RAW_PROTOCOL_ARTIFACT_ID: &str = "raw.protocol.lba0_12";
pub const RAW_PROTOCOL_EXTENT_ID: &str = "extent.protocol.lba0_12";
pub const PROTOCOL_REGION_ID: &str = "region.protocol";

pub(super) const FILE_MAGIC: &[u8; 8] = b"EDPB\r\n\x1a\n";
pub(super) const CHUNK_MAGIC: &[u8; 8] = b"EDPCHNK\n";
pub(super) const FOOTER_MAGIC: &[u8; 8] = b"EDPBFTR\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureLevel {
    Core,
    Metadata,
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
pub enum BackupPurpose {
    MetadataOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreContract {
    pub restores_partition_structure: bool,
    pub restores_edp_protocol: bool,
    pub restores_filesystem: bool,
    pub restores_user_data: bool,
    pub post_restore_assessment_required: bool,
}

impl RestoreContract {
    pub fn metadata_only(restores_edp_protocol: bool) -> Self {
        Self {
            restores_partition_structure: true,
            restores_edp_protocol,
            restores_filesystem: false,
            restores_user_data: false,
            post_restore_assessment_required: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestPartition {
    pub index: u32,
    pub role: Option<String>,
    pub partition_type: Option<String>,
    pub start_lba: u64,
    pub sector_count: u64,
    pub filesystem_hint: Option<String>,
    pub volume_label_hint: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestSerialQuality {
    Usable,
    Suspicious,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestTransport {
    Uas,
    Bot,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestProvisionKind {
    Plain,
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestHardwareIdentity {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    #[serde(default)]
    pub serial: Option<String>,
    pub serial_quality: ManifestSerialQuality,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
    pub transport: Option<ManifestTransport>,
    pub total_sectors: Option<u64>,
    pub logical_sector_size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestProtocolIdentity {
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub provision_kind: Option<ManifestProvisionKind>,
    pub lba4_identity_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ManifestDerivedIdentity {
    #[serde(default)]
    pub device_id_candidates: Vec<String>,
    pub legacy_derived_candidate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestIdentity {
    pub hardware: ManifestHardwareIdentity,
    pub protocol: ManifestProtocolIdentity,
    pub derived: ManifestDerivedIdentity,
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
    pub backup_purpose: BackupPurpose,
    pub restore_contract: RestoreContract,
    pub device: DeviceIdentity,
    pub identity: ManifestIdentity,
    pub geometry: DeviceGeometry,
    pub observation: Observation,
    #[serde(default)]
    pub partitions: Vec<ManifestPartition>,
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
    pub partitions: Vec<ManifestPartition>,
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
