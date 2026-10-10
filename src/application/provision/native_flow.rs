//! One source-aware native-block provisioning plan for every frontend and
//! logical geometry. The disk transport decides discovery only; the write
//! plan, identity pin, WAL and readback are the same for USB and Disk Image.

use super::{FormatOptions, OfficialProvisionRequest, ProvisionRequest};
use crate::diskio::{NativeBlockDevice, NativeRawBlockDevice};
use crate::filesystem::NativeVirtualDiskPlan;
use crate::platform::{system, HardwareProbe};
use crate::ports::CmdRunner;
use crate::provision::{
    wrap_file_key, wrap_legacy_lba7_file_key, DiskProvisionKind, OfficialPartitionSizes,
    OfficialProvisionPlan, OnlyId, PassInfoPolicy, ProvisionEntropy, ProvisionMetadata,
    ProvisionProfile, ProvisionSpec, TargetIdentity,
};
use std::path::Path;

/// Plan is immutable across user confirmation and the write lease. It includes
/// an exact source prefix pin; a changed disk is not silently reclassified.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NativePreparedProvision {
    pub disk: u32,
    pub source: DiskProvisionKind,
    pub target: crate::provision::ProvisionTarget,
    pub device_id: String,
    pub plan: NativeVirtualDiskPlan,
    pub source_native_prefix: Vec<Vec<u8>>,
    pub source_protocol_projection: Vec<u8>,
    pub before_pin: crate::media_identity::MediaIdentityPin,
    pub probe: HardwareProbe,
    pub onlyid: Option<String>,
    pub lce_extent: Option<(u64, u64)>,
    pub partitions: Vec<NativePreviewPartition>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NativePreviewPartition {
    pub role: Option<crate::provision::PartitionRole>,
    pub start_lba: u64,
    pub sector_count: u64,
    pub filesystem: Option<crate::filesystem::FilesystemKind>,
    pub formatted: bool,
    pub physically_encrypted: bool,
}

fn native_compatibility_extent(
    total: u64,
    sector: u32,
) -> Result<crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout, String> {
    use crate::protocol::lba7_compat::*;
    let tracks = u64::from(VERIFIED_USB_TRACKS_PER_CYLINDER);
    let sectors = u64::from(VERIFIED_USB_SECTORS_PER_TRACK);
    let cylinders = total / (tracks * sectors);
    locate_lba7_compatibility_extent_from_geometry(
        cylinders,
        VERIFIED_USB_TRACKS_PER_CYLINDER,
        VERIFIED_USB_SECTORS_PER_TRACK,
        sector,
    )
    .ok_or("原生LCE无法从当前容量和逻辑扇区几何定位".into())
}

type NativeOfficialPlanResult = (
    String,
    NativeVirtualDiskPlan,
    Vec<NativePreviewPartition>,
    (u64, u64),
    String,
);

fn native_rebuild_format_policy(
    mode: crate::provision::OfficialPartitionMode,
    request: &OfficialProvisionRequest,
) -> Result<FormatOptions, String> {
    use crate::provision::OfficialPartitionMode as Mode;
    let has_boot = matches!(
        mode,
        Mode::DefaultThreePartition | Mode::IntranetExtranetDualPartition
    );
    let has_share = mode != Mode::WholeDiskEncrypted;
    let has_encrypt = mode != Mode::IntranetExtranetDualPartition;
    let mut options = request.format.clone();
    if !request.preserve_unformatted && !options.boot && !options.share && !options.encrypt {
        // CLI's implicit full-rebuild default is explicitly destructive.
        options.boot = has_boot;
        options.share = has_share;
        options.encrypt = has_encrypt;
    }
    // Do not reinterpret an explicit unformatted request as a full rebuild.
    // Source-aware validation below must prove each unformatted partition is
    // preserved; with no compatible source, prepare fails before any write.
    Ok(options)
}

/// Use the same source-aware geometry/disposition machinery as the legacy
/// provision prepare. All coordinates here are *native* LBAs.
fn native_source_aware_targets(
    mode: crate::provision::OfficialPartitionMode,
    sector: u32,
    lce_start: u64,
    request: &OfficialProvisionRequest,
    options: &mut FormatOptions,
    source: Option<&crate::provision::ParsedExistingProvision>,
) -> Result<
    (
        Vec<crate::provision::TargetPartitionGeometry>,
        crate::provision::TargetProvisionPlan,
    ),
    String,
