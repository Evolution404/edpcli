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
use crate::backup_metadata::PartitionGeometry;
use crate::common::{EdpCliError, EdpCliResult, EXIT_IO, EXIT_TARGET, SECTOR};
use crate::diskio::{self, SectorDev};
use crate::identify::identify;
use crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity;
use crate::provision::{
    apply_target_geometry_overrides, build_empty_exfat, build_empty_fat16,
    build_official_partition_filesystem, build_official_provision_protocol_image,
    build_passwordless_conversion, is_mode0_source, parse_existing_provision,
    prefill_for_target_mode, wrap_file_key, wrap_legacy_lba7_file_key, CapacityInput,
    CapacitySource, FileKeyWrapMode, OfficialFilesystemFormat, OfficialPartitionFilesystems,
    OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan,
    OfficialProvisionWriteImage, OnlyId, ParsedExistingProvision, PartitionAction,
    PartitionFilesystemImage, PartitionFormatTarget, PartitionRole, PasswordlessConversionImage,
    ProvisionEntropy, ProvisionImage, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
    QuickCapacityUnit, SparseFilesystemImage, TargetGeometryOverrides, TargetIdentity,
    TargetPartitionGeometry, TargetProvisionPlan, DEFAULT_MODE0_BOOT_SECTORS,
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
pub struct NewProvisionRequest {
    pub mode: u8,
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
    pub force_change_password: bool,
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
    for choice in planned.iter().filter(|choice| choice.selected) {
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

#[derive(Clone, Eq, PartialEq)]
pub struct PreparedNewProvision {
    pub disk: u32,
    pub device_id: String,
    pub mode: OfficialPartitionMode,
    pub force_change_password: bool,
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

impl std::fmt::Debug for PreparedNewProvision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedNewProvision")
            .field("disk", &self.disk)
            .field("device_id", &self.device_id)
            .field("mode", &self.mode)
            .field("force_change_password", &self.force_change_password)
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
pub struct PreparedPasswordlessConversion {
    pub disk: u32,
    pub device_id: String,
    pub source_metadata: Vec<u8>,
    pub conversion: PasswordlessConversionImage,
    pub patch: BTreeMap<u32, Vec<u8>>,
}

fn random_array<const N: usize>() -> EdpCliResult<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| err(EXIT_IO, format!("错误: 系统随机数生成失败: {error}")))?;
    Ok(bytes)
}

fn mode(value: u8) -> EdpCliResult<OfficialPartitionMode> {
    match value {
        0 => Ok(OfficialPartitionMode::DefaultThreePartition),
        1 => Ok(OfficialPartitionMode::BootShareCombined),
        2 => Ok(OfficialPartitionMode::WholeDiskEncrypted),
        3 => Ok(OfficialPartitionMode::IntranetExtranetDualPartition),
        _ => Err(err(
            EXIT_TARGET,
            format!("错误: 不支持的官方制盘模式 {value}"),
        )),
    }
}

fn sizes(request: &NewProvisionRequest) -> EdpCliResult<OfficialPartitionSizes> {
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
    if request.boot_sectors.is_some() && !matches!(request.mode, 0 | 3) {
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
    } else if request.mode == 0 && request.boot_mib.is_none() {
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

pub fn prepare_new_provision(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &NewProvisionRequest,
) -> EdpCliResult<PreparedNewProvision> {
    guard_usb_disk(runner, disk)?;
    let total_sectors = sysinfo::disk_total_sectors(runner, disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = runner
        .hardware_probe(disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let device_id = target.device_id().to_string();
    let compatibility =
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, SECTOR as u32)
            .ok_or_else(|| {
            err(
                EXIT_TARGET,
                "错误: 当前目标不符合已验证的 512B/255x63 USB LCE 几何",
            )
        })?;

    let metadata = ProvisionMetadata::new(
        OnlyId::parse(&request.label_id)
            .map_err(|message| err(EXIT_TARGET, format!("错误: 标签标识无效: {message}")))?,
        request.user.clone(),
        request.dept.clone(),
        request.label.clone(),
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘身份字段无效: {message}")))?;
    let profile =
        ProvisionProfile::canonical_v1().with_force_change_password(request.force_change_password);
    let spec = ProvisionSpec::new(target, metadata, profile)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘元数据无法编码: {message}")))?;

    let mut file_key = random_array::<16>()?;
    let legacy_file_key = random_array::<8>()?;
    let entropy = ProvisionEntropy::new(random_array::<252>()?);
    let current_key = wrap_file_key(request.password.as_bytes(), file_key, FileKeyWrapMode::Sm4);
    let legacy_key = wrap_legacy_lba7_file_key(request.password.as_bytes(), legacy_file_key);
    let selected_mode = mode(request.mode)?;
    let plan = OfficialProvisionPlan::new(
        selected_mode,
        sizes(request)?,
        compatibility,
        legacy_key,
        current_key,
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘布局无效: {message}")))?
    .with_filesystems(request.format.filesystems());
    let logical_count = plan
        .logical_partitions(SECTOR as u64)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘布局无效: {message}")))?
        .len();
    let mut serials = Vec::with_capacity(logical_count);
    for _ in 0..logical_count {
        serials.push(u32::from_le_bytes(random_array::<4>()?));
    }
    let format_targets_result = plan_format_targets(&plan, &request.format, &serials, &file_key);
    file_key.fill(0);
    let format_targets =
        format_targets_result.map_err(|message| err(EXIT_TARGET, format!("错误: {message}")))?;
    let expected_serial = if format_targets.iter().any(|choice| choice.selected) {
        Some(
            runner
                .hardware_serial(disk)
                .filter(|serial| !serial.trim().is_empty())
                .ok_or_else(|| err(EXIT_TARGET, "错误: 无法读取 USB 硬件序列号，拒绝安排格式化"))?,
        )
    } else {
        None
    };
    let write_image = build_official_provision_protocol_image(&spec, &entropy, &plan)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 无法构造制盘镜像: {message}")))?;
    let target_geometry = plan
        .format_targets()
        .map_err(|message| err(EXIT_TARGET, message))?
        .into_iter()
        .map(|target| TargetPartitionGeometry {
            role: target.role,
            partition_type: target.geometry.partition_type,
            start_lba: target.geometry.start_sector,
            sector_count: target.geometry.sector_count(),
            physically_encrypted: target.physically_encrypted,
            filesystem: target.filesystem,
        })
        .collect::<Vec<_>>();
    let target_plan = TargetProvisionPlan::build(
        None,
        selected_mode,
        &target_geometry,
        compatibility.start_lba,
        request.password.as_bytes(),
    )
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法构造目标制盘计划: {message}"),
        )
    })?;

    Ok(PreparedNewProvision {
        disk,
        device_id,
        mode: selected_mode,
        force_change_password: request.force_change_password,
        lce_start_lba: compatibility.start_lba,
        write_image,
        format_targets,
        target_plan: Some(target_plan),
        source_metadata: None,
        plan,
        expected_onlyid: request.label_id.clone(),
        expected_serial,
        expected_probe: probe,
        expected_lba3: None,
    })
}

fn confirmed_filesystem(
    boot: &[u8],
    start_lba: u64,
    sectors: u64,
) -> Option<OfficialFilesystemFormat> {
    if boot.len() != SECTOR {
        return None;
    }
    if boot.get(3..11) == Some(b"EXFAT   ")
        && u64::from_le_bytes(boot.get(64..72)?.try_into().ok()?) == start_lba
        && u64::from_le_bytes(boot.get(72..80)?.try_into().ok()?) == sectors
    {
        return Some(OfficialFilesystemFormat::ExFat);
    }
    if boot.get(54..62) == Some(b"FAT16   ")
        && u32::from_le_bytes(boot.get(28..32)?.try_into().ok()?) as u64 == start_lba
    {
        let short = u16::from_le_bytes(boot.get(19..21)?.try_into().ok()?) as u64;
        let total = if short != 0 {
            short
        } else {
            u32::from_le_bytes(boot.get(32..36)?.try_into().ok()?) as u64
        };
        if total == sectors {
            return Some(OfficialFilesystemFormat::Fat16);
        }
    }
    None
}

fn inspect_source_profile(
    dev: &mut dyn SectorDev,
    source_metadata: &[u8],
    device_id: &str,
    total_sectors: u64,
    password: &[u8],
) -> EdpCliResult<Option<ParsedExistingProvision>> {
    let image = ProvisionImage::from_bytes(source_metadata.to_vec())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源元数据长度无效: {message}")))?;
    let mut source =
        parse_existing_provision(&image, device_id, total_sectors).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 来源盘注册结构无法可靠解析: {message}"),
            )
        })?;
    if let Some(source) = source.as_mut() {
        let parts = source.profile.partitions.clone();
        for (index, part) in parts.iter().enumerate() {
            if part.role == PartitionRole::CompatibilityReserve {
                continue;
            }
            let Ok(lba) = u32::try_from(part.start_lba) else {
                continue;
            };
            let Ok(raw) = dev.read_sector(lba) else {
                continue;
            };
            if raw.len() != SECTOR {
                continue;
            }
            let plaintext = if part.physically_encrypted {
                let Ok(key) = source.records[index].verified_sm4_file_key(password) else {
                    continue;
                };
                let Ok(value) = crate::backup_deep::keys::decrypt_mode2(&raw, &key) else {
                    continue;
                };
                value
            } else {
                raw
            };
            if let Some(filesystem) =
                confirmed_filesystem(&plaintext, part.start_lba, part.sector_count)
            {
                source
                    .confirm_filesystem(part.role, filesystem)
                    .map_err(|message| err(EXIT_TARGET, message))?;
            }
        }
    }
    Ok(source)
}

fn override_capacity(
    mib: Option<u64>,
    sectors: Option<u64>,
) -> EdpCliResult<Option<CapacityInput>> {
    if mib.is_some() && sectors.is_some() {
        return Err(err(EXIT_TARGET, "错误: 同一分区不能同时指定 MiB 与 sector"));
    }
    match (mib, sectors) {
        (Some(value), None) => {
            CapacityInput::from_quick(value, QuickCapacityUnit::MiB, CapacitySource::UserEdited)
                .map(Some)
                .map_err(|message| err(EXIT_TARGET, message))
        }
        (None, Some(value)) => CapacityInput::from_exact(value, CapacitySource::UserEdited)
            .map(Some)
            .map_err(|message| err(EXIT_TARGET, message)),
        (None, None) => Ok(None),
        _ => unreachable!(),
    }
}

fn target_encrypt_capacity_override(
    _mode: OfficialPartitionMode,
    mib: Option<u64>,
    sectors: Option<u64>,
) -> EdpCliResult<Option<CapacityInput>> {
    // Quick and Exact always describe the target partition itself. In mode2
    // the fixed 63-sector CompatibilityReserve is a separate canonical
    // partition and must never be subtracted from the Encrypt input.
    override_capacity(mib, sectors)
}

/// The physical path for both plain and registered USB media. Source mode is
/// consulted only while deriving defaults and Preserve candidates.
pub fn prepare_target_provision(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &NewProvisionRequest,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedNewProvision> {
    guard_usb_disk(runner, disk)?;
    let total_sectors = sysinfo::disk_total_sectors(runner, disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = runner
        .hardware_probe(disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let device_id = target.device_id().to_string();
    let compatibility =
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, SECTOR as u32)
            .ok_or_else(|| {
            err(
                EXIT_TARGET,
                "错误: 当前目标不符合已验证的 512B/255x63 USB LCE 几何",
            )
        })?;
    let source_metadata = read_image(dev)?;
    let source = inspect_source_profile(
        dev,
        &source_metadata,
        &device_id,
        total_sectors,
        request.password.as_bytes(),
    )?;
    let source_identity = if source.is_some() {
        let base = crate::inspect::InspectMeta {
            device_id: Some(device_id.clone()),
            vid: None,
            pid: None,
            size_bytes: Some(total_sectors * SECTOR as u64),
            onlyid: None,
        };
        Some(
            crate::metainfo::summarize(&base, |lba| {
                source_metadata
                    .get(lba as usize * SECTOR..(lba as usize + 1) * SECTOR)
                    .map(|raw| raw.to_vec())
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "source metadata sector missing",
                        )
                    })
            })
            .map_err(|error| {
                err(
                    EXIT_TARGET,
                    format!("错误: 无法继承来源盘身份字段: {error}"),
                )
            })?,
        )
    } else {
        None
    };
    let selected_mode = mode(request.mode)?;
    let prefill = prefill_for_target_mode(
        source.as_ref().map(|source| &source.profile),
        selected_mode,
        compatibility.start_lba,
        SECTOR as u64,
    )
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法生成目标模式默认布局: {message}"),
        )
    })?;
    let encrypt_override = target_encrypt_capacity_override(
        selected_mode,
        request.encrypt_mib,
        request.encrypt_sectors,
    )?;
    let prefill = apply_target_geometry_overrides(
        prefill,
        source.as_ref().map(|source| &source.profile),
        TargetGeometryOverrides {
            boot: override_capacity(request.boot_mib, request.boot_sectors)?,
            share: override_capacity(request.share_mib, request.share_sectors)?,
            encrypt: encrypt_override,
            boot_start_lba: request.boot_start_lba,
            share_start_lba: request.share_start_lba,
            encrypt_start_lba: request.encrypt_start_lba,
        },
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 目标分区重叠或越界: {message}")))?;
    let mut targets = prefill
        .target_partitions(SECTOR as u64)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标分区重叠或越界: {message}")))?;
    for target in &mut targets {
        let selected_format = request.format.choice(target.role).0;
        if !selected_format {
            if let Some(old) = source
                .as_ref()
                .and_then(|source| source.profile.partition(target.role))
            {
                if old.filesystem.is_some() {
                    target.filesystem = old.filesystem;
                }
            }
        }
    }
    let mut target_plan = TargetProvisionPlan::build(
        source.as_ref(),
        selected_mode,
        &targets,
        compatibility.start_lba,
        request.password.as_bytes(),
    )
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法生成统一目标制盘计划: {message}"),
        )
    })?;
    for part in &mut target_plan.partitions {
        if request.format.choice(part.geometry.role).0
            && part.action == PartitionAction::PreserveExact
        {
            part.action = PartitionAction::Rebuild;
            part.reason = "用户选择重新格式化；原数据不能原样保留".into();
            part.preserved_record = None;
        }
    }
    let source_onlyid = source.as_ref().and_then(|_| {
        source_metadata
            .get(4 * SECTOR..5 * SECTOR)
            .and_then(diskio::lba4_label_id_from)
    });
    let onlyid = if request.label_id.trim().is_empty() {
        match source_onlyid {
            Some(value) => value,
            None => OnlyId::random_candidate()
                .map_err(|message| err(EXIT_TARGET, message))?
                .text()
                .to_string(),
        }
    } else {
        request.label_id.clone()
    };
    let inherited = |value: &str, source: Option<&str>| -> String {
        if value.trim().is_empty() {
            source.unwrap_or_default().to_string()
        } else {
            value.to_string()
        }
    };
    let user = inherited(
        &request.user,
        source_identity
            .as_ref()
            .and_then(|value| value.ownership.user.as_deref()),
    );
    let dept = inherited(
        &request.dept,
        source_identity
            .as_ref()
            .and_then(|value| value.ownership.dept.as_deref()),
    );
    let label = inherited(
        &request.label,
        source_identity
            .as_ref()
            .and_then(|value| value.safe6_label.as_deref()),
    );
    let label = if label.is_empty() {
        crate::provision::DEFAULT_SAFE6_LABEL.to_string()
    } else {
        label
    };
    let metadata = ProvisionMetadata::new(
        OnlyId::parse(&onlyid)
            .map_err(|message| err(EXIT_TARGET, format!("错误: 标签标识无效: {message}")))?,
        user,
        dept,
        label,
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘身份字段无效: {message}")))?;
    let profile =
        ProvisionProfile::canonical_v1().with_force_change_password(request.force_change_password);
    let spec = ProvisionSpec::new(target, metadata, profile)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘元数据无法编码: {message}")))?;
    let mut filesystems = request.format.filesystems();
    for part in &target_plan.partitions {
        if let Some(format) = part.geometry.filesystem {
            match part.geometry.role {
                PartitionRole::Boot => filesystems.boot = format,
                PartitionRole::Share | PartitionRole::BootShareCombined => {
                    filesystems.share = format
                }
                PartitionRole::Encrypt => filesystems.encrypt = format,
                PartitionRole::CompatibilityReserve => {}
            }
        }
    }
    let mut plan = OfficialProvisionPlan::new(
        selected_mode,
        sizes(request)?,
        compatibility,
        wrap_legacy_lba7_file_key(request.password.as_bytes(), random_array::<8>()?),
        wrap_file_key(
            request.password.as_bytes(),
            random_array::<16>()?,
            FileKeyWrapMode::Sm4,
        ),
    )
    .map_err(|message| err(EXIT_TARGET, message))?
    .with_filesystems(filesystems)
    .with_target_geometry(&targets, SECTOR as u64)
    .map_err(|message| err(EXIT_TARGET, message))?;
    let mut file_keys = Vec::with_capacity(target_plan.partitions.len());
    for (index, part) in target_plan.partitions.iter().enumerate() {
        if let Some(record) = part.preserved_record {
            let key = if record.lba12.need_encrypt != 0 {
                record
                    .verified_sm4_file_key(request.password.as_bytes())
                    .map_err(|message| {
                        err(
                            EXIT_TARGET,
                            format!("错误: 保留分区密钥无法验证: {message}"),
                        )
                    })?
            } else {
                [0; 16]
            };
            file_keys.push(key);
            if record.lba12.need_encrypt != 0 {
                plan = plan
                    .with_partition_key_material(
                        index,
                        record.lba7_key_material(),
                        record
                            .lba12_key_material()
                            .map_err(|message| err(EXIT_TARGET, message))?,
                    )
                    .map_err(|message| err(EXIT_TARGET, message))?;
            }
        } else {
            let key = random_array::<16>()?;
            file_keys.push(key);
            plan = plan
                .with_partition_key_material(
                    index,
                    wrap_legacy_lba7_file_key(request.password.as_bytes(), random_array::<8>()?),
                    wrap_file_key(request.password.as_bytes(), key, FileKeyWrapMode::Sm4),
                )
                .map_err(|message| err(EXIT_TARGET, message))?;
        }
    }
    let mut format_options = request.format.clone();
    format_options.boot = target_plan.partitions.iter().any(|part| {
        part.geometry.role == PartitionRole::Boot && part.action == PartitionAction::Rebuild
    });
    format_options.share = target_plan.partitions.iter().any(|part| {
        matches!(
            part.geometry.role,
            PartitionRole::Share | PartitionRole::BootShareCombined
        ) && part.action == PartitionAction::Rebuild
    });
    format_options.encrypt = target_plan.partitions.iter().any(|part| {
        part.geometry.role == PartitionRole::Encrypt && part.action == PartitionAction::Rebuild
    });
    let mut serials = Vec::with_capacity(target_plan.partitions.len());
    for _ in &target_plan.partitions {
        serials.push(u32::from_le_bytes(random_array::<4>()?));
    }
    let format_result = plan_format_targets_with_keys(&plan, &format_options, &serials, &file_keys);
    for key in &mut file_keys {
        key.fill(0);
    }
    let format_targets = format_result.map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法构造目标格式化计划: {message}"),
        )
    })?;
    let expected_serial = if format_targets.iter().any(|choice| choice.selected) {
        Some(
            runner
                .hardware_serial(disk)
                .filter(|serial| !serial.trim().is_empty())
                .ok_or_else(|| err(EXIT_TARGET, "错误: 无法读取 USB 硬件序列号，拒绝安排格式化"))?,
        )
    } else {
        None
    };
    let entropy = ProvisionEntropy::new(random_array::<252>()?);
    let write_image =
        build_official_provision_protocol_image(&spec, &entropy, &plan).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 无法构造目标协议镜像: {message}"),
            )
        })?;
    validate_target_write_set(&target_plan, &write_image.patch, &format_targets)?;
    Ok(PreparedNewProvision {
        disk,
        device_id,
        mode: selected_mode,
        force_change_password: request.force_change_password,
        lce_start_lba: compatibility.start_lba,
        write_image,
        format_targets,
        target_plan: Some(target_plan),
        source_metadata: Some(source_metadata),
        plan,
        expected_onlyid: onlyid,
        expected_serial,
        expected_probe: probe,
        expected_lba3: None,
    })
}

