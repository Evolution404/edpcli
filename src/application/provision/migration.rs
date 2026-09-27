use super::*;
use crate::backup_deep::{
    analyze_partition, stream_file_payload, AnalysisStatus, FileEntry, PartitionReader,
};
use crate::provision::{
    build_migration_manifest, finalize_staged_entry, Extent, FilesystemProfile, MigrationBudgets,
    MigrationInventory, MigrationManifestEntry, MigrationSource, MigrationStagedEntry,
    MigrationTransform, PhysicalCryptoProfile, SourceRegion,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedMigrationTarget {
    pub target_index: usize,
    pub role: PartitionRole,
    pub entries: Vec<MigrationStagedEntry>,
    pub total_logical_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

struct MigrationPartitionReader<'a> {
    dev: &'a mut dyn SectorDev,
    start_lba: u64,
    sector_count: u64,
    file_key: Option<[u8; 16]>,
}

impl PartitionReader for MigrationPartitionReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        use std::io::{Error, ErrorKind};
        if relative_lba >= self.sector_count {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "migration read outside source partition",
            ));
        }
        let absolute = self
            .start_lba
            .checked_add(relative_lba)
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| Error::other("migration source LBA overflow"))?;
        let raw = self.dev.read_sector(absolute)?;
        if raw.len() != SECTOR {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "migration source sector is truncated",
            ));
        }
        match self.file_key {
            Some(key) => {
                crate::backup_deep::keys::decrypt_mode2(&raw, &key).map_err(std::io::Error::other)
            }
            None => Ok(raw),
        }
    }
}

fn source_file_key(
    source: &ParsedExistingProvision,
    source_index: usize,
    key_domains: &KeyDomainSecrets,
) -> EdpCliResult<Option<[u8; 16]>> {
    let part = source
        .profile
        .partitions
        .get(source_index)
        .ok_or_else(|| err(EXIT_TARGET, "错误: K6 来源分区索引越界"))?;
    if !part.physically_encrypted {
        return Ok(None);
    }
    let domain = KeyDomainRole::from_partition_role(part.role).ok_or_else(|| {
        err(
            EXIT_TARGET,
            format!("错误: {} 加密来源没有可验证的密码域", part.role.label()),
        )
    })?;
    let supplied = key_domains.source_password(part.role);
    let knowledge = source.source_password_knowledge(domain, supplied);
    let password = match knowledge {
        SourcePasswordKnowledge::DefaultVerified => Some(DEFAULT_KEY_DOMAIN_PASSWORD),
        SourcePasswordKnowledge::UserVerified => supplied,
        SourcePasswordKnowledge::Unknown => None,
    }
    .ok_or_else(|| {
        err(
            EXIT_TARGET,
            format!(
                "错误: {} K6 数据迁移需要已验证来源密码，不能从未知 key material 解密迁移",
                part.role.label()
            ),
        )
    })?;
    let record = source
        .records
        .get(source_index)
        .ok_or_else(|| err(EXIT_TARGET, "错误: K6 来源 key record 缺失"))?;
    record
        .verified_sm4_file_key(password)
        .map(Some)
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!(
                    "错误: {} K6 来源 FileKey 验证失败: {message}",
                    part.role.label()
                ),
            )
        })
}

fn source_geometry(region: SourceRegion, index: usize) -> EdpCliResult<PartitionGeometry> {
    let size = region
        .extent
        .sector_count
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| err(EXIT_TARGET, "错误: K6 来源分区字节数溢出"))?;
    Ok(PartitionGeometry {
        index,
        partition_type: region.partition_type,
        partition_count: 1,
        need_disturb: 0,
        // MigrationPartitionReader exposes plaintext after verified decryption.
        need_encrypt: 0,
        start_sector: region.extent.start_lba,
        sector_size: SECTOR as u64,
        partition_size: size,
        sector_count: region.extent.sector_count,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    })
}

fn inventory_source(
    dev: &mut dyn SectorDev,
    source: &ParsedExistingProvision,
    source_index: usize,
    region: SourceRegion,
    key_domains: &KeyDomainSecrets,
) -> EdpCliResult<MigrationInventory> {
    let mut key = source_file_key(source, source_index, key_domains)?;
    let mut reader = MigrationPartitionReader {
        dev,
        start_lba: region.extent.start_lba,
        sector_count: region.extent.sector_count,
        file_key: key,
    };
    let geometry = source_geometry(region, source_index)?;
    let report = analyze_partition(&geometry, &mut reader);
    if let Some(key) = key.as_mut() {
        key.fill(0);
    }
    if report.status != AnalysisStatus::Parsed {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: {} K6 来源文件系统无法完整解析: {}",
                region.role.label(),
                report.reason
            ),
        ));
    }
    let entries = report.entries.ok_or_else(|| {
        err(
            EXIT_TARGET,
            format!("错误: {} K6 来源解析未返回文件清单", region.role.label()),
        )
    })?;
    Ok(MigrationInventory {
        source_index,
        region,
        entries,
    })
}