> {
    use crate::provision::{
        apply_target_geometry_overrides, prefill_for_target_mode, CapacityInput, CapacitySource,
        PartitionRole, PasswordDisposition, QuickCapacityUnit, RegionDisposition,
        TargetGeometryOverrides, TargetProvisionPlan,
    };
    let capacity =
        |mib: Option<u64>, sectors: Option<u64>| -> Result<Option<CapacityInput>, String> {
            if mib.is_some() && sectors.is_some() {
                return Err("同一分区不能同时指定MiB与sector".into());
            }
            match (mib, sectors) {
                (Some(mib), None) => CapacityInput::from_quick_native(
                    mib,
                    QuickCapacityUnit::MiB,
                    CapacitySource::UserEdited,
                    sector,
                )
                .map(Some),
                (None, Some(sectors)) => {
                    CapacityInput::from_exact(sectors, CapacitySource::UserEdited).map(Some)
                }
                _ => Ok(None),
            }
        };
    let boot = if let Some(value) = request.boot_mib {
        Some(CapacityInput::from_exact(
            crate::provision::official_boot_sectors_from_end_mib(value, u64::from(sector))?,
            CapacitySource::UserEdited,
        )?)
    } else {
        capacity(None, request.boot_sectors)?
    };
    let prefill = prefill_for_target_mode(
        source.map(|source| &source.profile),
        mode,
        lce_start,
        u64::from(sector),
    )?;
    let prefill = apply_target_geometry_overrides(
        prefill,
        source.map(|source| &source.profile),
        TargetGeometryOverrides {
            boot,
            share: capacity(request.share_mib, request.share_sectors)?,
            encrypt: capacity(request.encrypt_mib, request.encrypt_sectors)?,
            boot_start_lba: request.boot_start_lba,
            share_start_lba: request.share_start_lba,
            encrypt_start_lba: request.encrypt_start_lba,
        },
    )?;
    let mut targets = prefill.target_partitions(u64::from(sector))?;
    for target in &mut targets {
        if options.choice(target.role).0 {
            target.filesystem = options.filesystems().for_role(target.role);
        } else if let Some(old) = source.and_then(|source| source.profile.partition(target.role)) {
            target.filesystem = old.filesystem;
            // A confirmed source filesystem is authoritative on a retained region.
            if let Some(kind) = old.filesystem {
                match target.role {
                    PartitionRole::Boot => options.boot_fs = kind,
                    PartitionRole::Share | PartitionRole::BootShareCombined => {
                        options.share_fs = kind
                    }
                    PartitionRole::Encrypt => options.encrypt_fs = kind,
                    PartitionRole::CompatibilityReserve => {}
                }
            }
        }
    }
    let mut dispositions =
        TargetProvisionPlan::build(source, mode, &targets, lce_start, &request.key_domains)?;
    for target in &targets {
        if options.choice(target.role).0 {
            dispositions.force_rebuild_for_format(target.role);
        }
    }
    for part in &dispositions.partitions {
        if part.geometry.role == PartitionRole::CompatibilityReserve {
            continue; // Mode2's reserved type1 region has no filesystem.
        }
        if !options.choice(part.geometry.role).0 {
            if part.password_disposition == Some(PasswordDisposition::Blocked)
                || part.disposition == RegionDisposition::Rebuild
            {
                return Err(format!(
                    "{}无法无损保留：{}；几何/文件系统/密钥不兼容或来源密码不足。未授权格式化，停止写盘。",
                    part.geometry.role.label(), part.reason,
                ));
            }
            if let Some(record) = part.preserved_record {
                if record.lba12.need_encrypt != 0
                    && record.lba12.encrypt_mode != request.algorithm.file_key_wrap_mode().raw()
                {
                    return Err(format!(
                        "{}来源算法与目标算法不一致，不可保留原密文；未授权格式化，停止写盘。",
                        part.geometry.role.label(),
                    ));
                }
            }
        }
    }
    Ok((targets, dispositions))
}

