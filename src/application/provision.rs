//! Shared provisioning application service.
//!
//! This layer resolves a real USB target, creates the exact pure-domain write
//! plan, and owns the final raw-device safety transition. CLI/TUI must not
//! duplicate these checks or construct alternative raw-write patches.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

use crate::backup_deep::{analyze_partition, AnalysisStatus, PartitionReader};
use crate::backup_metadata::{parse_lba7_compatibility_geometry, PartitionGeometry};
use crate::common::{EdpCliError, EdpCliResult, EXIT_IO, EXIT_TARGET, SECTOR};
use crate::diskio::{self, SectorDev};
use crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity;
use crate::provision::{
    apply_target_geometry_overrides, build_empty_exfat, build_empty_fat16,
    build_official_partition_filesystem, build_official_provision_protocol_image,
    build_plain_provision_write_plan, parse_existing_provision, prefill_for_target_mode,
    unwrap_legacy_lba7_file_key, wrap_file_key, wrap_legacy_lba7_file_key, CapacityInput,
    CapacitySource, FileKeyWrapMode,
    KeyDomainRole, KeyDomainSecrets, OfficialFilesystemFormat, OfficialPartitionFilesystems,
    OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan,
    OfficialProvisionWriteImage, OnlyId, ParsedExistingProvision, PartitionAction,
    PartitionFilesystemImage, PartitionFormatTarget, PartitionRole, PassInfoPolicy,
    RegionDisposition,
    PlainCleanupExtent, PlainPartitionSpec, PlainProvisionPlan, PlainProvisionWritePlan,
    ProvisionEntropy, ProvisionImage, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
    ProvisionTarget, QuickCapacityUnit, SourcePasswordKnowledge, SparseFilesystemImage,
    TargetGeometryOverrides, TargetIdentity, TargetPasswordPolicy, TargetProvisionPlan,
    DEFAULT_KEY_DOMAIN_PASSWORD,
    DEFAULT_MODE0_BOOT_SECTORS,
};
use crate::sysinfo::{self, CmdRunner};
use encoding_rs::GBK;

use super::device::open_readonly_usb_disk;
use super::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use super::write::{read_image, verify_reopened_snapshot};

const OPEN_WAIT: Duration = Duration::from_secs(10);