fn stage_manifest_entry(
    dev: &mut dyn SectorDev,
    source: &ParsedExistingProvision,
    key_domains: &KeyDomainSecrets,
    manifest: &crate::provision::MigrationManifestEntry,
) -> EdpCliResult<MigrationStagedEntry> {
    if manifest.is_directory {
        return finalize_staged_entry(manifest, Vec::new())
            .map_err(|message| err(EXIT_TARGET, format!("错误: {message}")));
    }

    let locator = manifest.payload_locator.clone().ok_or_else(|| {
        err(
            EXIT_TARGET,
            format!("错误: K6 文件 {:?} 缺少 payload locator", manifest.path),
        )
    })?;
    let entry = FileEntry {
        path: manifest.path.clone(),
        is_directory: false,
        logical_size: manifest.logical_size,
        allocated_size: None,
        mtime: manifest.mtime.clone(),
        ctime: manifest.ctime.clone(),
        attributes: manifest.attributes,
        payload_locator: Some(locator),
    };

    let mut key = source_file_key(source, manifest.source_index, key_domains)?;
    let mut reader = MigrationPartitionReader {
        dev,
        start_lba: manifest.source_region.extent.start_lba,
        sector_count: manifest.source_region.extent.sector_count,
        file_key: key,
    };
    let capacity = usize::try_from(manifest.logical_size).map_err(|_| {
        err(
            EXIT_TARGET,
            format!(
                "错误: K6 文件 {:?} 太大，当前进程无法建立 staging buffer",
                manifest.path
            ),
        )
    })?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|_| {
        err(
            EXIT_TARGET,
            format!(
                "错误: K6 文件 {:?} staging 内存预留失败，写盘尚未开始",
                manifest.path
            ),
        )
    })?;
    let summary = stream_file_payload(&mut reader, &entry, manifest.logical_size, &mut bytes)
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: K6 文件 {:?} staging 失败: {message}", manifest.path),
            )
        })?;
    if let Some(key) = key.as_mut() {
        key.fill(0);
    }
    if summary.logical_size != manifest.logical_size || bytes.len() as u64 != manifest.logical_size
    {
        return Err(err(
            EXIT_TARGET,
            format!("错误: K6 文件 {:?} staging 长度校验失败", manifest.path),
        ));
    }
    finalize_staged_entry(manifest, bytes)
        .map_err(|message| err(EXIT_TARGET, format!("错误: {message}")))
}

#[derive(Clone, Debug)]
pub(super) struct PlainImportPlan {
    pub target_index: usize,
    pub sources: Vec<MigrationSource>,
    pub prepared: PreparedMigrationTarget,
}

#[derive(Clone, Copy, Debug)]
struct PlainSourcePartition {
    index: usize,
    partition_type: u8,
    start_lba: u64,
    sector_count: u64,
}

fn parse_plain_mbr(dev: &mut dyn SectorDev) -> EdpCliResult<Vec<PlainSourcePartition>> {
    let sector = dev.read_sector(0).map_err(|error| {
        err(
            EXIT_TARGET,
            format!("错误: K6 Plain 来源 MBR 读取失败: {error}"),
        )
    })?;
    if sector.len() != SECTOR || sector[510..512] != [0x55, 0xaa] {
        return Err(err(EXIT_TARGET, "错误: K6 Plain 来源没有有效 MBR 签名"));
    }
    let mut partitions = Vec::new();
    for index in 0..4 {
        let at = 0x1be + index * 16;
        let partition_type = sector[at + 4];
        let start_lba = u32::from_le_bytes(sector[at + 8..at + 12].try_into().unwrap()) as u64;
        let sector_count = u32::from_le_bytes(sector[at + 12..at + 16].try_into().unwrap()) as u64;
        if partition_type == 0 || sector_count == 0 {
            continue;
        }
        if start_lba == 0 {
            return Err(err(
                EXIT_TARGET,
                format!("错误: K6 Plain P{} 非法占用 LBA0", index + 1),
            ));
        }
        start_lba.checked_add(sector_count).ok_or_else(|| {
            err(
                EXIT_TARGET,
                format!("错误: K6 Plain P{} 范围溢出", index + 1),
            )
        })?;
        partitions.push(PlainSourcePartition {
            index,
            partition_type,
            start_lba,
            sector_count,
        });
    }
    if partitions.is_empty() {
        return Err(err(EXIT_TARGET, "错误: K6 Plain 来源没有可迁移的 MBR 分区"));
    }
    partitions.sort_by_key(|partition| partition.start_lba);
    for pair in partitions.windows(2) {
        if pair[0].start_lba + pair[0].sector_count > pair[1].start_lba {
            return Err(err(EXIT_TARGET, "错误: K6 Plain 来源 MBR 分区互相重叠"));
        }
    }
    Ok(partitions)
}