fn generate_official_native_plan(
    total: u64,
    sector: u32,
    probe: &HardwareProbe,
    request: &OfficialProvisionRequest,
    inherited_onlyid: Option<&str>,
    inherited_pass_info: Option<PassInfoPolicy>,
    source: Option<&crate::provision::ParsedExistingProvision>,
    source_prefix: &[Vec<u8>],
) -> Result<NativeOfficialPlanResult, String> {
    request.algorithm.validate_first_party_write()?;
    let mode = request.target.official_mode().ok_or("目标不是官方模式")?;
    let identity = TargetIdentity::from_probe(probe, total)?;
    let device_id = identity.device_id().to_owned();
    let onlyid = if !request.label_id.is_empty() {
        OnlyId::parse(&request.label_id)?
    } else if let Some(valid) = inherited_onlyid.and_then(|s| OnlyId::parse(s).ok()) {
        // Keep trusted protocol identity when destructively changing layout.
        // New random OnlyID is only generated for a new/unregistered source.
        valid
    } else {
        OnlyId::random_candidate()?
    };
    let metadata = ProvisionMetadata::new(
        onlyid,
        if request.user.is_empty() {
            "EDP USER".to_owned()
        } else {
            request.user.clone()
        },
        if request.dept.is_empty() {
            "EDP DEPT".to_owned()
        } else {
            request.dept.clone()
        },
        if request.label.is_empty() {
            crate::provision::DEFAULT_SAFE6_LABEL.to_owned()
        } else {
            request.label.clone()
        },
    )?
    .with_lba8_identity(request.lba8_identity.clone())?;
    let inherited = inherited_pass_info.unwrap_or_default();
    let policy = PassInfoPolicy {
        force_change_password: request
            .force_change_password
            .unwrap_or(inherited.force_change_password),
        cancel_password_complexity_check: request
            .cancel_password_complexity_check
            .unwrap_or(inherited.cancel_password_complexity_check),
        max_share_password_errors: request
            .max_share_password_errors
            .unwrap_or(inherited.max_share_password_errors),
        max_encrypt_password_errors: request
            .max_encrypt_password_errors
            .unwrap_or(inherited.max_encrypt_password_errors),
    };
    let spec = ProvisionSpec::new(
        identity,
        metadata,
        ProvisionProfile::canonical_v1().with_pass_info_policy(policy),
    )?;
    let compatibility = native_compatibility_extent(total, sector)?;
    let mut sizes = OfficialPartitionSizes::new(
        request
            .boot_mib
            .unwrap_or(crate::provision::DEFAULT_OEM_BOOT_END_MIB),
        request.share_mib.unwrap_or(64),
        request.encrypt_mib.unwrap_or(128),
    );
    if let Some(value) = request.boot_sectors {
        sizes = sizes.with_boot_sectors(value);
    }
    if let Some(value) = request.share_sectors {
        sizes = sizes.with_share_sectors(value);
    }
    if let Some(value) = request.encrypt_sectors {
        sizes = sizes.with_encrypt_sectors(value);
    }

    // Rebuilding a protocol generates new partition FileKeys below. A
    // preserved encrypted extent MUST carry over the original LBA7/LBA12
    // wrapped key materials. Never turn TUI "保留" into "格式化" silently.
    let mut options = native_rebuild_format_policy(mode, request)?;
    // Current OEM writer passes `L"FAT"` to FormatEx: FAT12/FAT16 must
    // come from the native volume's actual cluster count. The CLI/TUI share
    // this application policy and never branch by USB vs Disk Image.
    if matches!(
        mode,
        crate::provision::OfficialPartitionMode::DefaultThreePartition
            | crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition
    ) && matches!(
        options.boot_fs,
        crate::filesystem::FilesystemKind::Fat12 | crate::filesystem::FilesystemKind::Fat16
    ) {
        let boot =
            crate::provision::build_official_partition_layout(mode, sizes, u64::from(sector))?
                .into_iter()
                .next()
                .ok_or("官方模式缺少独立启动区")?;
        options.boot_fs = crate::filesystem::select_native_oem_boot_fat(
            crate::filesystem::FilesystemGeometry::new(
                request.boot_start_lba.unwrap_or(boot.start_sector),
                boot.sector_count(),
                sector,
            ),
        )?;
    }
    let key_mode = request.algorithm.file_key_wrap_mode();
    let mut first_key = [0u8; 16];
    getrandom::fill(&mut first_key).map_err(|e| format!("生成FileKey失败: {e}"))?;
    let mut first_legacy_key = [0u8; 8];
    getrandom::fill(&mut first_legacy_key).map_err(|e| format!("生成LBA7密钥失败: {e}"))?;
    let first_pass = crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD;
    let mut plan = OfficialProvisionPlan::new(
        mode,
        sizes,
        compatibility,
        wrap_legacy_lba7_file_key(first_pass, first_legacy_key),
        wrap_file_key(first_pass, first_key, key_mode),
    )?
    .with_filesystems(options.filesystems());

    let starts = [
        (
            crate::provision::PartitionRole::Boot,
            request.boot_start_lba,
        ),
        (
            crate::provision::PartitionRole::Share,
            request.share_start_lba,
        ),
        (
            crate::provision::PartitionRole::Encrypt,
            request.encrypt_start_lba,
        ),
    ];
    if starts.iter().any(|(_, start)| start.is_some()) {
        let mut parts = plan
            .format_targets_native(sector)?
            .iter()
            .map(|p| crate::provision::TargetPartitionGeometry {
                role: p.role,
                partition_type: p.geometry.partition_type,
                start_lba: p.geometry.start_sector,
                sector_count: p.geometry.sector_count(),
                physically_encrypted: p.physically_encrypted,
                filesystem: p.filesystem,
            })
            .collect::<Vec<_>>();
        for part in &mut parts {
            if let Some((_, Some(start))) = starts.iter().find(|(role, _)| {
                *role == part.role
                    || (*role == crate::provision::PartitionRole::Share
                        && part.role == crate::provision::PartitionRole::BootShareCombined)
            }) {
                part.start_lba = *start;
            }
        }
        plan = plan.with_target_geometry(&parts, u64::from(sector))?;
    }

    // Preserve the existing destructive geometry path; only preservation
    // requests use source-aware anchors, and both resolve into one write plan.
    let needs_preserve = plan
        .format_targets_native(sector)?
        .iter()
        .any(|part| part.format_capable && !options.choice(part.role).0);
    let dispositions = if needs_preserve {
        let (targets, decisions) = native_source_aware_targets(
            mode,
            sector,
            compatibility.start_lba,
            request,
            &mut options,
            source,
        )?;
        plan = plan
            .with_filesystems(options.filesystems())
            .with_target_geometry(&targets, u64::from(sector))?;
        decisions
    } else {
        let targets = plan
            .format_targets_native(sector)?
            .iter()
            .map(|p| crate::provision::TargetPartitionGeometry {
                role: p.role,
                partition_type: p.geometry.partition_type,
                start_lba: p.geometry.start_sector,
                sector_count: p.geometry.sector_count(),
                physically_encrypted: p.physically_encrypted,
                filesystem: p.filesystem,
            })
            .collect::<Vec<_>>();
        let mut decisions = crate::provision::TargetProvisionPlan::build(
            source,
            mode,
            &targets,
            compatibility.start_lba,
            &request.key_domains,
        )?;
        for part in &targets {
            if options.choice(part.role).0 {
                decisions.force_rebuild_for_format(part.role);
            }
        }
        decisions
    };
    let targets = plan.format_targets_native(sector)?;
    let mut serials = Vec::with_capacity(targets.len());
    let mut keys = Vec::with_capacity(targets.len());
    for (index, target) in targets.iter().enumerate() {
        let mut serial = [0u8; 4];
        getrandom::fill(&mut serial).map_err(|e| format!("卷序列号生成失败: {e}"))?;
        serials.push(u32::from_le_bytes(serial));
        use crate::provision::RegionDisposition;
        let decision = &dispositions.partitions[index];
        match decision.disposition {
            RegionDisposition::PreserveOpaque | RegionDisposition::PreserveVerified => {
                let record = decision.preserved_record.ok_or_else(|| {
                    format!(
                        "{}保留计划缺少来源 LBA7/LBA12 FileKey 记录",
                        target.role.label()
                    )
                })?;
                if record.lba12.need_encrypt != 0 {
                    plan = plan.with_partition_key_material(
                        index,
                        record.lba7_key_material(),
                        record.lba12_key_material()?,
                    )?;
                }
                keys.push([0u8; 16]); // No FS writes may consume this placeholder.
            }
            RegionDisposition::RewrapVerified => {
                let record = decision.preserved_record.ok_or("Rewrap 缺少来源密钥记录")?;
                let old_password = request
                    .key_domains
                    .source_password(target.role)
                    .unwrap_or(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD);
                let new_password = request
                    .key_domains
                    .target_password(target.role)
                    .ok_or("Rewrap 必须提供目标密码")?;
                let key = record.verified_sm4_file_key(old_password)?;
                let mut legacy = crate::provision::unwrap_legacy_lba7_file_key(
                    old_password,
                    record.lba7_key_material(),
                )?;
                plan = plan.with_partition_key_material(
                    index,
                    wrap_legacy_lba7_file_key(new_password, legacy),
                    wrap_file_key(new_password, key, key_mode),
                )?;
                legacy.fill(0);
                keys.push(key);
            }
            RegionDisposition::Rebuild => {
                let mut file_key = [0u8; 16];
                getrandom::fill(&mut file_key).map_err(|e| format!("分区FileKey生成失败: {e}"))?;
                let mut legacy_key = [0u8; 8];
                getrandom::fill(&mut legacy_key)
                    .map_err(|e| format!("分区LBA7密钥生成失败: {e}"))?;
                let password = request
                    .key_domains
                    .target_password(target.role)
                    .filter(|v| !v.is_empty())
                    .unwrap_or(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD);
                plan = plan.with_partition_key_material(
                    index,
                    wrap_legacy_lba7_file_key(password, legacy_key),
                    wrap_file_key(password, file_key, key_mode),
                )?;
                keys.push(file_key);
            }
            RegionDisposition::Drop => return Err("目标分区不允许使用Drop".into()),
        }
    }
    let mut entropy = [0u8; 252];
    getrandom::fill(&mut entropy).map_err(|e| format!("协议随机字段生成失败: {e}"))?;
    let partitions = targets
        .iter()
        .map(|p| {
            let formatted = match p.role {
                crate::provision::PartitionRole::Boot => options.boot,
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined => options.share,
                crate::provision::PartitionRole::Encrypt => options.encrypt,
                crate::provision::PartitionRole::CompatibilityReserve => false,
            };
            NativePreviewPartition {
                role: Some(p.role),
                start_lba: p.geometry.start_sector,
                sector_count: p.geometry.sector_count(),
                filesystem: p.filesystem,
                formatted,
                physically_encrypted: p.physically_encrypted,
            }
        })
        .collect::<Vec<_>>();
    let mut candidate = super::native_image::plan_native_edp_image(
        &spec,
        &ProvisionEntropy::new(entropy),
        &plan,
        &options,
        &serials,
        &keys,
        sector,
    )?;
    // A preserved partition's cipher text and original LCE are NEVER rewritten.
    // A new LCE would make retained key records semantically inconsistent.
    if dispositions.has_preserved_partitions() {
        let projection = source_prefix
            .iter()
            .flat_map(|block| block[..512].iter().copied())
            .collect::<Vec<_>>();
        let old_lce = crate::domain::geometry::parse_lba7_compatibility_geometry_with_sector_bytes(
            &projection,
            &device_id,
            total,
            sector,
        )
        .map_err(|e| format!("保留来源LCE尚未认证，拒绝写盘: {e}"))?;
        if old_lce.start_lba != compatibility.start_lba
            || old_lce.sector_count != compatibility.size_sectors
        {
            return Err("来源LCE原生范围与目标不一致，不能保留密文".into());
        }
        candidate.writes.retain(|write| {
            write.relative_lba < compatibility.start_lba
                || write.relative_lba >= compatibility.start_lba + compatibility.size_sectors
        });
        for (start, count) in dispositions.preserved_extents() {
            let end = start.checked_add(count).ok_or("保留区原生LBA溢出")?;
            if candidate
                .writes
                .iter()
                .any(|write| (start..end).contains(&write.relative_lba))
            {
                return Err(format!("保留区LBA{start}..{end}与写集合发生冲突，停止写盘"));
            }
        }
    }
    // OEM protocol owns its first 512B per native block; the remaining bytes
    // belong to the source media and are not disposable zero-padding.
    if source_prefix.len() == 13 && source_prefix.iter().all(|b| b.len() == sector as usize) {
        for write in candidate.writes.iter_mut().filter(|w| w.relative_lba < 13) {
            if sector > 512 {
                write.data[512..]
                    .copy_from_slice(&source_prefix[write.relative_lba as usize][512..]);
            }
        }
    }
    Ok((
        device_id,
        candidate,
        partitions,
        (compatibility.start_lba, compatibility.size_sectors),
        spec.metadata().onlyid().text().to_owned(),
    ))
}