pub fn capture_manufacturer_lba3(
    dev: &mut dyn SectorDev,
    prepared: &mut PreparedNewProvision,
) -> EdpCliResult<()> {
    let raw = dev
        .read_sector(3)
        .map_err(|error| err(EXIT_IO, format!("错误: 读取目标 LBA3 失败: {error}")))?;
    let lba3: [u8; SECTOR] = raw.try_into().map_err(|raw: Vec<u8>| {
        err(
            EXIT_IO,
            format!("错误: 目标 LBA3 长度为 {}B，预期 {SECTOR}B", raw.len()),
        )
    })?;
    prepared.write_image.patch.insert(3, lba3.to_vec());
    prepared.expected_lba3 = Some(lba3);
    Ok(())
}

fn build_conversion_patch(
    conversion: &PasswordlessConversionImage,
) -> EdpCliResult<BTreeMap<u32, Vec<u8>>> {
    let mut patch = BTreeMap::new();
    patch.insert(0, conversion.lba0.to_vec());
    patch.insert(7, conversion.lba7.to_vec());
    patch.insert(12, conversion.lba12.to_vec());
    for (&relative, sector) in conversion.front_filesystem.sectors() {
        let absolute = conversion
            .plan
            .front_start_lba
            .checked_add(relative)
            .ok_or_else(|| err(EXIT_TARGET, "错误: 前部文件系统 LBA 溢出"))?;
        if absolute >= conversion.plan.encrypt_start_lba || absolute > u32::MAX as u64 {
            return Err(err(
                EXIT_TARGET,
                format!("错误: 前部文件系统越过 type4 边界: LBA{absolute}"),
            ));
        }
        patch.insert(absolute as u32, sector.to_vec());
    }
    Ok(patch)
}