fn analyze_plain_partition(
    dev: &mut dyn SectorDev,
    source: PlainSourcePartition,
    role: PartitionRole,
) -> EdpCliResult<(SourceRegion, Vec<FileEntry>)> {
    let region = SourceRegion {
        role,
        partition_type: source.partition_type as u32,
        extent: Extent {
            start_lba: source.start_lba,
            sector_count: source.sector_count,
        },
        physical_crypto: PhysicalCryptoProfile::Plain,
        filesystem: FilesystemProfile::Unknown,
        key_profile: None,
    };
    let mut reader = MigrationPartitionReader {
        dev,
        start_lba: source.start_lba,
        sector_count: source.sector_count,
        file_key: None,
    };
    let geometry = source_geometry(region, source.index)?;
    let report = analyze_partition(&geometry, &mut reader);
    if report.status != AnalysisStatus::Parsed {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: K6 Plain P{} 文件系统无法完整解析: {}",
                source.index + 1,
                report.reason
            ),
        ));
    }
    Ok((
        SourceRegion {
            filesystem: report
                .filesystem
                .as_deref()
                .and_then(|name| match name {
                    "fat16" => Some(OfficialFilesystemFormat::Fat16),
                    "fat32" => Some(OfficialFilesystemFormat::Fat32),
                    "exfat" => Some(OfficialFilesystemFormat::ExFat),
                    "ntfs" => Some(OfficialFilesystemFormat::Ntfs),
                    _ => None,
                })
                .map(FilesystemProfile::Known)
                .unwrap_or(FilesystemProfile::Unknown),
            ..region
        },
        report.entries.unwrap_or_default(),
    ))
}

fn stage_plain_file(
    dev: &mut dyn SectorDev,
    source: PlainSourcePartition,
    entry: &FileEntry,
    target_path: String,
) -> EdpCliResult<MigrationStagedEntry> {
    if entry.is_directory {
        return Ok(MigrationStagedEntry {
            source_index: source.index,
            transform: MigrationTransform::PlainToEdp,
            path: target_path,
            is_directory: true,
            data: Vec::new(),
            attributes: entry.attributes,
            mtime: entry.mtime.clone(),
            ctime: entry.ctime.clone(),
        });
    }
    let mut reader = MigrationPartitionReader {
        dev,
        start_lba: source.start_lba,
        sector_count: source.sector_count,
        file_key: None,
    };
    let capacity = usize::try_from(entry.logical_size)
        .map_err(|_| err(EXIT_TARGET, "错误: K6 Plain 文件太大，无法 staging"))?;
    let mut data = Vec::new();
    data.try_reserve_exact(capacity)
        .map_err(|_| err(EXIT_TARGET, "错误: K6 Plain 文件 staging 内存预留失败"))?;
    stream_file_payload(&mut reader, entry, entry.logical_size, &mut data).map_err(|message| {
        err(
            EXIT_TARGET,
            format!(
                "错误: K6 Plain 文件 {:?} staging 失败: {message}",
                entry.path
            ),
        )
    })?;
    Ok(MigrationStagedEntry {
        source_index: source.index,
        transform: MigrationTransform::PlainToEdp,
        path: target_path,
        is_directory: false,
        data,
        attributes: entry.attributes,
        mtime: entry.mtime.clone(),
        ctime: entry.ctime.clone(),
    })
}