/// Plain conversion must retire EDP registration LBAs. Updating only MBR
/// LBA0 leaves stale LBA7/LBA12 descriptors which conflict with the new MBR.
/// Manufacturer-owned LBA3 is retained; LBA0 remains the final commit block.
fn retire_edp_metadata_for_plain(
    plan: &mut NativeVirtualDiskPlan,
    source: DiskProvisionKind,
    source_prefix: &[Vec<u8>],
    device_id: &str,
) -> Result<(), String> {
    use crate::filesystem::NativeFilesystemWrite;
    use std::collections::BTreeMap;

    let mbr = plan.writes.pop().ok_or("Plain计划缺少MBR提交块")?;
    if mbr.relative_lba != 0 {
        return Err("Plain计划最后的块必须为LBA0".into());
    }
    let mut writes = plan
        .writes
        .drain(..)
        .map(|write| (write.relative_lba, write.data))
        .collect::<BTreeMap<_, _>>();
    for lba in 1..13 {
        if lba == 3 {
            continue;
        }
        writes.insert(lba, vec![0u8; plan.sector_bytes as usize]);
    }
    if source != DiskProvisionKind::Plain {
        let projection = source_prefix
            .iter()
            .flat_map(|block| block[..512].iter().copied())
            .collect::<Vec<_>>();
        let lce = crate::domain::geometry::parse_lba7_compatibility_geometry_with_sector_bytes(
            &projection,
            device_id,
            plan.total_sectors,
            plan.sector_bytes,
        )
        .map_err(|error| format!("已注册来源盘LCE清理地址不能确认: {error}"))?;
        for lba in lce.start_lba..lce.start_lba + lce.sector_count {
            writes
                .entry(lba)
                .or_insert_with(|| vec![0u8; plan.sector_bytes as usize]);
        }
    }
    plan.writes = writes
        .into_iter()
        .map(|(relative_lba, data)| NativeFilesystemWrite { relative_lba, data })
        .collect();
    plan.writes.push(mbr);
    Ok(())
}

