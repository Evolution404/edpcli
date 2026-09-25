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
    wrap_file_key, wrap_legacy_lba7_file_key, CapacityInput, CapacitySource, FileKeyWrapMode,
    OfficialFilesystemFormat, OfficialPartitionFilesystems, OfficialPartitionMode,
    OfficialPartitionSizes, OfficialProvisionPlan, OfficialProvisionWriteImage, OnlyId,
    ParsedExistingProvision, PartitionAction, PartitionFilesystemImage, PartitionFormatTarget,
    PartitionRole, PassInfoPolicy, PlainCleanupExtent, PlainPartitionSpec, PlainProvisionPlan,
    PlainProvisionWritePlan, ProvisionEntropy, ProvisionImage, ProvisionMetadata, ProvisionProfile,
    ProvisionSpec, ProvisionTarget, QuickCapacityUnit, SparseFilesystemImage,
    TargetGeometryOverrides, TargetIdentity, TargetProvisionPlan, DEFAULT_MODE0_BOOT_SECTORS,
};
use crate::sysinfo::{self, CmdRunner};
use encoding_rs::GBK;

use super::device::guard_usb_disk;
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
    pub password: String,
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
    Official(OfficialProvisionRequest),
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
pub use prepare::{prepare_plain_provision, prepare_provision, prepare_target_provision};