pub fn try_prepare_mode1_from_existing_mode0(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<Option<PreparedPasswordlessConversion>> {
    guard_usb_disk(runner, disk)?;
    let source_metadata = read_image(dev)?;
    let identity = identify(runner, disk, &source_metadata[7 * SECTOR..8 * SECTOR]);
    let Some(device_id) = identity.device_id else {
        return Ok(None);
    };
    let source = ProvisionImage::from_bytes(source_metadata.clone())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 源协议镜像无效: {message}")))?;
    if !is_mode0_source(&source, &device_id) {
        return Ok(None);
    }
    let volume_serial = u32::from_le_bytes(random_array::<4>()?);
    let conversion = build_passwordless_conversion(&source, &device_id, volume_serial, "SAFE6")
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 无法生成模式1保留保密区重制计划: {message}"),
            )
        })?;
    let patch = build_conversion_patch(&conversion)?;
    Ok(Some(PreparedPasswordlessConversion {
        disk,
        device_id,
        source_metadata,
        conversion,
        patch,
    }))
}

pub fn prepare_passwordless_conversion(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedPasswordlessConversion> {
    guard_usb_disk(runner, disk)?;
    let source_metadata = read_image(dev)?;
    let identity = identify(runner, disk, &source_metadata[7 * SECTOR..8 * SECTOR]);
    let device_id = identity.device_id.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 无法从现有官方盘识别 device_id，拒绝生成免密转换",
        )
    })?;
    let source = ProvisionImage::from_bytes(source_metadata.clone())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 源协议镜像无效: {message}")))?;
    let volume_serial = u32::from_le_bytes(random_array::<4>()?);
    let conversion = build_passwordless_conversion(&source, &device_id, volume_serial, "SAFE6")
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 无法生成严格免密转换: {message}"),
            )
        })?;
    let patch = build_conversion_patch(&conversion)?;
    Ok(PreparedPasswordlessConversion {
        disk,
        device_id,
        source_metadata,
        conversion,
        patch,
    })
}

