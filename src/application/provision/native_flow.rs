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

fn generate_official_native_plan(
    total: u64,
    sector: u32,
    probe: &HardwareProbe,
    request: &OfficialProvisionRequest,
    inherited_onlyid: Option<&str>,
    inherited_pass_info: Option<PassInfoPolicy>,
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
        request.boot_mib.unwrap_or(32),
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

    let mut options: FormatOptions = request.format.clone();
    // Fresh destructive provisioning cannot leave unformatted ciphertext
    // under a newly generated FileKey. When nothing is explicitly selected,
    // initialize all present roles rather than producing unusable partitions.
    if !options.boot && !options.share && !options.encrypt {
        options.boot = matches!(
            mode,
            crate::provision::OfficialPartitionMode::DefaultThreePartition
                | crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition
        );
        options.share = mode != crate::provision::OfficialPartitionMode::WholeDiskEncrypted;
        options.encrypt =
            mode != crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition;
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

    let targets = plan.format_targets_native(sector)?;
    let mut serials = Vec::with_capacity(targets.len());
    let mut keys = Vec::with_capacity(targets.len());
    for (index, target) in targets.iter().enumerate() {
        let mut serial = [0u8; 4];
        getrandom::fill(&mut serial).map_err(|e| format!("卷序列号生成失败: {e}"))?;
        serials.push(u32::from_le_bytes(serial));
        let mut file_key = [0u8; 16];
        getrandom::fill(&mut file_key).map_err(|e| format!("分区FileKey生成失败: {e}"))?;
        let mut legacy_key = [0u8; 8];
        getrandom::fill(&mut legacy_key).map_err(|e| format!("分区LBA7密钥生成失败: {e}"))?;
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
    let candidate = super::native_image::plan_native_edp_image(
        &spec,
        &ProvisionEntropy::new(entropy),
        &plan,
        &options,
        &serials,
        &keys,
        sector,
    )?;
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
    let inherited_pass_info = if source != DiskProvisionKind::Plain {
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
        .and_then(|parsed| parsed.pass_info_policy)
    } else {
        None
    };
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
