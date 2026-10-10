//! Shared provisioning application service.
//!
//! This layer resolves a real USB target, creates the exact pure-domain write
//! plan, and owns the final raw-device safety transition. CLI/TUI must not
//! duplicate these checks or construct alternative raw-write patches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::backup_metadata::{parse_lba7_compatibility_geometry, PartitionGeometry};
use crate::common::{
    EdpCliError, EdpCliResult, EXIT_INTERMEDIATE, EXIT_IO, EXIT_OK, EXIT_ROLLED_BACK, EXIT_TARGET,
    SECTOR,
};
use crate::diskio;
use crate::filesystem::analysis::{analyze_partition, AnalysisStatus, PartitionReader};
use crate::filesystem::{FilesystemKind, SparseFilesystemImage};
use crate::platform::system;
use crate::ports::{CmdRunner, SectorDev};
use crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity;
use crate::provision::{
    apply_target_geometry_overrides, build_official_provision_protocol_image,
    build_plain_provision_write_plan, parse_existing_provision, prefill_for_target_mode,
    unwrap_legacy_lba7_file_key, wrap_file_key, wrap_legacy_lba7_file_key, CapacityInput,
    CapacitySource, FileKeyWrapMode, KeyDomainRole, KeyDomainSecrets, OfficialPartitionFilesystems,
    OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan,
    OfficialProvisionWriteImage, OnlyId, ParsedExistingProvision, PartitionFilesystemImage,
    PartitionFormatTarget, PartitionRole, PassInfoPolicy, PlainCleanupExtent, PlainPartitionSpec,
    PlainProvisionPlan, PlainProvisionWritePlan, ProvisionEntropy, ProvisionImage,
    ProvisionMetadata, ProvisionProfile, ProvisionSpec, ProvisionTarget, QuickCapacityUnit,
    RegionDisposition, SourcePasswordKnowledge, TargetGeometryOverrides, TargetIdentity,
    TargetPasswordPolicy, TargetProvisionPlan, DEFAULT_KEY_DOMAIN_PASSWORD,
    DEFAULT_MODE0_BOOT_SECTORS,
};

use super::device::open_readonly_usb_disk;
use super::media_identity::MediaIdentityPin;
use super::media_identity_observer::media_identity_from_protocol_image;
use super::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use super::write::{read_image, verify_reopened_snapshot};

const OPEN_WAIT: Duration = Duration::from_secs(10);