pub fn commit_passwordless_conversion(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedPasswordlessConversion,
) -> EdpCliResult<()> {
    let _guard = sysinfo::prepare_write(runner, prepared.disk).map_err(|error| {
        err(
            EXIT_IO,
            format!("错误: 无法卸载/锁定 disk{}: {error}", prepared.disk),
        )
    })?;
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法以读写方式重开目标盘: {error}")))?;
    verify_reopened_snapshot(dev, &prepared.source_metadata)?;
    diskio::atomic_write_passwordless_conversion_sectors(
        dev,
        &prepared.patch,
        prepared.conversion.plan.encrypt_start_lba,
    )
}

pub fn commit_new_provision(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<ProvisionCommitReport> {
    let target_plan = prepared
        .target_plan
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 缺少统一目标制盘计划，拒绝写盘"))?;
    validate_target_write_set(
        target_plan,
        &prepared.write_image.patch,
        &prepared.format_targets,
    )?;
    validate_preserve_source_snapshot(
        target_plan.has_preserved_partitions(),
        prepared.source_metadata.as_deref(),
    )?;
    guard_usb_disk(runner, prepared.disk)?;
    let _guard = sysinfo::prepare_write(runner, prepared.disk).map_err(|error| {
        err(
            EXIT_IO,
            format!("错误: 无法卸载/锁定 disk{}: {error}", prepared.disk),
        )
    })?;
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法以读写方式重开目标盘: {error}")))?;
    if let Some(source_metadata) = &prepared.source_metadata {
        verify_reopened_snapshot(dev, source_metadata)?;
    }
    let fresh_probe = runner
        .hardware_probe(prepared.disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, prepared.disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标硬件身份"))?;
    let fresh_total = sysinfo::disk_total_sectors(runner, prepared.disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标容量"))?;
    if fresh_probe != prepared.expected_probe || fresh_total != prepared.write_image.total_sectors {
        return Err(err(
            EXIT_TARGET,
            "错误: 制盘确认/卸载期间目标硬件身份或容量发生变化，疑似换盘，拒绝写入",
        ));
    }
    if let Some(serial) = &prepared.expected_serial {
        if runner.hardware_serial(prepared.disk).as_ref() != Some(serial) {
            return Err(err(
                EXIT_TARGET,
                "错误: 制盘确认期间 USB 硬件序列号发生变化",
            ));
        }
    }
    let expected_lba3 = prepared.expected_lba3.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 制盘写入前未捕获制造商 LBA3，拒绝覆盖不透明制造商元数据",
        )
    })?;
    let fresh_lba3 = dev
        .read_sector(3)
        .map_err(|error| err(EXIT_IO, format!("错误: 重开后复核 LBA3 失败: {error}")))?;
    if fresh_lba3.as_slice() != expected_lba3 {
        return Err(err(
            EXIT_TARGET,
            "错误: 制盘确认/卸载期间制造商 LBA3 发生变化，拒绝写入",
        ));
    }
    diskio::atomic_write_official_provision_sectors(
        dev,
        &prepared.write_image.patch,
        prepared.write_image.total_sectors,
    )?;
    verify_protocol_readback(dev, prepared)?;
    let mut report = ProvisionCommitReport {
        provision_succeeded: true,
        formats: Vec::new(),
    };
    for choice in prepared
        .format_targets
        .iter()
        .filter(|choice| choice.selected)
    {
        let result = format_partition(runner, dev, prepared, choice);
        report.formats.push(PartitionFormatResult {
            role: choice.target.role,
            result: result.map_err(|error| error.msg),
        });
    }
    Ok(report)
}