pub(super) fn prepare_plain_to_official(
    dev: &mut dyn SectorDev,
    target_plan: &TargetProvisionPlan,
) -> EdpCliResult<PlainImportPlan> {
    let target_index = [
        PartitionRole::Share,
        PartitionRole::BootShareCombined,
        PartitionRole::Encrypt,
        PartitionRole::Boot,
    ]
    .into_iter()
    .find_map(|role| {
        target_plan
            .partitions
            .iter()
            .position(|part| part.geometry.role == role && part.geometry.filesystem.is_some())
    })
    .ok_or_else(|| err(EXIT_TARGET, "错误: K6 Plain→EDP 没有可接收文件的目标分区"))?;
    let target = &target_plan.partitions[target_index];
    let plain_parts = parse_plain_mbr(dev)?;
    let multiple = plain_parts.len() > 1;
    let mut sources = Vec::with_capacity(plain_parts.len());
    let mut staged = Vec::new();
    let mut total_logical_bytes = 0u64;
    let mut file_count = 0u64;
    let mut directory_count = 0u64;
    let mut paths = std::collections::BTreeSet::new();

    for plain in plain_parts {
        let (region, entries) = analyze_plain_partition(dev, plain, target.geometry.role)?;
        sources.push(MigrationSource {
            source_index: plain.index,
            region,
            transform: MigrationTransform::PlainToEdp,
        });
        let prefix = multiple.then(|| format!("/P{}/", plain.index + 1));
        if let Some(prefix) = &prefix {
            paths.insert(prefix.trim_end_matches('/').to_lowercase());
            staged.push(MigrationStagedEntry {
                source_index: plain.index,
                transform: MigrationTransform::PlainToEdp,
                path: prefix.clone(),
                is_directory: true,
                data: Vec::new(),
                attributes: 0x10,
                mtime: None,
                ctime: None,
            });
            directory_count += 1;
        }
        for entry in entries.iter().filter(|entry| entry.path != "/") {
            let target_path = match &prefix {
                Some(prefix) => format!("{}{}", prefix, entry.path.trim_start_matches('/')),
                None => entry.path.clone(),
            };
            let key = target_path.trim_end_matches('/').to_lowercase();
            if !paths.insert(key) {
                return Err(err(
                    EXIT_TARGET,
                    format!("错误: K6 Plain→EDP 目标路径冲突: {target_path:?}"),
                ));
            }
            let staged_entry = stage_plain_file(dev, plain, entry, target_path)?;
            if staged_entry.is_directory {
                directory_count += 1;
            } else {
                file_count += 1;
                total_logical_bytes = total_logical_bytes
                    .checked_add(staged_entry.data.len() as u64)
                    .ok_or_else(|| err(EXIT_TARGET, "错误: K6 Plain→EDP 文件总量溢出"))?;
            }
            staged.push(staged_entry);
        }
    }
    let target_bytes = target
        .geometry
        .sector_count
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| err(EXIT_TARGET, "错误: K6 Plain→EDP 目标容量溢出"))?;
    if total_logical_bytes > target_bytes {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: K6 Plain→EDP 文件总量 {total_logical_bytes} 超过目标容量 {target_bytes}"
            ),
        ));
    }
    Ok(PlainImportPlan {
        target_index,
        sources,
        prepared: PreparedMigrationTarget {
            target_index,
            role: target.geometry.role,
            entries: staged,
            total_logical_bytes,
            file_count,
            directory_count,
        },
    })
}

fn role_prefix(role: PartitionRole) -> &'static str {
    match role {
        PartitionRole::Boot => "EDP_BOOT",
        PartitionRole::Share => "EDP_SHARE",
        PartitionRole::Encrypt => "EDP_ENCRYPT",
        PartitionRole::BootShareCombined => "EDP_COMBINED",
        PartitionRole::CompatibilityReserve => "EDP_COMPAT",
    }
}