fn err(code: i32, message: impl Into<String>) -> EdpCliError {
    EdpCliError::new(code, message)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficialProvisionRequest {
    pub target: ProvisionTarget,
    /// Manufacturer global algorithm selector; not an independent FileKey mode.
    pub algorithm: crate::provision::OfficialLabelAlgorithm,
    pub boot_start_lba: Option<u64>,
    pub share_start_lba: Option<u64>,
    pub encrypt_start_lba: Option<u64>,
    pub boot_mib: Option<u64>,
    pub boot_sectors: Option<u64>,
    pub share_mib: Option<u64>,
    pub share_sectors: Option<u64>,
    pub encrypt_mib: Option<u64>,
    pub encrypt_sectors: Option<u64>,
    pub label_id: String,
    pub user: String,
    pub dept: String,
    pub label: String,
    pub lba8_identity: crate::provision::Lba8Identity,
    pub key_domains: KeyDomainSecrets,
    pub volume_label: String,
    pub format: FormatOptions,
    /// True when the caller's unchecked format roles mean *preserve the
    /// existing bytes and FileKey*, not "use the destructive CLI defaults".
    /// The native writer must never reinterpret this as permission to format.
    pub preserve_unformatted: bool,
    pub force_change_password: Option<bool>,
    pub cancel_password_complexity_check: Option<bool>,
    pub max_share_password_errors: Option<u8>,
    pub max_encrypt_password_errors: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlainPartitionSize {
    Sectors(u64),
    MiB(u64),
    GiB(u64),
    Fill,
}

impl PlainPartitionSize {
    fn sectors_typed(self) -> Result<Option<u64>, ProvisionPlanningError> {
        match self {
            Self::Sectors(value) => Ok(Some(value)),
            Self::MiB(value) => value
                .checked_mul(1024 * 1024 / SECTOR as u64)
                .map(Some)
                .ok_or(ProvisionPlanningError::SizeOverflow { unit: "MiB" }),
            Self::GiB(value) => value
                .checked_mul(1024 * 1024 * 1024 / SECTOR as u64)
                .map(Some)
                .ok_or(ProvisionPlanningError::SizeOverflow { unit: "GiB" }),
            Self::Fill => Ok(None),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainPartitionRequest {
    pub start_lba: u64,
    pub size: PlainPartitionSize,
    pub filesystem: FilesystemKind,
    pub volume_label: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlainProvisionRequest {
    pub partitions: Vec<PlainPartitionRequest>,
}

impl PlainProvisionRequest {
    pub fn from_plan(plan: &PlainProvisionPlan) -> Self {
        Self {
            partitions: plan
                .partitions
                .iter()
                .map(|partition| PlainPartitionRequest {
                    start_lba: partition.start_lba,
                    size: PlainPartitionSize::Sectors(partition.sector_count),
                    filesystem: partition.filesystem,
                    volume_label: partition.volume_label.clone(),
                })
                .collect(),
        }
    }

    pub fn resolve_typed(
        &self,
        total_sectors: u64,
    ) -> Result<PlainProvisionPlan, ProvisionPlanningError> {
        if self.partitions.is_empty() {
            return PlainProvisionPlan::default_for_disk(total_sectors)
                .map_err(ProvisionPlanningError::Geometry);
        }
        if self.partitions.len() > crate::provision::MAX_PLAIN_PARTITIONS {
            return Err(ProvisionPlanningError::TooManyPartitions {
                count: self.partitions.len(),
                max: crate::provision::MAX_PLAIN_PARTITIONS,
            });
        }

        let mut partitions = Vec::with_capacity(self.partitions.len());
        for (index, request) in self.partitions.iter().enumerate() {
            crate::filesystem::validate_writable_filesystem(request.filesystem).map_err(
                |source| ProvisionPlanningError::Filesystem {
                    partition: Some(index),
                    source,
                },
            )?;
            let next_start = self
                .partitions
                .iter()
                .enumerate()
                .filter(|(other_index, other)| {
                    *other_index != index && other.start_lba > request.start_lba
                })
                .map(|(_, other)| other.start_lba)
                .min()
                .unwrap_or(total_sectors);
            let sector_count = match request.size.sectors_typed()? {
                Some(value) => value,
                None => next_start
                    .checked_sub(request.start_lba)
                    .filter(|value| *value > 0)
                    .ok_or(ProvisionPlanningError::FillWithoutSpace { partition: index })?,
            };
            partitions.push(PlainPartitionSpec::new(
                request.start_lba,
                sector_count,
                request.filesystem,
                request.volume_label.clone(),
            ));
        }
        PlainProvisionPlan::new(total_sectors, partitions).map_err(ProvisionPlanningError::Geometry)
    }

    pub fn resolve(&self, total_sectors: u64) -> Result<PlainProvisionPlan, String> {
        self.resolve_typed(total_sectors)
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProvisionRequest {
    Official(Box<OfficialProvisionRequest>),
    Plain(PlainProvisionRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatOptions {
    pub boot: bool,
    pub share: bool,
    pub encrypt: bool,
    pub boot_label: String,
    pub share_label: String,
    pub encrypt_label: String,
    pub boot_fs: FilesystemKind,
    pub share_fs: FilesystemKind,
    pub encrypt_fs: FilesystemKind,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            boot: false,
            share: false,
            encrypt: false,
            boot_label: "启动区".into(),
            share_label: "交换区".into(),
            encrypt_label: "保密区".into(),
            boot_fs: FilesystemKind::Fat16,
            share_fs: FilesystemKind::ExFat,
            encrypt_fs: FilesystemKind::ExFat,
        }
    }
}

impl FormatOptions {
    pub fn filesystems(&self) -> OfficialPartitionFilesystems {
        OfficialPartitionFilesystems {
            boot: self.boot_fs,
            share: self.share_fs,
            encrypt: self.encrypt_fs,
        }
    }
    fn choice(&self, role: PartitionRole) -> (bool, &str) {
        match role {
            PartitionRole::Boot => (self.boot, &self.boot_label),
            PartitionRole::Share => (self.share, &self.share_label),
            PartitionRole::BootShareCombined => (self.share, &self.boot_label),
            PartitionRole::Encrypt => (self.encrypt, &self.encrypt_label),
            PartitionRole::CompatibilityReserve => (false, ""),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PlannedPartitionFormat {
    pub target: PartitionFormatTarget,
    pub selected: bool,
    pub filesystem: Option<FilesystemKind>,
    pub volume_label: String,
    pub volume_serial: u32,
    pub prepared_image: Option<PartitionFilesystemImage>,
    pub verification_image: Option<SparseFilesystemImage>,
}

impl std::fmt::Debug for PlannedPartitionFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlannedPartitionFormat")
            .field("target", &self.target)
            .field("selected", &self.selected)
            .field("filesystem", &self.filesystem)
            .field("volume_label", &self.volume_label)
            .field("volume_serial", &self.volume_serial)
            .field("prepared_image", &self.prepared_image.is_some())
            .field("verification_image", &self.verification_image.is_some())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionFormatResult {
    pub role: PartitionRole,
    pub result: Result<(), PartitionFormatError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PartitionFormatError {
    Failed(super::error::OperationError),
    Skipped { after: PartitionRole },
}

impl PartitionFormatError {
    pub fn operation_error(&self) -> Option<&super::error::OperationError> {
        match self {
            Self::Failed(error) => Some(error),
            Self::Skipped { .. } => None,
        }
    }

    pub const fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped { .. })
    }
}

impl From<String> for PartitionFormatError {
    fn from(message: String) -> Self {
        Self::Failed(message.into())
    }
}

impl From<&str> for PartitionFormatError {
    fn from(message: &str) -> Self {
        Self::Failed(message.into())
    }
}

impl std::fmt::Display for PartitionFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(error) => std::fmt::Display::fmt(error, f),
            Self::Skipped { after } => {
                write!(f, "未执行：{}格式化失败后已停止后续写入", after.label())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvisionCommitReport {
    pub provision_succeeded: bool,
    pub formats: Vec<PartitionFormatResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProvisionCommitOutcome {
    Official(ProvisionCommitReport),
    Plain { partition_count: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvisionWriteOutcome {
    pub backup: super::post_restore::MetadataBackupReport,
    pub commit: ProvisionCommitOutcome,
    pub warnings: Vec<ProvisionWarning>,
}

mod assessment;
pub use assessment::PreserveAssessment;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvisionExecutionStatus {
    Success,
    CompletedWithWarnings,
    PartialFormatFailure,
    FormatRolledBack,
    MediaIntermediate,
    MediaStateUnknown,
    FatalFailure,
}

impl ProvisionExecutionStatus {
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Success | Self::CompletedWithWarnings => EXIT_OK,
            Self::PartialFormatFailure | Self::FatalFailure => EXIT_IO,
            Self::FormatRolledBack => EXIT_ROLLED_BACK,
            Self::MediaIntermediate | Self::MediaStateUnknown => EXIT_INTERMEDIATE,
        }
    }
}

impl ProvisionWriteOutcome {
    pub fn exit_code(&self) -> i32 {
        let status = self.execution_status();
        if matches!(
            status,
            ProvisionExecutionStatus::MediaIntermediate
                | ProvisionExecutionStatus::MediaStateUnknown
        ) {
            return status.exit_code();
        }
        if let ProvisionCommitOutcome::Official(report) = &self.commit {
            if let Some(code) = report
                .formats
                .iter()
                .find_map(|format| format.result.as_ref().err()?.operation_error()?.code)
            {
                return code;
            }
        }
        status.exit_code()
    }

    pub fn execution_status(&self) -> ProvisionExecutionStatus {
        use super::error::MediaState;
        if let ProvisionCommitOutcome::Official(report) = &self.commit {
            let states = report
                .formats
                .iter()
                .filter_map(|format| format.result.as_ref().err()?.operation_error()?.media_state)
                .collect::<Vec<_>>();
            if states.contains(&MediaState::Intermediate) {
                return ProvisionExecutionStatus::MediaIntermediate;
            }
            if states.contains(&MediaState::Unknown) {
                return ProvisionExecutionStatus::MediaStateUnknown;
            }
            if states.contains(&MediaState::RolledBack) {
                return ProvisionExecutionStatus::FormatRolledBack;
            }
            if !report.provision_succeeded {
                return ProvisionExecutionStatus::FatalFailure;
            }
        }
        if self
            .warnings
            .iter()
            .any(|warning| matches!(warning, ProvisionWarning::IncompleteFormat))
            || matches!(
                &self.commit,
                ProvisionCommitOutcome::Official(report)
                    if report.formats.iter().any(|format| format.result.is_err())
            )
        {
            ProvisionExecutionStatus::PartialFormatFailure
        } else if self.warnings.is_empty() {
            ProvisionExecutionStatus::Success
        } else {
            ProvisionExecutionStatus::CompletedWithWarnings
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProvisionWarning {
    IncompleteFormat,
    AfterIdentityObservationFailed(String),
    HostLineagePersistenceFailed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProvisionKeyProbe {
    pub source_kind: crate::provision::DiskProvisionKind,
    /// Decoded per-partition on-disk LBA12 EncryptMode; not a target write permit.
    pub share_source_algorithm: Option<crate::provision::OfficialLabelAlgorithm>,
    pub encrypt_source_algorithm: Option<crate::provision::OfficialLabelAlgorithm>,
    pub share: Option<SourcePasswordKnowledge>,
    pub share_opaque_profile: bool,
    pub encrypt: Option<SourcePasswordKnowledge>,
    pub encrypt_opaque_profile: bool,
}

/// Prepared safety facts cannot be edited by external callers; prepare a new request instead.
/// ```compile_fail
/// use edpcli::application::provision::PreparedNewProvision;
/// fn change_target(plan: &mut PreparedNewProvision) { plan.disk = 99; }
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedNewProvision {
    pub(crate) disk: u32,
    pub(crate) device_id: String,
    pub(crate) source_kind: crate::provision::DiskProvisionKind,
    pub(crate) mode: OfficialPartitionMode,
    pub(crate) algorithm: crate::provision::OfficialLabelAlgorithm,
    pub(crate) force_change_password: bool,
    pub(crate) pass_info_policy: PassInfoPolicy,
    pub(crate) lce_start_lba: u64,
    pub(crate) write_image: OfficialProvisionWriteImage,
    pub(crate) format_targets: Vec<PlannedPartitionFormat>,
    pub(crate) target_plan: Option<TargetProvisionPlan>,
    source_metadata: Option<Vec<u8>>,
    before_pin: MediaIdentityPin,
    plan: OfficialProvisionPlan,
    expected_onlyid: String,
    expected_serial_digest: Option<String>,
    expected_probe: crate::platform::HardwareProbe,
    expected_lba3: Option<[u8; SECTOR]>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PreparedPlainProvision {
    pub(crate) disk: u32,
    pub(crate) device_id: String,
    pub(crate) plan: PlainProvisionPlan,
    pub(crate) write_plan: PlainProvisionWritePlan,
    pub(crate) source_kind: crate::provision::DiskProvisionKind,
    pub(crate) source_lce_start_lba: Option<u64>,
    source_metadata: Vec<u8>,
    before_pin: MediaIdentityPin,
    expected_probe: crate::platform::HardwareProbe,
}

impl std::fmt::Debug for PreparedPlainProvision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedPlainProvision")
            .field("disk", &self.disk)
            .field("device_id", &self.device_id)
            .field("plan", &self.plan)
            .field("write_plan", &self.write_plan)
            .field("source_kind", &self.source_kind)
            .field("source_lce_start_lba", &self.source_lce_start_lba)
            .field("source_metadata_len", &self.source_metadata.len())
            .field("before_pin", &self.before_pin)
            .field("expected_probe", &self.expected_probe)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PreparedNewProvision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedNewProvision")
            .field("disk", &self.disk)
            .field("device_id", &self.device_id)
            .field("source_kind", &self.source_kind)
            .field("mode", &self.mode)
            .field("algorithm", &self.algorithm)
            .field("force_change_password", &self.force_change_password)
            .field("pass_info_policy", &self.pass_info_policy)
            .field("lce_start_lba", &self.lce_start_lba)
            .field("write_image", &self.write_image)
            .field("format_targets", &self.format_targets)
            .field("target_plan", &self.target_plan)
            .field("source_metadata_captured", &self.source_metadata.is_some())
            .field("plan", &self.plan)
            .field("expected_onlyid", &self.expected_onlyid)
            .field("expected_serial_digest", &self.expected_serial_digest)
            .field("before_pin", &self.before_pin)
            .field("expected_probe", &self.expected_probe)
            .field("expected_lba3", &self.expected_lba3)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedProvision {
    Official(Box<PreparedNewProvision>),
    Plain(Box<PreparedPlainProvision>),
    Native(Box<native_flow::NativePreparedProvision>),
}

impl PreparedProvision {
    pub const fn target(&self) -> ProvisionTarget {
        match self {
            Self::Official(prepared) => ProvisionTarget::Official(prepared.mode),
            Self::Plain(_) => ProvisionTarget::Plain,
            Self::Native(native) => native.target,
        }
    }

    pub const fn disk(&self) -> u32 {
        match self {
            Self::Official(prepared) => prepared.disk,
            Self::Plain(prepared) => prepared.disk,
            Self::Native(native) => native.disk,
        }
    }

    pub fn device_id(&self) -> &str {
        match self {
            Self::Official(prepared) => &prepared.device_id,
            Self::Plain(prepared) => &prepared.device_id,
            Self::Native(native) => &native.device_id,
        }
    }

    fn before_pin(&self) -> &MediaIdentityPin {
        match self {
            Self::Official(prepared) => &prepared.before_pin,
            Self::Plain(prepared) => &prepared.before_pin,
            Self::Native(native) => &native.before_pin,
        }
    }

    fn source_metadata(&self) -> EdpCliResult<&[u8]> {
        match self {
            Self::Official(prepared) => prepared.source_metadata.as_deref().ok_or_else(|| {
                err(
                    EXIT_TARGET,
                    "错误: 制盘前缺少来源 LBA0–12 快照，无法创建强制备份",
                )
            }),
            Self::Plain(prepared) => Ok(&prepared.source_metadata),
            Self::Native(native) => Ok(&native.source_protocol_projection),
        }
    }

    fn source_backup_onlyid(&self) -> EdpCliResult<Option<String>> {
        let source = self.source_metadata()?;
        let lba4 = source
            .get(4 * SECTOR..5 * SECTOR)
            .ok_or_else(|| err(EXIT_TARGET, "错误: 来源快照缺少 LBA4，无法绑定强制备份身份"))?;
        Ok(crate::infrastructure::backup_store::catalog::lba4_label_id_from(lba4))
    }
}

fn random_array<const N: usize>() -> EdpCliResult<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| err(EXIT_IO, format!("错误: 系统随机数生成失败: {error}")))?;
    Ok(bytes)
}

fn official_mode(target: ProvisionTarget) -> EdpCliResult<OfficialPartitionMode> {
    target.official_mode().ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 当前入口只接受官方模式目标；普通盘必须使用 Plain planner",
        )
    })
}

fn sizes(
    request: &OfficialProvisionRequest,
    mode: OfficialPartitionMode,
) -> EdpCliResult<OfficialPartitionSizes> {
    let share_region = if mode == OfficialPartitionMode::BootShareCombined {
        PartitionRole::BootShareCombined.label()
    } else {
        PartitionRole::Share.label()
    };
    if request.boot_mib.is_some() && request.boot_sectors.is_some() {
        return Err(err(
            EXIT_TARGET,
            "错误: 启动区不能同时指定 MiB 和精确扇区数",
        ));
    }
    if request.share_mib.is_some() && request.share_sectors.is_some() {
        return Err(err(
            EXIT_TARGET,
            format!("错误: {share_region}不能同时指定 MiB 和精确扇区数"),
        ));
    }
    if request.encrypt_mib.is_some() && request.encrypt_sectors.is_some() {
        return Err(err(
            EXIT_TARGET,
            "错误: 保密区不能同时指定 MiB 和精确扇区数",
        ));
    }
    if request.boot_sectors.is_some()
        && !matches!(
            mode,
            OfficialPartitionMode::DefaultThreePartition
                | OfficialPartitionMode::IntranetExtranetDualPartition
        )
    {
        return Err(err(EXIT_TARGET, "错误: 精确启动区扇区数仅用于官方模式0/3"));
    }
    // Unused fields are ignored by the official mode; keep a non-zero sentinel
    // so domain validation cannot accidentally turn an unused value into a
    // zero-size emitted partition if a mode definition changes later.
    let mut sizes = OfficialPartitionSizes::new(
        request.boot_mib.unwrap_or(1),
        request.share_mib.unwrap_or(1),
        request.encrypt_mib.unwrap_or(1),
    );
    if let Some(boot_sectors) = request.boot_sectors {
        if boot_sectors == 0 {
            return Err(err(EXIT_TARGET, "错误: 启动区扇区数必须大于 0"));
        }
        sizes = sizes.with_boot_sectors(boot_sectors);
    } else if mode == OfficialPartitionMode::DefaultThreePartition && request.boot_mib.is_none() {
        sizes = sizes.with_boot_sectors(DEFAULT_MODE0_BOOT_SECTORS);
    }
    if let Some(share_sectors) = request.share_sectors {
        if share_sectors == 0 {
            return Err(err(
                EXIT_TARGET,
                format!("错误: {share_region}扇区数必须大于 0"),
            ));
        }
        sizes = sizes.with_share_sectors(share_sectors);
    }
    if let Some(encrypt_sectors) = request.encrypt_sectors {
        if encrypt_sectors == 0 {
            return Err(err(EXIT_TARGET, "错误: 保密区扇区数必须大于 0"));
        }
        sizes = sizes.with_encrypt_sectors(encrypt_sectors);
    }
    Ok(sizes)
}

mod commit;
mod error;
mod export;
mod format_plan;
mod identity_lineage;
mod prepare;
mod prepared_projection;
mod progress_projection;
mod review_facts;
pub(crate) use review_facts::assess_official_review;

pub use commit::capture_manufacturer_lba3;
pub use error::ProvisionPlanningError;
pub use export::{
    export_provision_image, export_sparse_plain_provision_image, export_sparse_provision_image,
};
pub use format_plan::plan_format_targets_typed;
use format_plan::plan_format_targets_with_keys;
pub use prepare::{
    prepare_plain_provision, prepare_provision, prepare_target_provision,
    probe_provision_key_domains_on_disk, verify_provision_source_password_on_disk,
};
#[cfg(test)]
use prepare::{read_plain_source_extents, target_encrypt_capacity_override};

pub fn prepare_provision_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &ProvisionRequest,
) -> EdpCliResult<PreparedProvision> {
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    prepare_provision(runner, disk, request, &mut dev)
}

/// Real device commit requires a verified, source-bound backup capability.
/// ```compile_fail
/// use edpcli::application::provision::{PreparedProvision, commit_provision_on_disk};
/// use edpcli::ports::CmdRunner;
/// fn bypass(runner: &dyn CmdRunner, prepared: &PreparedProvision) {
///     commit_provision_on_disk(runner, prepared).unwrap();
/// }
/// ```
pub fn commit_provision_on_disk(
    runner: &dyn CmdRunner,
    backed_up: &BackedUpPreparedProvision<'_>,
) -> EdpCliResult<ProvisionCommitOutcome> {
    commit_provision_on_disk_with_progress(runner, backed_up, &mut |_, _, _| {})
}

/// A source-bound backup proof. Only the verified backup acquisition creates it.
/// ```no_run
/// use edpcli::application::provision::{BackedUpPreparedProvision, commit_provision_on_disk};
/// use edpcli::ports::CmdRunner;
/// fn commit(runner: &dyn CmdRunner, proof: &BackedUpPreparedProvision<'_>) {
///     commit_provision_on_disk(runner, proof).unwrap();
/// }
/// ```
pub struct BackedUpPreparedProvision<'a> {
    prepared: &'a PreparedProvision,
    backup: super::post_restore::MetadataBackupReport,
    sha256: String,
}

impl BackedUpPreparedProvision<'_> {
    pub fn prepared(&self) -> &PreparedProvision {
        self.prepared
    }
    pub fn backup(&self) -> &super::post_restore::MetadataBackupReport {
        &self.backup
    }
}

pub fn backup_prepared_provision_on_disk<'a>(
    runner: &dyn CmdRunner,
    prepared: &'a PreparedProvision,
    backup_dir: PathBuf,
    prompt: &mut dyn super::Prompter,
) -> EdpCliResult<BackedUpPreparedProvision<'a>> {
    let expected_onlyid = prepared.source_backup_onlyid()?;
    let backup = super::write::backup_create_on_disk(
        runner,
        prepared.disk(),
        backup_dir,
        prompt,
        expected_onlyid.as_deref(),
        Some(prepared.device_id()),
    )?;
    let sha256 =
        verify_mandatory_backup_pin(&backup, prepared.before_pin(), prepared.source_metadata()?)?;
    Ok(BackedUpPreparedProvision {
        prepared,
        backup,
        sha256,
    })
}

fn commit_provision_on_disk_with_progress(
    runner: &dyn CmdRunner,
    backed_up: &BackedUpPreparedProvision<'_>,
    progress: &mut dyn FnMut(
        crate::application::progress::Phase,
        crate::application::progress::Step,
        Option<diskio::TransactionActivity>,
    ),
) -> EdpCliResult<ProvisionCommitOutcome> {
    let prepared = backed_up.prepared;
    let fresh_sha256 = verify_mandatory_backup_pin(
        &backed_up.backup,
        prepared.before_pin(),
        prepared.source_metadata()?,
    )?;
    if fresh_sha256 != backed_up.sha256 {
        return Err(err(EXIT_TARGET, "错误: 已授权备份内容发生变化，禁止提交"));
    }
    let mut dev = open_readonly_usb_disk(runner, prepared.disk())?;
    commit::commit_provision_with_progress(runner, &mut dev, prepared, progress)
}

#[cfg(test)]
fn run_mandatory_backup_before_commit<B, C>(
    backup: B,
    commit: C,
) -> EdpCliResult<ProvisionWriteOutcome>
where
    B: FnOnce() -> EdpCliResult<super::post_restore::MetadataBackupReport>,
    C: FnOnce() -> EdpCliResult<ProvisionCommitOutcome>,
{
    let backup = backup()?;
    let commit = commit()?;
    Ok(ProvisionWriteOutcome {
        backup,
        commit,
        warnings: Vec::new(),
    })
}

fn verify_mandatory_backup_pin(
    report: &super::post_restore::MetadataBackupReport,
    pin: &MediaIdentityPin,
    source_metadata: &[u8],
) -> EdpCliResult<String> {
    let reader = crate::edpb::VerifiedBackupReader::open(&report.path).map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 强制备份 EDPB 校验失败: {message}"),
        )
    })?;
    let verified = reader.verified();
    let identity =
        crate::edpb::canonical_media_identity(&verified.manifest).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 强制备份 canonical identity 无效: {message}"),
            )
        })?;
    pin.verify(&identity, source_metadata).map_err(|conflict| {
        err(
            EXIT_TARGET,
            format!("错误: 强制备份与制盘准备阶段介质身份不一致: {conflict:?}"),
        )
    })?;
    let artifact_id = if report.edp_protocol_saved {
        crate::edpb::RAW_PROTOCOL_ARTIFACT_ID
    } else {
        "raw.plain.partition_table.0"
    };
    let raw = reader.read_artifact(artifact_id).map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 强制备份来源快照不可读: {message}"),
        )
    })?;
    let expected = if report.edp_protocol_saved {
        source_metadata
    } else {
        source_metadata
            .get(..SECTOR)
            .ok_or_else(|| err(EXIT_TARGET, "错误: 制盘准备阶段 Plain MBR 快照长度异常"))?
    };
    if raw != expected {
        return Err(err(
            EXIT_TARGET,
            "错误: 强制备份元数据与制盘准备阶段快照不一致",
        ));
    }
    Ok(verified.file_sha256.clone())
}