fn validate_preserve_source_snapshot(
    has_preserved_partitions: bool,
    source_metadata: Option<&[u8]>,
) -> EdpCliResult<()> {
    if !has_preserved_partitions {
        return Ok(());
    }
    let source_metadata = source_metadata.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: PreserveExact 计划缺少准备阶段来源元数据快照，拒绝写盘",
        )
    })?;
    if source_metadata.len() != 13 * SECTOR {
        return Err(err(
            EXIT_TARGET,
            "错误: PreserveExact 计划的来源元数据快照长度异常，拒绝写盘",
        ));
    }
    Ok(())
}

fn validate_target_write_set(
    target_plan: &TargetProvisionPlan,
    patch: &BTreeMap<u32, Vec<u8>>,
    formats: &[PlannedPartitionFormat],
) -> EdpCliResult<()> {
    for (start, count) in target_plan.preserved_extents() {
        let end = start
            .checked_add(count)
            .ok_or_else(|| err(EXIT_TARGET, "错误: 保留分区 LBA 溢出"))?;
        if patch
            .keys()
            .any(|lba| (start..end).contains(&u64::from(*lba)))
        {
            return Err(err(
                EXIT_TARGET,
                format!("错误: 协议写集合触碰保留分区 LBA{start}..{}", end - 1),
            ));
        }
        if formats.iter().any(|choice| {
            choice.selected
                && choice.target.geometry.start_sector < end
                && start < choice.target.geometry.end_sector_exclusive()
        }) {
            return Err(err(
                EXIT_TARGET,
                format!("错误: 格式化计划触碰保留分区 LBA{start}..{}", end - 1),
            ));
        }
    }
    if target_plan.partitions.iter().any(|part| {
        part.action == PartitionAction::PreserveExact && part.preserved_record.is_none()
    }) {
        return Err(err(EXIT_TARGET, "错误: 保留分区缺少原 key material"));
    }
    Ok(())
}