fn err(code: i32, message: impl Into<String>) -> EdpCliError {
    EdpCliError::new(code, message)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficialProvisionRequest {
    pub target: ProvisionTarget,
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
    pub key_domains: KeyDomainSecrets,
    pub volume_label: String,
    pub format: FormatOptions,
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
    fn sectors(self) -> Result<Option<u64>, String> {
        match self {
            Self::Sectors(value) => Ok(Some(value)),
            Self::MiB(value) => value
                .checked_mul(1024 * 1024 / SECTOR as u64)
                .map(Some)
                .ok_or_else(|| "普通分区 MiB 容量溢出".to_string()),
            Self::GiB(value) => value
                .checked_mul(1024 * 1024 * 1024 / SECTOR as u64)
                .map(Some)
                .ok_or_else(|| "普通分区 GiB 容量溢出".to_string()),
            Self::Fill => Ok(None),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainPartitionRequest {
    pub start_lba: u64,
    pub size: PlainPartitionSize,
    pub filesystem: OfficialFilesystemFormat,
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

    pub fn resolve(&self, total_sectors: u64) -> Result<PlainProvisionPlan, String> {
        if self.partitions.is_empty() {
            return PlainProvisionPlan::default_for_disk(total_sectors);
        }
        if self.partitions.len() > crate::provision::MAX_PLAIN_PARTITIONS {
            return Err(format!(
                "普通盘最多支持 {} 个 MBR 主分区",
                crate::provision::MAX_PLAIN_PARTITIONS
            ));
        }

        let mut partitions = Vec::with_capacity(self.partitions.len());
        for (index, request) in self.partitions.iter().enumerate() {
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
            let sector_count = match request.size.sectors()? {
                Some(value) => value,
                None => next_start
                    .checked_sub(request.start_lba)
                    .filter(|value| *value > 0)
                    .ok_or_else(|| format!("P{} fill 后没有可用空间", index.saturating_add(1)))?,
            };
            partitions.push(PlainPartitionSpec::new(
                request.start_lba,
                sector_count,
                request.filesystem,
                request.volume_label.clone(),
            ));
        }
        PlainProvisionPlan::new(total_sectors, partitions)
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
    pub boot_fs: OfficialFilesystemFormat,
    pub share_fs: OfficialFilesystemFormat,
    pub encrypt_fs: OfficialFilesystemFormat,
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
            boot_fs: OfficialFilesystemFormat::Fat16,
            share_fs: OfficialFilesystemFormat::ExFat,
            encrypt_fs: OfficialFilesystemFormat::ExFat,
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
            PartitionRole::Share | PartitionRole::BootShareCombined => {
                (self.share, &self.share_label)
            }
            PartitionRole::Encrypt => (self.encrypt, &self.encrypt_label),
            PartitionRole::CompatibilityReserve => (false, ""),
        }
    }
}

fn build_plain_format_image(
    target: &PartitionFormatTarget,
    filesystem: OfficialFilesystemFormat,
    volume_label: &str,
    volume_serial: u32,
) -> Result<SparseFilesystemImage, String> {
    match filesystem {
        OfficialFilesystemFormat::Fat16 => build_empty_fat16(
            target.geometry.start_sector,
            target.geometry.sector_count(),
            volume_serial,
            volume_label,
        ),
        OfficialFilesystemFormat::ExFat => build_empty_exfat(
            target.geometry.start_sector,
            target.geometry.sector_count(),
            volume_serial,
            volume_label,
        ),
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => Err(format!(
            "portable filesystem writer does not yet implement {}",
            filesystem.config_token()
        )),
    }
}

pub fn plan_format_targets(
    plan: &OfficialProvisionPlan,
    options: &FormatOptions,
    serials: &[u32],
    file_key: &[u8; 16],
) -> Result<Vec<PlannedPartitionFormat>, String> {
    let keys = vec![*file_key; serials.len()];
    plan_format_targets_with_keys(plan, options, serials, &keys)
}

fn plan_format_targets_with_keys(
    plan: &OfficialProvisionPlan,
    options: &FormatOptions,
    serials: &[u32],
    file_keys: &[[u8; 16]],
) -> Result<Vec<PlannedPartitionFormat>, String> {
    let targets = plan.format_targets()?;
    if targets.len() != serials.len() || targets.len() != file_keys.len() {
        return Err("format serial/key count does not match partition count".into());
    }
    if options.boot
        && !targets
            .iter()
            .any(|t| t.format_capable && t.role == PartitionRole::Boot)
        || options.share
            && !targets.iter().any(|t| {
                t.format_capable
                    && matches!(
                        t.role,
                        PartitionRole::Share | PartitionRole::BootShareCombined
                    )
            })
        || options.encrypt
            && !targets
                .iter()
                .any(|t| t.format_capable && t.role == PartitionRole::Encrypt)
    {
        return Err("当前模式不包含所选的可格式化分区".into());
    }
    let mut planned = targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| {
            let (selected, label) = options.choice(target.role);
            PlannedPartitionFormat {
                target,
                selected,
                filesystem: target.filesystem,
                volume_label: label.to_string(),
                volume_serial: serials[index],
                prepared_image: None,
                verification_image: None,
            }
        })
        .collect::<Vec<_>>();
    for choice in planned.iter().filter(|choice| choice.target.format_capable) {
        if !matches!(
            choice.filesystem,
            Some(OfficialFilesystemFormat::Fat16 | OfficialFilesystemFormat::ExFat)
        ) {
            return Err(format!(
                "{} 文件系统尚无可验证的写入实现",
                choice.target.role.label()
            ));
        }
    }
    for (index, choice) in planned
        .iter_mut()
        .enumerate()
        .filter(|(_, choice)| choice.selected)
    {
        let filesystem = choice
            .filesystem
            .ok_or("compatibility reserve is not a filesystem")?;
        let verification_image = build_plain_format_image(
            &choice.target,
            filesystem,
            &choice.volume_label,
            choice.volume_serial,
        )
        .map_err(|message| format!("{} 格式化计划无效: {message}", choice.target.role.label()))?;
        let prepared_image = build_official_partition_filesystem(
            plan,
            &choice.target,
            &file_keys[index],
            &choice.volume_label,
            choice.volume_serial,
        )
        .map_err(|message| format!("{} 格式化计划无效: {message}", choice.target.role.label()))?;
        if !choice.target.physically_encrypted && prepared_image.image != verification_image {
            return Err(format!(
                "{} 明文格式化镜像与验证镜像不一致",
                choice.target.role.label()
            ));
        }
        choice.prepared_image = Some(prepared_image);
        choice.verification_image = Some(verification_image);
    }
    Ok(planned)
}

#[derive(Clone, Eq, PartialEq)]
pub struct PlannedPartitionFormat {
    pub target: PartitionFormatTarget,
    pub selected: bool,
    pub filesystem: Option<OfficialFilesystemFormat>,
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
    pub result: Result<(), String>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProvisionKeyProbe {
    pub source_kind: crate::provision::DiskProvisionKind,
    pub share: Option<SourcePasswordKnowledge>,
    pub encrypt: Option<SourcePasswordKnowledge>,
}


#[derive(Clone, Eq, PartialEq)]
pub struct PreparedNewProvision {
    pub disk: u32,
    pub device_id: String,
    pub mode: OfficialPartitionMode,
    pub force_change_password: bool,
    pub pass_info_policy: PassInfoPolicy,
    pub lce_start_lba: u64,
    pub write_image: OfficialProvisionWriteImage,
    pub format_targets: Vec<PlannedPartitionFormat>,
    pub target_plan: Option<TargetProvisionPlan>,
    source_metadata: Option<Vec<u8>>,
    plan: OfficialProvisionPlan,
    expected_onlyid: String,
    expected_serial: Option<String>,
    expected_probe: crate::platform::HardwareProbe,
    expected_lba3: Option<[u8; SECTOR]>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PreparedPlainProvision {
    pub disk: u32,
    pub device_id: String,
    pub plan: PlainProvisionPlan,
    pub write_plan: PlainProvisionWritePlan,
    pub source_kind: crate::provision::DiskProvisionKind,
    pub source_lce_start_lba: Option<u64>,
    source_metadata: Vec<u8>,
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
            .field("mode", &self.mode)
            .field("force_change_password", &self.force_change_password)
            .field("pass_info_policy", &self.pass_info_policy)
            .field("lce_start_lba", &self.lce_start_lba)
            .field("write_image", &self.write_image)
            .field("format_targets", &self.format_targets)
            .field("target_plan", &self.target_plan)
            .field("source_metadata_captured", &self.source_metadata.is_some())
            .field("plan", &self.plan)
            .field("expected_onlyid", &self.expected_onlyid)
            .field("expected_serial", &self.expected_serial)
            .field("expected_probe", &self.expected_probe)
            .field("expected_lba3", &self.expected_lba3)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedProvision {
    Official(Box<PreparedNewProvision>),
    Plain(Box<PreparedPlainProvision>),
}

impl PreparedProvision {
    pub const fn target(&self) -> ProvisionTarget {
        match self {
            Self::Official(prepared) => ProvisionTarget::Official(prepared.mode),
            Self::Plain(_) => ProvisionTarget::Plain,
        }
    }

    pub const fn disk(&self) -> u32 {
        match self {
            Self::Official(prepared) => prepared.disk,
            Self::Plain(prepared) => prepared.disk,
        }
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
    if request.boot_mib.is_some() && request.boot_sectors.is_some() {
        return Err(err(
            EXIT_TARGET,
            "错误: 启动区不能同时指定 MiB 和精确扇区数",
        ));
    }
    if request.share_mib.is_some() && request.share_sectors.is_some() {
        return Err(err(
            EXIT_TARGET,
            "错误: 交换区不能同时指定 MiB 和精确扇区数",
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
            return Err(err(EXIT_TARGET, "错误: 交换区扇区数必须大于 0"));
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
mod export;
mod prepare;

pub use commit::{
    capture_manufacturer_lba3, commit_new_provision, commit_plain_provision, commit_provision,
};
pub use export::{
    export_provision_image, export_sparse_plain_provision_image, export_sparse_provision_image,
};
#[cfg(test)]
use prepare::target_encrypt_capacity_override;
pub use prepare::{
    prepare_plain_provision, prepare_provision, prepare_target_provision,
    probe_provision_key_domains_on_disk,
};

pub fn prepare_provision_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &ProvisionRequest,
) -> EdpCliResult<PreparedProvision> {
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    prepare_provision(runner, disk, request, &mut dev)
}

pub fn commit_provision_on_disk(
    runner: &dyn CmdRunner,
    prepared: &PreparedProvision,
) -> EdpCliResult<ProvisionCommitOutcome> {
    let mut dev = open_readonly_usb_disk(runner, prepared.disk())?;
    commit_provision(runner, &mut dev, prepared)
}

use commit::{validate_key_disposition_plan, validate_target_write_set};
#[cfg(test)]
use commit::{
    execute_partition_format, validate_preserve_source_snapshot, verify_format_hardware,
    verify_protocol_readback,
};

#[cfg(test)]
mod tests;