fn record_lineage_after_commit(
    runner: &dyn CmdRunner,
    prepared: &PreparedProvision,
    backup_dir: &Path,
    backup: &super::post_restore::MetadataBackupReport,
    backup_sha256: String,
) -> Result<PathBuf, ProvisionWarning> {
    let mut dev = open_readonly_usb_disk(runner, prepared.disk())
        .map_err(|error| ProvisionWarning::AfterIdentityObservationFailed(error.msg))?;
    let after = super::media_identity_observer::observe_media_identity_readonly(
        runner,
        prepared.disk(),
        &mut dev,
    )
    .map_err(|error| ProvisionWarning::AfterIdentityObservationFailed(error.msg))?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|error| {
        ProvisionWarning::HostLineagePersistenceFailed(format!("transaction id: {error}"))
    })?;
    let transaction_id = crate::common::hex_lower(&random);
    let epoch_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            ProvisionWarning::HostLineagePersistenceFailed(format!("system clock: {error}"))
        })?
        .as_secs();
    let created_epoch = i64::try_from(epoch_seconds).map_err(|error| {
        ProvisionWarning::HostLineagePersistenceFailed(format!("system clock range: {error}"))
    })?;
    let operation = match prepared.target() {
        ProvisionTarget::Plain => "Plain".to_string(),
        ProvisionTarget::Official(mode) => format!("Official::{mode:?}"),
    };
    let record = identity_lineage::IdentityTransitionRecord {
        schema: identity_lineage::SCHEMA.into(),
        transaction_id,
        created_epoch,
        operation,
        before_identity: prepared.before_pin().snapshot.clone(),
        after_identity: after.snapshot,
        mandatory_backup_path: backup.path.clone(),
        mandatory_backup_sha256: backup_sha256,
        provision_summary_digest: crate::sha256::sha256_hex(&after.protocol_image),
    };
    identity_lineage::persist(backup_dir, &record)
        .map_err(ProvisionWarning::HostLineagePersistenceFailed)
}