fn verify_format_identity(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    guard_usb_disk(runner, prepared.disk)?;
    let fresh_probe = runner
        .hardware_probe(prepared.disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, prepared.disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化前无法复核硬件身份"))?;
    let fresh_total = sysinfo::disk_total_sectors(runner, prepared.disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化前无法复核容量"))?;
    verify_format_hardware(
        &prepared.expected_probe,
        prepared.write_image.total_sectors,
        &prepared.device_id,
        prepared.expected_serial.as_deref(),
        &fresh_probe,
        fresh_total,
        runner.hardware_serial(prepared.disk).as_deref(),
    )?;
    verify_protocol_readback(dev, prepared)
}

fn verify_format_hardware(
    expected_probe: &crate::platform::HardwareProbe,
    expected_total: u64,
    expected_device_id: &str,
    expected_serial: Option<&str>,
    fresh_probe: &crate::platform::HardwareProbe,
    fresh_total: u64,
    fresh_serial: Option<&str>,
) -> EdpCliResult<()> {
    let fresh_identity =
        TargetIdentity::from_probe(fresh_probe, fresh_total).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 格式化前硬件身份无效: {message}"),
            )
        })?;
    if fresh_probe != expected_probe
        || fresh_total != expected_total
        || fresh_identity.device_id() != expected_device_id
    {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前硬件身份、容量或 device_id 已变化",
        ));
    }
    if expected_serial.is_none_or(|serial| Some(serial) != fresh_serial) {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前 USB 硬件序列号缺失或已变化",
        ));
    }
    Ok(())
}

