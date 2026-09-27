use super::*;
use crate::backup_deep::{
    analyze_partition, stream_file_payload, AnalysisStatus, FileEntry, PartitionReader,
};
use crate::provision::{
    build_migration_manifest, finalize_staged_entry, MigrationBudgets, MigrationInventory,
    MigrationStagedEntry, SourceRegion,
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
            Some(key) => crate::backup_deep::keys::decrypt_mode2(&raw, &key)
                .map_err(std::io::Error::other),
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
                format!("错误: {} K6 来源 FileKey 验证失败: {message}", part.role.label()),
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
    if summary.logical_size != manifest.logical_size || bytes.len() as u64 != manifest.logical_size {
        return Err(err(
            EXIT_TARGET,
            format!("错误: K6 文件 {:?} staging 长度校验失败", manifest.path),
        ));
    }
    finalize_staged_entry(manifest, bytes)
        .map_err(|message| err(EXIT_TARGET, format!("错误: {message}")))
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
                            format!(
                                "错误: K6 来源 {} inventory 缺失",
                                migration.source_index
                            ),
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