/// Mandatory provisioning safety chain shared by CLI and TUI.
///
/// A metadata EDPB backup of the currently selected physical USB must complete
/// successfully before the write path is allowed to enter unmount/lock. The
/// backup is bound to the prepared source identity and a backup failure aborts
/// the operation without reaching `commit_provision_on_disk`.
pub fn commit_provision_with_backup_on_disk(
    runner: &dyn CmdRunner,
    prepared: &PreparedProvision,
    backup_dir: PathBuf,
    prompt: &mut dyn super::Prompter,
) -> EdpCliResult<ProvisionWriteOutcome> {
    commit_provision_with_backup_on_disk_with_progress(
        runner,
        prepared,
        backup_dir,
        prompt,
        &mut |_| {},
    )
}

pub fn commit_provision_with_backup_on_disk_with_progress(
    runner: &dyn CmdRunner,
    prepared: &PreparedProvision,
    backup_dir: PathBuf,
    prompt: &mut dyn super::Prompter,
    sink: &mut dyn FnMut(crate::application::progress::ProgressEvent),
) -> EdpCliResult<ProvisionWriteOutcome> {
    use crate::application::progress::{
        emit_isolated, LogPolicy, Phase, ProgressEvent, Severity, Step,
    };
    if let PreparedProvision::Native(native) = prepared {
        std::fs::create_dir_all(&backup_dir)
            .map_err(|e| err(EXIT_IO, format!("原生WAL备份目录创建失败: {e}")))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let wal = backup_dir.join(format!("native-provision-disk{}-{stamp}.wal", native.disk));
        // Native transactions must publish the same authoritative progress
        // contract used by the original 512B pipeline. Early event covers
        // potentially long identity checks and OS volume lock acquisition.
        emit_isolated(
            sink,
            ProgressEvent::started(
                crate::application::progress::OperationKind::Provision,
                Phase::Identity,
                Step::LockAndReopen,
                "核验介质、卸载并获得原生写租约",
            ),
        );
        let mut high_water_mark = 0u16;
        native_flow::commit_prepared_native_provision_observed(
            runner,
            native,
            &wal,
            &mut |activity| {
                let mut event = progress_projection::commit_event(
                    Phase::Transaction,
                    Step::ProtocolWrite,
                    2,
                    4,
                    Some(activity),
                );
                // WAL preparation and native write-session preflight may
                // interleave; never reverse the overall progress indicator.
                high_water_mark = high_water_mark.max(event.overall.basis_points());
                event.overall = crate::application::progress::OverallProgress::from_basis_points(
                    high_water_mark,
                );
                emit_isolated(sink, event);
            },
        )
        .map_err(|message| err(EXIT_IO, message))?;
        let mut complete = ProgressEvent::new(Phase::Complete, Step::Completed, 4, 4);
        complete.log_policy = LogPolicy::Append;
        complete.detail = Some("原生 WAL 持久化、事务写入及读回验证完成".into());
        emit_isolated(sink, complete);
        // The original native write blocks are retained as a durable
        // rollback WAL, not as a portable EDPB backup. The result UI must
        // identify its actual file type instead of promising an EDPB export.
        let backup = super::post_restore::MetadataBackupReport {
            path: wal,
            partition_count: native.partitions.len(),
            edp_protocol_saved: true,
        };
        let commit = match native.target {
            ProvisionTarget::Plain => ProvisionCommitOutcome::Plain {
                partition_count: native.partitions.len(),
            },
            ProvisionTarget::Official(_) => {
                ProvisionCommitOutcome::Official(ProvisionCommitReport {
                    provision_succeeded: true,
                    formats: vec![],
                })
            }
        };
        return Ok(ProvisionWriteOutcome {
            backup,
            commit,
            warnings: Vec::new(),
        });
    }
    let format_count = match prepared {
        PreparedProvision::Official(official) => official
            .format_targets
            .iter()
            .filter(|choice| choice.selected)
            .count(),
        PreparedProvision::Plain(_) => 0,
        PreparedProvision::Native(native) => {
            native.partitions.iter().filter(|p| p.formatted).count()
        }
    } as u64;
    let total = 5 + format_count;
    let mut current = 0;
    emit_isolated(
        sink,
        ProgressEvent::new(Phase::Backup, Step::MandatoryBackup, current, total),
    );
    let expected_onlyid = prepared.source_backup_onlyid()?;
    let backup = super::write::backup_create_on_disk(
        runner,
        prepared.disk(),
        backup_dir.clone(),
        prompt,
        expected_onlyid.as_deref(),
        Some(prepared.device_id()),
    )?;
    current += 1;
    emit_isolated(
        sink,
        ProgressEvent::new(Phase::Backup, Step::MandatoryBackup, current, total),
    );
    let backup_sha256 =
        verify_mandatory_backup_pin(&backup, prepared.before_pin(), prepared.source_metadata()?)?;
    let backed_up = BackedUpPreparedProvision {
        prepared,
        backup,
        sha256: backup_sha256.clone(),
    };
    current += 1;
    emit_isolated(
        sink,
        ProgressEvent::new(Phase::Identity, Step::BackupVerification, current, total),
    );
    let commit =
        commit_provision_on_disk_with_progress(runner, &backed_up, &mut |phase, step, work| {
            if work.is_none() && step != Step::LockAndReopen {
                current += 1;
            }
            let event = progress_projection::commit_event(phase, step, current, total, work);
            emit_isolated(sink, event);
        })?;
    let mut outcome = ProvisionWriteOutcome {
        backup: backed_up.backup,
        commit,
        warnings: Vec::new(),
    };
    if matches!(&outcome.commit, ProvisionCommitOutcome::Official(report) if report.formats.iter().any(|format| format.result.is_err()))
    {
        outcome.warnings.push(ProvisionWarning::IncompleteFormat);
        let mut complete = ProgressEvent::new(Phase::Complete, Step::Completed, total, total);
        let unsafe_state = matches!(
            outcome.execution_status(),
            ProvisionExecutionStatus::MediaIntermediate
                | ProvisionExecutionStatus::MediaStateUnknown
        );
        complete.severity = if unsafe_state {
            Severity::Error
        } else {
            Severity::Warning
        };
        complete.log_policy = LogPolicy::Append;
        complete.detail = Some(
            if unsafe_state {
                "协议已写入；格式化或回滚后介质状态未安全确认，已停止后续写入，请重新检查设备。"
            } else {
                "协议已写入；至少一个分区格式化失败，后续分区未执行。"
            }
            .into(),
        );
        emit_isolated(sink, complete);
        return Ok(outcome);
    }
    if let Err(warning) = record_lineage_after_commit(
        runner,
        prepared,
        &backup_dir,
        &outcome.backup,
        backup_sha256,
    ) {
        outcome.warnings.push(warning);
    }
    current += 1;
    emit_isolated(
        sink,
        ProgressEvent::new(Phase::Lineage, Step::PostWriteIdentity, current, total),
    );
    let mut complete = ProgressEvent::new(Phase::Complete, Step::Completed, total, total);
    complete.log_policy = LogPolicy::Append;
    complete.detail = Some("制盘全部步骤完成".into());
    emit_isolated(sink, complete);
    Ok(outcome)
}

#[cfg(test)]
use commit::{
    execute_partition_format, validate_preserve_source_snapshot, verify_format_hardware,
    verify_lce_readback, verify_protocol_readback,
};
use commit::{validate_key_disposition_plan, validate_target_write_set};

#[cfg(test)]
mod tests;

pub mod password;
pub mod preflight;
pub mod result_model;

// Offline regular-file-only native block image production.
pub mod native_commit;
pub mod native_flow;
pub mod native_image;
pub mod native_preflight;
pub mod native_virtual_transition;