fn verify_protocol_readback(
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    let raw = read_image(dev)?;
    if (0..13u32).any(|lba| {
        prepared.write_image.patch.get(&lba).is_none_or(|expected| {
            raw[lba as usize * SECTOR..(lba as usize + 1) * SECTOR] != expected[..]
        })
    }) {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前 LBA0–12 与本次制盘计划不一致",
        ));
    }
    let onlyid = diskio::lba4_label_id_from(&raw[4 * SECTOR..5 * SECTOR]);
    if onlyid.as_deref() != Some(&prepared.expected_onlyid) {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前 onlyid 与本次制盘计划不一致",
        ));
    }
    let actual = crate::backup_metadata::parse_partition_geometry(
        &raw,
        &prepared.device_id,
        prepared.write_image.total_sectors,
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 格式化前分区表无效: {message}")))?;
    let planned = prepared
        .plan
        .logical_partitions(SECTOR as u64)
        .map_err(|message| err(EXIT_TARGET, message))?;
    if actual.len() != planned.len()
        || actual.iter().zip(planned.iter()).any(|(actual, planned)| {
            actual.partition_type != planned.partition_type.raw()
                || actual.start_sector != planned.start_sector
                || actual.sector_count != planned.sector_count()
        })
    {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前实际分区布局与制盘计划不一致",
        ));
    }
    Ok(())
}

struct PreparedImageReader<'a> {
    image: &'a SparseFilesystemImage,
}

impl PartitionReader for PreparedImageReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        self.image
            .sector_or_zero(relative_lba)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| std::io::Error::other("format read outside partition"))
    }
}

fn format_partition(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
    choice: &PlannedPartitionFormat,
) -> EdpCliResult<()> {
    verify_format_identity(runner, dev, prepared)?;
    execute_partition_format(dev, choice)?;
    verify_format_identity(runner, dev, prepared)?;
    Ok(())
}

/// Format one verified official partition. The caller owns device identity and
/// protocol verification; this operation never writes the protocol region.
fn execute_partition_format(
    dev: &mut dyn SectorDev,
    choice: &PlannedPartitionFormat,
) -> EdpCliResult<()> {
    let filesystem = choice
        .filesystem
        .ok_or_else(|| err(EXIT_TARGET, "错误: 兼容保留区不可格式化"))?;
    let built = choice
        .prepared_image
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化计划缺少预生成物理镜像"))?;
    let verification_image = choice
        .verification_image
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化计划缺少验证镜像"))?;
    if built.geometry != choice.target.geometry
        || built.physically_encrypted != choice.target.physically_encrypted
        || built.image.volume_sectors() != choice.target.geometry.sector_count()
        || verification_image.volume_sectors() != choice.target.geometry.sector_count()
    {
        return Err(err(EXIT_TARGET, "错误: 预生成格式化镜像与目标几何不一致"));
    }
    for (&relative, sector) in built.image.sectors() {
        let absolute = choice
            .target
            .geometry
            .start_sector
            .checked_add(relative)
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化写入 LBA 溢出"))?;
        dev.write_sector(absolute, sector).map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 写入失败: {error}"),
            )
        })?;
    }
    dev.sync()
        .map_err(|error| err(EXIT_IO, format!("错误: 格式化同步失败: {error}")))?;
    for (&relative, expected) in built.image.sectors() {
        let absolute = u32::try_from(choice.target.geometry.start_sector + relative)
            .map_err(|_| err(EXIT_TARGET, "错误: 格式化读回 LBA 溢出"))?;
        let actual = dev.read_sector(absolute).map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 读回失败: {error}"),
            )
        })?;
        if actual.as_slice() != expected {
            return Err(err(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 读回不一致"),
            ));
        }
    }
    let raw_boot = dev
        .read_sector(
            u32::try_from(choice.target.geometry.start_sector)
                .map_err(|_| err(EXIT_TARGET, "错误: 分区起点 LBA 溢出"))?,
        )
        .map_err(|error| err(EXIT_IO, format!("错误: 读取文件系统引导扇区失败: {error}")))?;
    if choice.target.physically_encrypted
        && (raw_boot.get(3..11) == Some(b"EXFAT   ") || raw_boot.get(54..62) == Some(b"FAT16   "))
    {
        return Err(err(EXIT_IO, "错误: 加密分区物理首扇区出现明文文件系统签名"));
    }
    let geometry = PartitionGeometry {
        index: 0,
        partition_type: choice.target.geometry.partition_type.raw(),
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: choice.target.geometry.start_sector,
        sector_size: SECTOR as u64,
        partition_size: choice.target.geometry.size_bytes,
        sector_count: choice.target.geometry.sector_count(),
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    let mut reader = PreparedImageReader {
        image: verification_image,
    };
    let boot = reader
        .read_sector(0)
        .map_err(|error| err(EXIT_IO, error.to_string()))?;
    let geometry_ok = match filesystem {
        OfficialFilesystemFormat::ExFat => {
            boot.get(3..11) == Some(b"EXFAT   ")
                && u64::from_le_bytes(boot[64..72].try_into().unwrap())
                    == choice.target.geometry.start_sector
                && u64::from_le_bytes(boot[72..80].try_into().unwrap())
                    == choice.target.geometry.sector_count()
                && u32::from_le_bytes(boot[100..104].try_into().unwrap()) == choice.volume_serial
        }
        OfficialFilesystemFormat::Fat16 => {
            let total16 = u16::from_le_bytes(boot[19..21].try_into().unwrap()) as u64;
            let total = if total16 != 0 {
                total16
            } else {
                u32::from_le_bytes(boot[32..36].try_into().unwrap()) as u64
            };
            boot.get(54..62) == Some(b"FAT16   ")
                && u32::from_le_bytes(boot[28..32].try_into().unwrap()) as u64
                    == choice.target.geometry.start_sector
                && total == choice.target.geometry.sector_count()
                && u32::from_le_bytes(boot[39..43].try_into().unwrap()) == choice.volume_serial
        }
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => false,
    };
    if !geometry_ok {
        return Err(err(EXIT_IO, "错误: 文件系统签名、几何或卷序列号读回不一致"));
    }
    let report = analyze_partition(&geometry, &mut reader);
    if report.status != AnalysisStatus::Parsed
        || report.filesystem.as_deref() != Some(filesystem.config_token())
        || report.file_count != Some(0)
    {
        return Err(err(
            EXIT_IO,
            format!(
                "错误: {} 深度解析失败: {}",
                filesystem.config_token(),
                report.reason
            ),
        ));
    }
    let root_lba = match filesystem {
        OfficialFilesystemFormat::ExFat => {
            let root_cluster = u32::from_le_bytes(boot[96..100].try_into().unwrap());
            let heap_offset = u32::from_le_bytes(boot[88..92].try_into().unwrap()) as u64;
            let cluster_sectors = 1u64 << boot[109];
            heap_offset + (root_cluster as u64 - 2) * cluster_sectors
        }
        OfficialFilesystemFormat::Fat16 => {
            u16::from_le_bytes(boot[14..16].try_into().unwrap()) as u64
                + boot[16] as u64 * u16::from_le_bytes(boot[22..24].try_into().unwrap()) as u64
        }
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => unreachable!(),
    };
    let root = reader
        .read_sector(root_lba)
        .map_err(|error| err(EXIT_IO, error.to_string()))?;
    let actual_label = match filesystem {
        OfficialFilesystemFormat::ExFat => root
            .as_chunks::<32>()
            .0
            .iter()
            .find(|entry| entry[0] == 0x83)
            .and_then(|entry| {
                let count = entry[1] as usize;
                (count <= 11).then(|| {
                    (0..count)
                        .map(|index| {
                            u16::from_le_bytes([entry[2 + index * 2], entry[3 + index * 2]])
                        })
                        .collect::<Vec<_>>()
                })
            })
            .and_then(|units| String::from_utf16(&units).ok())
            .unwrap_or_default(),
        OfficialFilesystemFormat::Fat16 => {
            if root[11] != 0x08 || boot[43..54] != root[..11] {
                return Err(err(EXIT_IO, "错误: FAT16 卷标目录项读回不一致"));
            }
            let (decoded, _, had_errors) = GBK.decode(&root[..11]);
            if had_errors {
                return Err(err(EXIT_IO, "错误: FAT16 卷标无法按 GBK 解码"));
            }
            decoded.trim_end_matches(' ').to_string()
        }
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => unreachable!(),
    };
    let expected_label = if filesystem == OfficialFilesystemFormat::Fat16 {
        choice.volume_label.to_uppercase()
    } else {
        choice.volume_label.clone()
    };
    if actual_label != expected_label {
        return Err(err(EXIT_IO, "错误: 文件系统卷标读回不一致"));
    }
    Ok(())
}