use commit::validate_target_write_set;
#[cfg(test)]
use commit::{
    execute_partition_format, validate_preserve_source_snapshot, verify_format_hardware,
    verify_protocol_readback,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct Lba3Dev([u8; SECTOR]);

    impl SectorDev for Lba3Dev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            if lba != 3 {
                return Err(io::Error::other("unexpected read"));
            }
            Ok(self.0.to_vec())
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Err(io::Error::other("read-only test device"))
        }
    }

    #[test]
    fn mode2_quick_and_exact_encrypt_capacity_are_partition_scoped() {
        let quick = target_encrypt_capacity_override(
            OfficialPartitionMode::WholeDiskEncrypted,
            Some(128),
            None,
        )
        .unwrap()
        .unwrap();
        let exact = target_encrypt_capacity_override(
            OfficialPartitionMode::WholeDiskEncrypted,
            None,
            Some(128 * 2048),
        )
        .unwrap()
        .unwrap();
        assert_eq!(quick.sectors(), 128 * 2048);
        assert_eq!(exact.sectors(), 128 * 2048);
        assert_eq!(quick.sectors(), exact.sectors());
    }

    #[test]
    fn preserve_requires_a_full_prewrite_source_metadata_snapshot() {
        assert!(validate_preserve_source_snapshot(false, None).is_ok());
        assert!(validate_preserve_source_snapshot(true, None).is_err());
        assert!(validate_preserve_source_snapshot(true, Some(&vec![0; 12 * SECTOR])).is_err());
        assert!(validate_preserve_source_snapshot(true, Some(&vec![0; 13 * SECTOR])).is_ok());
    }

    #[test]
    fn manufacturer_lba3_is_copied_verbatim_into_the_write_plan() {
        let metadata = ProvisionImage::from_bytes(vec![0; 13 * SECTOR]).unwrap();
        let mut patch = BTreeMap::new();
        patch.insert(3, vec![0; SECTOR]);
        let probe = crate::platform::HardwareProbe {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            transport: crate::platform::NativeTransport::Uas,
            inquiry: None,
        };
        let mut prepared = PreparedNewProvision {
            disk: 4,
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            mode: OfficialPartitionMode::BootShareCombined,
            force_change_password: false,
            pass_info_policy: PassInfoPolicy::default(),
            lce_start_lba: 900,
            write_image: OfficialProvisionWriteImage {
                metadata,
                total_sectors: 1024,
                patch,
            },
            format_targets: Vec::new(),
            target_plan: None,
            source_metadata: None,
            plan: OfficialProvisionPlan::new(
                OfficialPartitionMode::BootShareCombined,
                OfficialPartitionSizes::new(32, 64, 128),
                crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
                    chs_bytes: 0,
                    start_byte_offset: 0,
                    start_lba: 900,
                    size_bytes: 3072,
                    size_sectors: 6,
                },
                wrap_legacy_lba7_file_key(b"0000aaaa", [0; 8]),
                wrap_file_key(b"0000aaaa", [0; 16], FileKeyWrapMode::Sm4),
            )
            .unwrap(),
            expected_onlyid: "1".into(),
            expected_serial: None,
            expected_probe: probe,
            expected_lba3: None,
        };
        let expected = [0xa5; SECTOR];
        let mut dev = Lba3Dev(expected);
        capture_manufacturer_lba3(&mut dev, &mut prepared).unwrap();
        assert_eq!(prepared.write_image.patch.get(&3).unwrap(), &expected);
        assert_eq!(prepared.expected_lba3, Some(expected));
    }

    #[derive(Default)]
    struct MemoryDev {
        sectors: BTreeMap<u32, Vec<u8>>,
        fail_at: Option<u32>,
    }

    impl SectorDev for MemoryDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            Ok(self
                .sectors
                .get(&lba)
                .cloned()
                .unwrap_or_else(|| vec![0; SECTOR]))
        }
        fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
            if self.fail_at == Some(lba) {
                return Err(io::Error::other("injected format failure"));
            }
            self.sectors.insert(lba, data.to_vec());
            Ok(())
        }
    }

    #[test]
    fn plain_prewrite_snapshot_rejects_stale_lba7_metadata() {
        let total_sectors = 100_000;
        let plan = PlainProvisionPlan::default_for_disk(total_sectors).unwrap();
        let write_plan = build_plain_provision_write_plan(&plan, None, &[0x1234_5678]).unwrap();
        let probe = crate::platform::HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: crate::platform::NativeTransport::Uas,
            inquiry: None,
        };
        let mut source_metadata = vec![0u8; 13 * SECTOR];
        source_metadata[3 * SECTOR..4 * SECTOR].fill(0xa5);
        let prepared = PreparedPlainProvision {
            disk: 4,
            device_id: "disk&ven_aigo&prod_u335".into(),
            plan,
            write_plan,
            source_kind: crate::provision::DiskProvisionKind::Plain,
            source_lce_start_lba: None,
            source_metadata: source_metadata.clone(),
            expected_probe: probe,
        };
        let mut dev = MemoryDev::default();
        for lba in 0..13u32 {
            dev.sectors.insert(
                lba,
                source_metadata[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec(),
            );
        }

        verify_reopened_snapshot(&mut dev, &prepared.source_metadata).unwrap();
        dev.sectors.get_mut(&7).unwrap()[0] ^= 1;
        let error = verify_reopened_snapshot(&mut dev, &prepared.source_metadata).unwrap_err();
        assert!(error.msg.contains("LBA7"));
    }

    fn format_test_plan(mode: OfficialPartitionMode, key: &[u8; 16]) -> OfficialProvisionPlan {
        OfficialProvisionPlan::new(
            mode,
            OfficialPartitionSizes::new(32, 64, 128),
            crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
                chs_bytes: 0,
                start_byte_offset: 4_194_000 * SECTOR as u64,
                start_lba: 4_194_000,
                size_bytes: 3072,
                size_sectors: 6,
            },
            wrap_legacy_lba7_file_key(b"0000aaaa", [0; 8]),
            wrap_file_key(b"0000aaaa", *key, FileKeyWrapMode::Sm4),
        )
        .unwrap()
    }

    #[test]
    fn format_executor_uses_the_same_matrix_and_preserves_protocol_sectors() {
        let key = [0x42; 16];
        for mode in [
            OfficialPartitionMode::DefaultThreePartition,
            OfficialPartitionMode::BootShareCombined,
            OfficialPartitionMode::WholeDiskEncrypted,
            OfficialPartitionMode::IntranetExtranetDualPartition,
        ] {
            let plan = format_test_plan(mode, &key);
            let serials = vec![0x1234_5678; plan.format_targets().unwrap().len()];
            let options = FormatOptions {
                boot: mode != OfficialPartitionMode::WholeDiskEncrypted
                    && mode != OfficialPartitionMode::BootShareCombined,
                share: mode != OfficialPartitionMode::WholeDiskEncrypted,
                encrypt: mode != OfficialPartitionMode::IntranetExtranetDualPartition,
                ..FormatOptions::default()
            };
            let choices = plan_format_targets(&plan, &options, &serials, &key).unwrap();
            let mut dev = MemoryDev::default();
            for lba in 0..13u32 {
                dev.sectors.insert(lba, vec![lba as u8; SECTOR]);
            }
            for choice in choices.iter().filter(|choice| choice.selected) {
                execute_partition_format(&mut dev, choice).unwrap();
                let raw = dev
                    .read_sector(choice.target.geometry.start_sector as u32)
                    .unwrap();
                if choice.target.physically_encrypted {
                    assert_ne!(raw.get(3..11), Some(&b"EXFAT   "[..]));
                    assert_ne!(raw.get(54..62), Some(&b"FAT16   "[..]));
                } else if choice.filesystem == Some(OfficialFilesystemFormat::Fat16) {
                    assert_eq!(raw.get(54..62), Some(&b"FAT16   "[..]));
                } else {
                    assert_eq!(raw.get(3..11), Some(&b"EXFAT   "[..]));
                }
            }
            for lba in 0..13u32 {
                assert_eq!(dev.read_sector(lba).unwrap(), vec![lba as u8; SECTOR]);
            }
            if mode == OfficialPartitionMode::WholeDiskEncrypted {
                assert!(!dev.sectors.contains_key(&63));
            }
        }
    }

    #[test]
    fn format_failure_keeps_the_protocol_and_prior_successful_partition() {
        let key = [0x42; 16];
        let plan = format_test_plan(OfficialPartitionMode::DefaultThreePartition, &key);
        let choices = plan_format_targets(
            &plan,
            &FormatOptions {
                boot: true,
                share: true,
                ..FormatOptions::default()
            },
            &[1, 2, 3],
            &key,
        )
        .unwrap();
        let mut dev = MemoryDev::default();
        for lba in 0..13u32 {
            dev.sectors.insert(lba, vec![0xa5; SECTOR]);
        }
        execute_partition_format(&mut dev, &choices[0]).unwrap();
        dev.fail_at = Some(choices[1].target.geometry.start_sector as u32);
        assert!(execute_partition_format(&mut dev, &choices[1]).is_err());
        assert_eq!(
            &dev.read_sector(choices[0].target.geometry.start_sector as u32)
                .unwrap()[54..62],
            b"FAT16   "
        );
        for lba in 0..13u32 {
            assert_eq!(dev.read_sector(lba).unwrap(), vec![0xa5; SECTOR]);
        }
    }

    #[test]
    fn sparse_export_includes_selected_format_images() {
        let key = [0x42; 16];
        let plan = format_test_plan(OfficialPartitionMode::DefaultThreePartition, &key);
        let choices = plan_format_targets(
            &plan,
            &FormatOptions {
                boot: true,
                ..FormatOptions::default()
            },
            &[0x1234_5678, 2, 3],
            &key,
        )
        .unwrap();
        let mut patch = BTreeMap::new();
        patch.insert(0, vec![0x5a; SECTOR]);
        let probe = crate::platform::HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: crate::platform::NativeTransport::Uas,
            inquiry: None,
        };
        let prepared = PreparedNewProvision {
            disk: 4,
            device_id: "disk&ven_aigo&prod_u335".into(),
            mode: plan.mode,
            force_change_password: false,
            pass_info_policy: PassInfoPolicy::default(),
            lce_start_lba: plan.lba7_compatibility_extent.start_lba,
            write_image: OfficialProvisionWriteImage {
                metadata: ProvisionImage::from_bytes(vec![0; 13 * SECTOR]).unwrap(),
                total_sectors: 1_000_000,
                patch,
            },
            format_targets: choices,
            target_plan: None,
            source_metadata: None,
            plan,
            expected_onlyid: "1".into(),
            expected_serial: None,
            expected_probe: probe,
            expected_lba3: None,
        };
        let path = std::env::temp_dir().join(format!(
            "edpcli-provision-export-{}-{}.img",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_file(&path);
        export_sparse_provision_image(&path, &prepared).unwrap();
        let mut file = std::fs::File::open(&path).unwrap();
        use std::io::{Read, Seek};
        let mut mbr = [0u8; SECTOR];
        file.read_exact(&mut mbr).unwrap();
        assert_eq!(mbr, [0x5a; SECTOR]);
        file.seek(SeekFrom::Start(63 * SECTOR as u64)).unwrap();
        let mut boot = [0u8; SECTOR];
        file.read_exact(&mut boot).unwrap();
        assert_eq!(&boot[54..62], b"FAT16   ");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn format_hardware_gate_rejects_changed_serial_probe_capacity_and_device_id() {
        let probe = crate::platform::HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: crate::platform::NativeTransport::Uas,
            inquiry: Some(crate::platform::InquiryInfo {
                vendor: "aigo".into(),
                product: "U335".into(),
                revision: "PMAP".into(),
            }),
        };
        let total = 16_777_216;
        let device_id = TargetIdentity::from_probe(&probe, total)
            .unwrap()
            .device_id()
            .to_string();
        let check =
            |fresh: &crate::platform::HardwareProbe, capacity, serial: Option<&str>, id: &str| {
                verify_format_hardware(&probe, total, id, Some("SERIAL-1"), fresh, capacity, serial)
            };
        assert!(check(&probe, total, Some("SERIAL-1"), &device_id).is_ok());
        assert!(check(&probe, total, Some("SERIAL-2"), &device_id).is_err());
        assert!(check(&probe, total, None, &device_id).is_err());
        assert!(check(&probe, total + 1, Some("SERIAL-1"), &device_id).is_err());
        assert!(check(&probe, total, Some("SERIAL-1"), "disk&ven_other&prod_other").is_err());
        let mut changed = probe.clone();
        changed.vid = Some(0x0951);
        assert!(check(&changed, total, Some("SERIAL-1"), &device_id).is_err());
        let mut changed = probe.clone();
        changed.inquiry.as_mut().unwrap().revision = "DIFF".into();
        assert!(check(&changed, total, Some("SERIAL-1"), &device_id).is_err());
    }

    #[test]
    fn protocol_readback_gate_rejects_changed_onlyid_and_layout() {
        let total = 16_777_216u64;
        let probe = crate::platform::HardwareProbe {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            transport: crate::platform::NativeTransport::Uas,
            inquiry: Some(crate::platform::InquiryInfo {
                vendor: "Netac".into(),
                product: "OnlyDisk".into(),
                revision: "1.00".into(),
            }),
        };
        let target = TargetIdentity::from_probe(&probe, total).unwrap();
        let device_id = target.device_id().to_string();
        let spec = ProvisionSpec::new(
            target,
            ProvisionMetadata::new(
                OnlyId::parse("1402259934").unwrap(),
                "USER06",
                "江苏省电力有限公司",
                "江苏电力!SAFE6",
            )
            .unwrap(),
            ProvisionProfile::canonical_v1(),
        )
        .unwrap();
        let plan = OfficialProvisionPlan::new(
            OfficialPartitionMode::DefaultThreePartition,
            OfficialPartitionSizes::new(32, 64, 128),
            locate_lba7_compatibility_extent_from_verified_usb_capacity(total, 512).unwrap(),
            wrap_legacy_lba7_file_key(b"0000aaaa", [0x7d; 8]),
            wrap_file_key(b"0000aaaa", [0x42; 16], FileKeyWrapMode::Sm4),
        )
        .unwrap();
        let write_image = build_official_provision_protocol_image(
            &spec,
            &ProvisionEntropy::new([0x5a; 252]),
            &plan,
        )
        .unwrap();
        let mut dev = MemoryDev {
            sectors: write_image.patch.clone(),
            fail_at: None,
        };
        let prepared = PreparedNewProvision {
            disk: 4,
            device_id,
            mode: plan.mode,
            force_change_password: false,
            pass_info_policy: PassInfoPolicy::default(),
            lce_start_lba: plan.lba7_compatibility_extent.start_lba,
            write_image,
            format_targets: vec![],
            target_plan: None,
            source_metadata: None,
            plan,
            expected_onlyid: "1402259934".into(),
            expected_serial: None,
            expected_probe: probe,
            expected_lba3: Some([0; SECTOR]),
        };
        verify_protocol_readback(&mut dev, &prepared).unwrap();
        dev.sectors.get_mut(&4).unwrap()[4] ^= 1;
        assert!(verify_protocol_readback(&mut dev, &prepared).is_err());
        dev.sectors
            .insert(4, prepared.write_image.patch[&4].clone());
        dev.sectors.get_mut(&12).unwrap()[0] ^= 1;
        assert!(verify_protocol_readback(&mut dev, &prepared).is_err());
    }
}