pub fn prepare_native_provision_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &ProvisionRequest,
) -> Result<NativePreparedProvision, String> {
    // Resolve format intent before read-only evidence capture. The *source-
    // aware* plan below must reject any unverified preservation before writes.
    if let ProvisionRequest::Official(official) = request {
        let mode = official.target.official_mode().ok_or("目标不是官方模式")?;
        native_rebuild_format_policy(mode, official)?;
    }
    let session = crate::application::target_session::TargetSession::<
        crate::application::target_session::ReadOnly,
    >::open_usb(runner, disk)
    .map_err(|e| e.msg)?;
    let geometry = session.native_geometry().map_err(|e| e.msg)?;
    let probe = system::native_provision_probe(runner, disk)?;
    let device_id = TargetIdentity::from_probe(&probe, geometry.native_sector_count)
        .map(|v| v.device_id().to_owned())?;
    let mut dev =
        NativeRawBlockDevice::open_readonly(&crate::platform::raw_disk_path(disk), geometry)
            .map_err(|e| format!("读取设备原生块失败: {e}"))?;
    let mut prefix = Vec::with_capacity(13);
    for lba in 0..13 {
        prefix.push(dev.read_block_fresh(lba).map_err(|e| e.to_string())?);
    }
    let source = crate::provision::classify_native_source(
        &prefix,
        geometry.logical_sector_bytes,
        geometry.native_sector_count,
        &device_id,
    )?;
    let inherited_onlyid = if source != DiskProvisionKind::Plain {
        crate::infrastructure::backup_store::catalog::lba4_label_id_from(&prefix[4][..512])
    } else {
        None
    };
    // SourceSnapshot: keep native LBA0..12 and independently confirm each
    // existing partition's filesystem from its real native boot block. The
    // encrypted probe only decodes after a valid FileKey/CRC/password check.
    let mut parsed_source = if source != DiskProvisionKind::Plain {
        let native = crate::protocol::image::NativeProtocolImage::from_native_bytes(
            geometry.logical_sector_bytes,
            prefix
                .iter()
                .flat_map(|block| block.iter().copied())
                .collect(),
        )
        .map_err(|error| format!("注册盘原生协议构造失败: {error}"))?;
        crate::provision::parse_existing_provision_native(
            &native,
            &device_id,
            geometry.native_sector_count,
        )?
    } else {
        None
    };
    if let (Some(parsed), ProvisionRequest::Official(official)) = (&mut parsed_source, request) {
        for (index, part) in parsed.profile.partitions.iter_mut().enumerate() {
            if part.role == crate::provision::PartitionRole::CompatibilityReserve {
                continue;
            }
            let mut boot = dev
                .read_block_fresh(part.start_lba)
                .map_err(|e| format!("读取来源分区启动块失败: {e}"))?;
            if part.physically_encrypted {
                let record = parsed.records[index];
                let password = official
                    .key_domains
                    .source_password(part.role)
                    .unwrap_or(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD);
                let Ok(key) = record.verified_file_key(Some(password)) else {
                    continue; // Unknown source password: opaque preservation only.
                };
                let cipher =
                    crate::partition_transform::NativePartitionDataCipher::from_encrypt_mode(
                        record.lba12.encrypt_mode,
                    )?;
                boot = crate::partition_transform::transform_native_sector_offline(
                    cipher,
                    crate::partition_transform::NativeCipherDirection::Decrypt,
                    &boot,
                    &key,
                    part.start_lba,
                    geometry.logical_sector_bytes,
                )?;
            }
            part.filesystem = crate::filesystem::detect_native_boot_sector(
                &boot,
                part.sector_count,
                geometry.logical_sector_bytes,
            )
            .map_err(|e| format!("来源分区启动块解析失败: {e}"))?;
        }
    }
    let inherited_pass_info = parsed_source.as_ref().and_then(|p| p.pass_info_policy);
    let (target, mut plan, partitions, lce_extent, onlyid) = match request {
        ProvisionRequest::Plain(request) => {
            let mut plan = super::native_image::plan_native_plain_image(
                geometry.native_sector_count,
                geometry.logical_sector_bytes,
                &request.partitions,
            )?;
            retire_edp_metadata_for_plain(&mut plan, source, &prefix, &device_id)?;
            let partitions = if request.partitions.is_empty() {
                vec![NativePreviewPartition {
                    role: None,
                    start_lba: 2048,
                    sector_count: geometry.native_sector_count - 2048,
                    filesystem: Some(crate::filesystem::FilesystemKind::ExFat),
                    formatted: true,
                    physically_encrypted: false,
                }]
            } else {
                request
                    .partitions
                    .iter()
                    .map(|part| {
                        let next = request
                            .partitions
                            .iter()
                            .filter(|candidate| candidate.start_lba > part.start_lba)
                            .map(|candidate| candidate.start_lba)
                            .min()
                            .unwrap_or(geometry.native_sector_count);
                        let count = super::native_image::checked_size(
                            part.size,
                            geometry.logical_sector_bytes,
                        )?
                        .unwrap_or(next.saturating_sub(part.start_lba));
                        Ok(NativePreviewPartition {
                            role: None,
                            start_lba: part.start_lba,
                            sector_count: count,
                            filesystem: Some(part.filesystem),
                            formatted: true,
                            physically_encrypted: false,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?
            };
            (
                crate::provision::ProvisionTarget::Plain,
                plan,
                partitions,
                None,
                None,
            )
        }
        ProvisionRequest::Official(request) => {
            let (_, plan, partitions, lce, onlyid) = generate_official_native_plan(
                geometry.native_sector_count,
                geometry.logical_sector_bytes,
                &probe,
                request,
                inherited_onlyid.as_deref(),
                inherited_pass_info,
                parsed_source.as_ref(),
                &prefix,
            )?;
            (request.target, plan, partitions, Some(lce), Some(onlyid))
        }
    };
    if target != crate::provision::ProvisionTarget::Plain {
        if let Some(lba3) = plan.writes.iter_mut().find(|block| block.relative_lba == 3) {
            lba3.data.clone_from(&prefix[3]);
        }
    }
    let mut snapshot = crate::media_identity::MediaIdentitySnapshot::default();
    snapshot.hardware.logical_sector_size = Some(geometry.logical_sector_bytes);
    snapshot.hardware.total_sectors = Some(geometry.native_sector_count);
    snapshot.protocol.provision_kind = Some(source);
    let source_protocol_projection = (0..13)
        .flat_map(|i| prefix[i][..512].iter().copied())
        .collect::<Vec<_>>();
    let before_pin =
        crate::media_identity::MediaIdentityPin::new(snapshot, &source_protocol_projection);
    Ok(NativePreparedProvision {
        disk,
        source,
        target,
        device_id,
        plan,
        source_native_prefix: prefix,
        source_protocol_projection,
        before_pin,
        probe,
        onlyid,
        lce_extent,
        partitions,
    })
}

/// Common prepared-intent commit, reused directly by CLI and TUI.
pub fn commit_prepared_native_provision(
    runner: &dyn CmdRunner,
    prepared: &NativePreparedProvision,
    wal: &Path,
) -> Result<(), String> {
    super::native_commit::commit_native_plan_on_disk_with_source(
        runner,
        prepared.disk,
        &prepared.plan,
        wal,
        Some(&prepared.source_native_prefix),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserve_flags_never_get_rewritten_to_destructive_format_or_new_filekey() {
        use crate::application::provision::{FormatOptions, OfficialProvisionRequest};
        use crate::provision::{KeyDomainSecrets, OfficialPartitionMode as Mode, ProvisionTarget};
        let mut request = OfficialProvisionRequest {
            target: ProvisionTarget::Official(Mode::DefaultThreePartition),
            algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
            boot_start_lba: None,
            share_start_lba: None,
            encrypt_start_lba: None,
            boot_mib: None,
            boot_sectors: None,
            share_mib: None,
            share_sectors: None,
            encrypt_mib: None,
            encrypt_sectors: None,
            label_id: String::new(),
            user: String::new(),
            dept: String::new(),
            label: String::new(),
            lba8_identity: crate::provision::Lba8Identity::default(),
            key_domains: KeyDomainSecrets::default(),
            volume_label: String::new(),
            format: FormatOptions::default(),
            preserve_unformatted: true,
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
        };
        for (mode, roles) in [
            (
                Mode::DefaultThreePartition,
                &["启动区", "交换区", "保密区"][..],
            ),
            (Mode::BootShareCombined, &["二合一区", "保密区"][..]),
            (Mode::WholeDiskEncrypted, &["保密区"][..]),
            (
                Mode::IntranetExtranetDualPartition,
                &["启动区", "交换区"][..],
            ),
        ] {
            let mut chosen = native_rebuild_format_policy(mode, &request).unwrap();
            assert!(!chosen.boot && !chosen.share && !chosen.encrypt);
            let error =
                native_source_aware_targets(mode, 512, 4_000_000, &request, &mut chosen, None)
                    .unwrap_err();
            assert!(
                error.contains("停止写盘") || error.contains("无法无损保留"),
                "{error}"
            );
            for _role in roles {
                // Every actual preserved region requires an independently
                // confirmed source profile; no implicit conversion to format.
                assert!(!chosen.boot && !chosen.share && !chosen.encrypt);
            }
        }
        // CLI's unselected *default* means a deliberate destructive rebuild,
        // not a promise to preserve existing ciphertext or key material.
        request.preserve_unformatted = false;
        for mode in [
            Mode::DefaultThreePartition,
            Mode::BootShareCombined,
            Mode::WholeDiskEncrypted,
            Mode::IntranetExtranetDualPartition,
        ] {
            let chosen = native_rebuild_format_policy(mode, &request).unwrap();
            assert_eq!(
                chosen.boot,
                matches!(
                    mode,
                    Mode::DefaultThreePartition | Mode::IntranetExtranetDualPartition
                )
            );
            assert_eq!(chosen.share, mode != Mode::WholeDiskEncrypted);
            assert_eq!(chosen.encrypt, mode != Mode::IntranetExtranetDualPartition);
        }
        // Partial rebuild requires a compatible source for each retained region.
        request.format.share = true;
        let mut chosen =
            native_rebuild_format_policy(Mode::DefaultThreePartition, &request).unwrap();
        assert!(native_source_aware_targets(
            Mode::DefaultThreePartition,
            512,
            4_000_000,
            &request,
            &mut chosen,
            None,
        )
        .is_err());
        request.preserve_unformatted = true;
        request.format.boot = true;
        request.format.encrypt = true;
        assert!(native_rebuild_format_policy(Mode::DefaultThreePartition, &request).is_ok());
    }

    fn offline_request(mode: crate::provision::OfficialPartitionMode) -> OfficialProvisionRequest {
        use crate::provision::{KeyDomainSecrets, ProvisionTarget};
        OfficialProvisionRequest {
            target: ProvisionTarget::Official(mode),
            algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
            boot_start_lba: None,
            share_start_lba: None,
            encrypt_start_lba: None,
            boot_mib: None,
            boot_sectors: None,
            share_mib: None,
            share_sectors: None,
            encrypt_mib: None,
            encrypt_sectors: None,
            label_id: String::new(),
            user: String::new(),
            dept: String::new(),
            label: String::new(),
            lba8_identity: crate::provision::Lba8Identity::default(),
            key_domains: KeyDomainSecrets::default(),
            volume_label: String::new(),
            format: FormatOptions::default(),
            preserve_unformatted: true,
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
        }
    }

    fn test_native_source(
        sector: u32,
        mode: crate::provision::OfficialPartitionMode,
    ) -> (
        HardwareProbe,
        crate::provision::ParsedExistingProvision,
        Vec<Vec<u8>>,
        String,
    ) {
        use crate::platform::{InquiryInfo, NativeTransport};
        use crate::provision::OfficialPartitionMode as Mode;
        let probe = HardwareProbe {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            transport: NativeTransport::Uas,
            windows_pnp_instance_id: None,
            inquiry: Some(InquiryInfo {
                vendor: "Netac".into(),
                product: "OnlyDisk".into(),
                revision: "1.00".into(),
            }),
        };
        let mut request = offline_request(mode);
        request.preserve_unformatted = false;
        request.format.boot = matches!(
            mode,
            Mode::DefaultThreePartition | Mode::IntranetExtranetDualPartition
        );
        request.format.share = mode != Mode::WholeDiskEncrypted;
        request.format.encrypt = mode != Mode::IntranetExtranetDualPartition;
        let total = 1_073_741_824 / u64::from(sector);
        let blank = vec![vec![0u8; sector as usize]; 13];
        let (did, fresh, previews, _lce, onlyid) = generate_official_native_plan(
            total, sector, &probe, &request, None, None, None, &blank,
        )
        .unwrap();
        assert!(onlyid.len() >= 8);
        let prefix = (0..13)
            .map(|lba| {
                fresh
                    .writes
                    .iter()
                    .find(|w| w.relative_lba == lba)
                    .unwrap()
                    .data
                    .clone()
            })
            .collect::<Vec<_>>();
        let native = crate::protocol::image::NativeProtocolImage::from_native_bytes(
            sector,
            prefix.iter().flat_map(|b| b.iter().copied()).collect(),
        )
        .unwrap();
        let mut parsed = crate::provision::parse_existing_provision_native(&native, &did, total)
            .unwrap()
            .unwrap();
        for (source, preview) in parsed.profile.partitions.iter_mut().zip(&previews) {
            source.filesystem = preview.filesystem;
        }
        (probe, parsed, prefix, did)
    }

    #[test]
    fn native_source_aware_preserves_filekey_ciphertext_and_lce_in_512_and_4kn() {
        use crate::provision::{OfficialPartitionMode as Mode, PartitionRole};
        for sector in [512, 1024, 2048, 4096] {
            for (from, to, keep_all) in [
                (Mode::BootShareCombined, Mode::BootShareCombined, true),
                (Mode::DefaultThreePartition, Mode::BootShareCombined, false),
            ] {
                let (probe, parsed, prefix, did) = test_native_source(sector, from);
                let mut request = offline_request(to);
                if !keep_all {
                    request.format.share = true;
                }
                let total = 1_073_741_824 / u64::from(sector);
                let (_, target, preview, lce, _) = generate_official_native_plan(
                    total,
                    sector,
                    &probe,
                    &request,
                    None,
                    parsed.pass_info_policy,
                    Some(&parsed),
                    &prefix,
                )
                .unwrap();
                let src = parsed.record(PartitionRole::Encrypt).unwrap();
                let target_prefix = (0..13)
                    .map(|lba| {
                        target
                            .writes
                            .iter()
                            .find(|w| w.relative_lba == lba)
                            .unwrap()
                            .data
                            .clone()
                    })
                    .collect::<Vec<_>>();
                let native = crate::protocol::image::NativeProtocolImage::from_native_bytes(
                    sector,
                    target_prefix
                        .iter()
                        .flat_map(|b| b.iter().copied())
                        .collect(),
                )
                .unwrap();
                let restored =
                    crate::provision::parse_existing_provision_native(&native, &did, total)
                        .unwrap()
                        .unwrap();
                let retained = restored.record(PartitionRole::Encrypt).unwrap();
                assert_eq!(
                    src.lba12_key_material().unwrap(),
                    retained.lba12_key_material().unwrap()
                );
                assert_eq!(src.lba7_key_material(), retained.lba7_key_material());
                let old = parsed.profile.partition(PartitionRole::Encrypt).unwrap();
                assert!(!target.writes.iter().any(|w| {
                    (old.start_lba..old.start_lba + old.sector_count).contains(&w.relative_lba)
                }));
                assert!(!target
                    .writes
                    .iter()
                    .any(|w| { (lce.0..lce.0 + lce.1).contains(&w.relative_lba) }));
                assert!(
                    !preview
                        .iter()
                        .find(|p| p.role == Some(PartitionRole::Encrypt))
                        .unwrap()
                        .formatted
                );
                if keep_all {
                    assert!(preview.iter().all(|p| !p.formatted));
                }
                for index in 0..13 {
                    assert_eq!(&target_prefix[index][512..], &prefix[index][512..]);
                }
            }
        }
    }

    #[test]
    fn native_rewrap_keeps_same_filekey_without_reformat() {
        use crate::provision::{KeyDomainSecretPair, OfficialPartitionMode as Mode, PartitionRole};
        for sector in [512, 1024, 2048, 4096] {
            let (probe, parsed, prefix, did) = test_native_source(sector, Mode::BootShareCombined);
            let mut request = offline_request(Mode::BootShareCombined);
            let replacement = b"NewPassword2026!";
            request.key_domains.encrypt = KeyDomainSecretPair::new(
                Some(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD),
                Some(replacement.as_slice()),
            );
            let total = 1_073_741_824 / u64::from(sector);
            let (_, plan, preview, lce, _) = generate_official_native_plan(
                total,
                sector,
                &probe,
                &request,
                None,
                parsed.pass_info_policy,
                Some(&parsed),
                &prefix,
            )
            .unwrap();
            assert!(preview.iter().all(|p| !p.formatted));
            assert!(!plan
                .writes
                .iter()
                .any(|w| { (lce.0..lce.0 + lce.1).contains(&w.relative_lba) }));
            let raw = (0..13)
                .flat_map(|lba| {
                    plan.writes
                        .iter()
                        .find(|w| w.relative_lba == lba)
                        .unwrap()
                        .data
                        .iter()
                        .copied()
                })
                .collect();
            let native =
                crate::protocol::image::NativeProtocolImage::from_native_bytes(sector, raw)
                    .unwrap();
            let target = crate::provision::parse_existing_provision_native(&native, &did, total)
                .unwrap()
                .unwrap();
            let before = parsed
                .record(PartitionRole::Encrypt)
                .unwrap()
                .verified_file_key(Some(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD))
                .unwrap();
            let after = target
                .record(PartitionRole::Encrypt)
                .unwrap()
                .verified_file_key(Some(replacement))
                .unwrap();
            assert_eq!(before, after);
            assert!(target
                .record(PartitionRole::Encrypt)
                .unwrap()
                .verified_file_key(Some(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD))
                .is_err());
        }
    }

    #[test]
    fn native_preservation_rejects_wrong_password_geometry_and_cipher() {
        use crate::provision::{KeyDomainSecretPair, OfficialPartitionMode as Mode, PartitionRole};
        for sector in [512, 1024, 2048, 4096] {
            let (probe, parsed, prefix, _) = test_native_source(sector, Mode::BootShareCombined);
            let total = 1_073_741_824 / u64::from(sector);

            let mut bad_password = offline_request(Mode::BootShareCombined);
            bad_password.key_domains.encrypt = KeyDomainSecretPair::new(
                Some(b"wrong-old-password".as_slice()),
                Some(b"changed-password".as_slice()),
            );
            let error = generate_official_native_plan(
                total,
                sector,
                &probe,
                &bad_password,
                None,
                parsed.pass_info_policy,
                Some(&parsed),
                &prefix,
            )
            .unwrap_err();
            assert!(
                error.contains("密码") || error.contains("无损"),
                "{sector}: {error}"
            );

            let mut bad_geometry = offline_request(Mode::BootShareCombined);
            let old = parsed.profile.partition(PartitionRole::Encrypt).unwrap();
            bad_geometry.encrypt_start_lba = Some(old.start_lba + 1);
            assert!(
                generate_official_native_plan(
                    total,
                    sector,
                    &probe,
                    &bad_geometry,
                    None,
                    parsed.pass_info_policy,
                    Some(&parsed),
                    &prefix,
                )
                .is_err(),
                "{sector}: moved encrypted region must not be silently preserved"
            );

            let mut bad_cipher = parsed.clone();
            let encrypt_idx = bad_cipher
                .profile
                .partitions
                .iter()
                .position(|part| part.role == PartitionRole::Encrypt)
                .unwrap();
            bad_cipher.records[encrypt_idx].lba12.encrypt_mode =
                crate::provision::FileKeyWrapMode::Aes128Ecb.raw();
            assert!(
                generate_official_native_plan(
                    total,
                    sector,
                    &probe,
                    &offline_request(Mode::BootShareCombined),
                    None,
                    bad_cipher.pass_info_policy,
                    Some(&bad_cipher),
                    &prefix,
                )
                .is_err(),
                "{sector}: algorithm mismatch cannot preserve old ciphertext"
            );
        }
    }

    #[test]
    fn geometry_locator_matches_lce_512_and_4kn() {
        for sector in [512, 1024, 2048, 4096, 8192] {
            let total = 262_144;
            let lce = native_compatibility_extent(total, sector).unwrap();
            assert!(lce.start_lba < total);
            assert!(lce.start_lba > 13);
            assert_eq!(lce.size_bytes % u64::from(sector), 0);
        }
    }
}