pub(super) fn prepare_existing_to_plain(
    dev: &mut dyn SectorDev,
    source: &ParsedExistingProvision,
    target_capacity_bytes: u64,
    key_domains: &KeyDomainSecrets,
) -> EdpCliResult<Vec<MigrationStagedEntry>> {
    let source_indices = source
        .profile
        .partitions
        .iter()
        .enumerate()
        .filter(|(_, part)| part.role != PartitionRole::CompatibilityReserve)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let multiple = source_indices.len() > 1;
    let mut staged = Vec::new();
    let mut total = 0u64;
    let mut paths = std::collections::BTreeSet::new();

    for source_index in source_indices {
        let partition = source.profile.partitions[source_index];
        let record = source.records[source_index];
        let region = SourceRegion::from_existing(partition, record);
        let inventory = inventory_source(dev, source, source_index, region, key_domains)?;
        let prefix = multiple.then(|| format!("/{}/", role_prefix(region.role)));
        if let Some(prefix) = &prefix {
            paths.insert(prefix.trim_end_matches('/').to_lowercase());
            staged.push(MigrationStagedEntry {
                source_index,
                transform: MigrationTransform::EdpToPlain,
                path: prefix.clone(),
                is_directory: true,
                data: Vec::new(),
                attributes: 0x10,
                mtime: None,
                ctime: None,
            });
        }
        for entry in inventory.entries.iter().filter(|entry| entry.path != "/") {
            let path = match &prefix {
                Some(prefix) => format!("{}{}", prefix, entry.path.trim_start_matches('/')),
                None => entry.path.clone(),
            };
            let key = path.trim_end_matches('/').to_lowercase();
            if !paths.insert(key) {
                return Err(err(
                    EXIT_TARGET,
                    format!("错误: K6 EDP→Plain 目标路径冲突: {path:?}"),
                ));
            }
            let manifest = MigrationManifestEntry {
                source_index,
                source_region: region,
                transform: MigrationTransform::EdpToPlain,
                path,
                is_directory: entry.is_directory,
                logical_size: entry.logical_size,
                attributes: entry.attributes,
                mtime: entry.mtime.clone(),
                ctime: entry.ctime.clone(),
                payload_locator: entry.payload_locator.clone(),
            };
            let staged_entry = stage_manifest_entry(dev, source, key_domains, &manifest)?;
            if !staged_entry.is_directory {
                total = total
                    .checked_add(staged_entry.data.len() as u64)
                    .ok_or_else(|| err(EXIT_TARGET, "错误: K6 EDP→Plain 文件总量溢出"))?;
            }
            staged.push(staged_entry);
        }
    }
    if total > target_capacity_bytes {
        return Err(err(
            EXIT_TARGET,
            format!("错误: K6 EDP→Plain 文件总量 {total} 超过目标容量 {target_capacity_bytes}"),
        ));
    }
    Ok(staged)
}

pub(super) fn prepare_migrations(
    dev: &mut dyn SectorDev,
    source: Option<&ParsedExistingProvision>,
    target_plan: &TargetProvisionPlan,
    key_domains: &KeyDomainSecrets,
) -> EdpCliResult<Vec<PreparedMigrationTarget>> {
    let migrate_targets = target_plan
        .partitions
        .iter()
        .enumerate()
        .filter(|(_, part)| part.disposition == RegionDisposition::Migrate)
        .collect::<Vec<_>>();
    if migrate_targets.is_empty() {
        return Ok(Vec::new());
    }
    let source = source.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: K6 Migrate 计划缺少来源注册结构，拒绝继续",
        )
    })?;

    let mut inventory_cache = std::collections::BTreeMap::<usize, MigrationInventory>::new();
    for (_, target) in &migrate_targets {
        for migration in &target.migration_sources {
            if !inventory_cache.contains_key(&migration.source_index) {
                let inventory = inventory_source(
                    dev,
                    source,
                    migration.source_index,
                    migration.region,
                    key_domains,
                )?;
                inventory_cache.insert(migration.source_index, inventory);
            }
        }
    }

    let mut prepared = Vec::with_capacity(migrate_targets.len());
    for (target_index, target) in migrate_targets {
        let inventories = target
            .migration_sources
            .iter()
            .map(|migration| {
                inventory_cache
                    .get(&migration.source_index)
                    .cloned()
                    .ok_or_else(|| {
                        err(
                            EXIT_TARGET,
                            format!("错误: K6 来源 {} inventory 缺失", migration.source_index),
                        )
                    })
            })
            .collect::<EdpCliResult<Vec<_>>>()?;
        let target_bytes = target
            .geometry
            .sector_count
            .checked_mul(SECTOR as u64)
            .ok_or_else(|| err(EXIT_TARGET, "错误: K6 目标分区容量溢出"))?;
        let staging_budget = target_bytes.min(usize::MAX as u64);
        let manifest = build_migration_manifest(
            &target.migration_sources,
            &target.geometry,
            &inventories,
            MigrationBudgets {
                staging_available_bytes: staging_budget,
                target_available_bytes: target_bytes,
            },
        )
        .map_err(|error| {
            err(
                EXIT_TARGET,
                format!(
                    "错误: {} K6 migration preflight 失败: {error}",
                    target.geometry.role.label()
                ),
            )
        })?;

        let mut staged = Vec::with_capacity(manifest.entries.len());
        for entry in &manifest.entries {
            staged.push(stage_manifest_entry(dev, source, key_domains, entry)?);
        }
        prepared.push(PreparedMigrationTarget {
            target_index,
            role: target.geometry.role,
            entries: staged,
            total_logical_bytes: manifest.total_logical_bytes,
            file_count: manifest.file_count,
            directory_count: manifest.directory_count,
        });
    }
    Ok(prepared)
}
