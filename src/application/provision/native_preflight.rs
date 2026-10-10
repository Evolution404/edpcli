//! 4Kn 实盘制盘计划的只读来源认证与完整原生写集预检。
//! 此模块只读磁盘、只读经校验的 EDPB，不取得卸载或写入权限。
use crate::filesystem::NativeVirtualDiskPlan;
use crate::ports::CmdRunner;
use sha2::{Digest, Sha256};
use std::path::Path;

/// 使用认证来源 EDPB 构造完整 Mode1 原生写集；不会操作任何 USB 设备。
pub fn plan_native_mode1_from_backup(backup: &Path) -> Result<NativeVirtualDiskPlan, String> {
    use crate::application::evidence::{EvidenceSource, SectorReader};
    use crate::platform::{HardwareProbe, InquiryInfo, NativeTransport};
    use crate::provision::{
        OnlyId, ProvisionEntropy, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
        TargetIdentity,
    };

    let mut source = EvidenceSource::open_backup(backup)
        .map_err(|error| format!("4Kn EDPB备份校验失败: {error}"))?;
    if source
        .backup_manifest()
        .is_none_or(|m| m.schema != "edpb.manifest.v4")
        || source.logical_sector_bytes() != 4096
        || source.native_protocol_image().is_none()
    {
        return Err("Mode1来源必须是完整的4Kn EDPB v4原生备份".into());
    }
    let identity = source.identity();
    let did = identity.device_id.as_deref().ok_or("EDPB没有可信设备ID")?;
    let device_parts = did
        .strip_prefix("disk&ven_")
        .and_then(|value| value.split_once("&prod_"))
        .ok_or("EDPB来源ID不是已验证的Windows设备身份格式")?;
    if device_parts.0.is_empty() || device_parts.1.is_empty() {
        return Err("EDPB供应商或产品身份无效".into());
    }
    let vid = u16::from_str_radix(identity.vid.as_deref().ok_or("EDPB缺少VID")?, 16)
        .map_err(|_| "EDPB VID不是十六进制")?;
    let pid = u16::from_str_radix(identity.pid.as_deref().ok_or("EDPB缺少PID")?, 16)
        .map_err(|_| "EDPB PID不是十六进制")?;
    let onlyid = OnlyId::parse(identity.onlyid.as_deref().ok_or("EDPB缺少OnlyId")?)?;
    let total = source.total_sectors();
    if identity.size_bytes != total.checked_mul(4096) {
        return Err("EDPB原生容量不完整或存在冲突".into());
    }
    let probe = HardwareProbe {
        vid: Some(vid),
        pid: Some(pid),
        transport: NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(InquiryInfo {
            vendor: device_parts.0.into(),
            product: device_parts.1.into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total)?;
    if target.device_id() != did {
        return Err("EDPB硬件身份无法由原始供应商和产品信息一致重建".into());
    }
    let snapshot = source
        .native_protocol_image()
        .ok_or("缺少原生EDPF快照")?
        .clone();
    let parsed = crate::provision::parse_existing_provision_native(&snapshot, did, total)?
        .ok_or("EDPB来源未确认注册Mode0")?;
    let pass_info = parsed.pass_info_policy.ok_or("EDPB来源PassInfo尚未确认")?;
    let metadata = ProvisionMetadata::new(
        onlyid,
        "SOURCE PRESERVED",
        "SOURCE PRESERVED",
        crate::provision::DEFAULT_SAFE6_LABEL,
    )?;
    let spec = ProvisionSpec::new(
        target,
        metadata,
        ProvisionProfile::canonical_v1().with_pass_info_policy(pass_info),
    )?;
    let options = crate::application::provision::FormatOptions {
        share: true,
        share_label: "启动区".into(),
        ..Default::default()
    };
    // Newly created plaintext ExFAT serial is not the encrypted volume key.
    // The protocol and original type4 key material remain exactly source-owned.
    let mut serial = [0u8; 4];
    getrandom::fill(&mut serial).map_err(|error| format!("生成新卷序列号失败: {error}"))?;
    let writes = super::native_image::plan_verified_native_4kn_mode0_to_mode1(
        &mut source,
        &snapshot,
        &spec,
        &ProvisionEntropy::new([0x5a; 252]),
        &options,
        u32::from_le_bytes(serial),
    )?;
    Ok(writes)
}

/// 只读绑定的完整原生计划：没有物理写权限，不得作为直接提交凭据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Native4knReadOnlyPreflight {
    pub disk: u32,
    pub device_identity: String,
    pub source_backup_sha256: String,
    pub planned_write_sha256: String,
    pub verified_original_blocks: usize,
    pub write_blocks: usize,
    pub format_block_count: usize,
    pub logical_sector_bytes: u32,
    pub total_sectors: u64,
    pub first_partition_lba: u64,
    pub first_partition_sectors: u64,
    pub preserved_encrypted_partition_lba: u64,
}

/// 一个不可变的来源绑定原生写集。不能取得写入设备句柄。后续受控事务
/// 必须复用此计划，不能在提交时通过重新随机化再次生成不同写集。
#[derive(Debug)]
pub struct PreparedNativeMode1 {
    summary: Native4knReadOnlyPreflight,
    plan: NativeVirtualDiskPlan,
}

impl PreparedNativeMode1 {
    pub fn summary(&self) -> &Native4knReadOnlyPreflight {
        &self.summary
    }
    pub fn planned_blocks(&self) -> &NativeVirtualDiskPlan {
        &self.plan
    }
}

/// 只读取证与当前设备匹配之后，生成一次且仅一次不可变的完整写集。
/// 此操作不执行卸载、锁盘或实体写入。
pub fn prepare_native_mode1_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    source_backup: &Path,
) -> Result<PreparedNativeMode1, String> {
    let checked = crate::application::evidence::verify_native_4kn_backup_against_disk_readonly(
        runner,
        disk,
        source_backup,
    )?;
    if checked.logical_sector_bytes != 4096 || checked.verified_native_blocks < 15 {
        return Err("4Kn来源EDPB原始块不足或原生几何不匹配".into());
    }
    let source = crate::application::evidence::EvidenceSource::open_backup(source_backup)
        .map_err(|error| format!("EDPB二次校验失败: {error}"))?;
    let identity = source
        .identity()
        .device_id
        .as_deref()
        .ok_or("EDPB来源没有可信设备身份")?
        .to_owned();
    let snapshot = source
        .native_protocol_image()
        .ok_or("EDPB缺少完整原生协议")?;
    let parsed = crate::provision::parse_existing_provision_native(
        snapshot,
        &identity,
        checked.total_sectors,
    )?
    .ok_or("来源不是已注册EDP磁盘")?;
    if parsed.profile.source_mode != crate::provision::OfficialPartitionMode::DefaultThreePartition
        || parsed.profile.partitions.len() != 3
    {
        return Err("当前4Kn来源不是Mode0；禁止复用Mode0→Mode1写盘计划".into());
    }
    let encrypted_lba = parsed.profile.partitions[2].start_lba;
    let plan = plan_native_mode1_from_backup(source_backup)?;
    if plan.sector_bytes != checked.logical_sector_bytes
        || plan.total_sectors != checked.total_sectors
        || plan
            .writes
            .last()
            .is_none_or(|write| write.relative_lba != 0)
    {
        return Err("来源认证后制盘写集几何或MBR提交顺序变化".into());
    }
    let mut hasher = Sha256::new();
    let mut unique = std::collections::BTreeSet::new();
    for write in &plan.writes {
        if write.relative_lba >= plan.total_sectors
            || write.data.len() != 4096
            || !unique.insert(write.relative_lba)
        {
            return Err(format!(
                "Mode1写集块重复、截断或越界: LBA{}",
                write.relative_lba
            ));
        }
        // 原LCE是唯一允许保留在保密区边界之后的完整原生块。
        if write.relative_lba >= encrypted_lba
            && write.relative_lba != parsed.records[2].lba7.start_sector
        {
            return Err(format!("Mode1写集触及原保密区: LBA{}", write.relative_lba));
        }
        hasher.update(write.relative_lba.to_le_bytes());
        hasher.update(&write.data);
    }
    let first = &plan.writes.last().ok_or("Mode1写集缺少MBR")?.data;
    if first.len() != 4096 || first[510..512] != [0x55, 0xaa] || first[450] != 7 {
        return Err("Mode1目标MBR签名或ExFAT分区类型不匹配".into());
    }
    let first_lba = u32::from_le_bytes(first[454..458].try_into().unwrap()) as u64;
    let first_count = u32::from_le_bytes(first[458..462].try_into().unwrap()) as u64;
    let combined_limit = parsed.profile.partitions[1]
        .start_lba
        .checked_add(parsed.profile.partitions[1].sector_count)
        .ok_or("来源交换区末端溢出")?;
    if first_lba != 63 || first_count != combined_limit - 63 {
        return Err("Mode1新分区超过经过认证的原交换区边界".into());
    }
    if std::fs::metadata(source_backup)
        .map_err(|error| format!("EDPB最终读取元数据失败: {error}"))?
        .len()
        > 128 * 1024 * 1024
    {
        return Err("原生EDPB文件超过128MiB来源预算".into());
    }
    let backup_bytes =
        std::fs::read(source_backup).map_err(|error| format!("EDPB最终读取失败: {error}"))?;
    let format_block_count = plan
        .writes
        .iter()
        .filter(|write| write.relative_lba >= 63 && write.relative_lba < combined_limit)
        .count();
    Ok(PreparedNativeMode1 {
        summary: Native4knReadOnlyPreflight {
            disk,
            device_identity: identity,
            source_backup_sha256: Sha256::digest(&backup_bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            planned_write_sha256: hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            verified_original_blocks: checked.verified_native_blocks,
            write_blocks: plan.writes.len(),
            format_block_count,
            logical_sector_bytes: checked.logical_sector_bytes,
            total_sectors: checked.total_sectors,
            first_partition_lba: first_lba,
            first_partition_sectors: first_count,
            preserved_encrypted_partition_lba: encrypted_lba,
        },
        plan,
    })
}

/// 兼容 CLI/TUI 的通用只读预检入口；不会授权提交。
pub fn preflight_native_mode1_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    source_backup: &Path,
) -> Result<Native4knReadOnlyPreflight, String> {
    prepare_native_mode1_on_disk(runner, disk, source_backup).map(|prepared| prepared.summary)
}