pub fn export_sparse_provision_image(
    path: &Path,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    if prepared.target_plan.as_ref().is_some_and(|plan| {
        plan.partitions
            .iter()
            .any(|part| part.action == PartitionAction::PreserveExact)
    }) {
        return Err(err(
            EXIT_TARGET,
            "错误: 含保留数据的制盘计划不能导出为稀疏镜像；镜像不包含来源盘用户数据",
        ));
    }
    let byte_len = prepared
        .write_image
        .total_sectors
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| err(EXIT_IO, "错误: 镜像长度溢出"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 无法创建 {}: {error}", path.display()),
            )
        })?;
    file.set_len(byte_len)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法设置镜像长度: {error}")))?;
    for (&lba, sector) in &prepared.write_image.patch {
        file.seek(SeekFrom::Start(u64::from(lba) * SECTOR as u64))
            .and_then(|_| file.write_all(sector))
            .map_err(|error| err(EXIT_IO, format!("错误: 写入镜像 LBA{lba} 失败: {error}")))?;
    }
    for choice in prepared
        .format_targets
        .iter()
        .filter(|choice| choice.selected)
    {
        let built = choice
            .prepared_image
            .as_ref()
            .ok_or_else(|| err(EXIT_TARGET, "错误: 导出计划缺少预生成格式化镜像"))?;
        for (&relative_lba, sector) in built.image.sectors() {
            let absolute_lba = choice
                .target
                .geometry
                .start_sector
                .checked_add(relative_lba)
                .ok_or_else(|| err(EXIT_TARGET, "错误: 导出格式化 LBA 溢出"))?;
            file.seek(SeekFrom::Start(absolute_lba * SECTOR as u64))
                .and_then(|_| file.write_all(sector))
                .map_err(|error| {
                    err(
                        EXIT_IO,
                        format!("错误: 写入格式化镜像 LBA{absolute_lba} 失败: {error}"),
                    )
                })?;
        }
    }
    file.sync_all()
        .map_err(|error| err(EXIT_IO, format!("错误: 镜像同步失败: {error}")))?;
    Ok(())
}

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
