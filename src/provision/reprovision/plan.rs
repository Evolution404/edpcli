use super::*;

impl ParsedExistingProvision {
    pub fn record_for_domain(
        &self,
        domain: super::super::KeyDomainRole,
    ) -> Option<&ExistingPartitionRecord> {
        self.profile
            .partitions
            .iter()
            .position(|part| {
                super::super::KeyDomainRole::from_partition_role(part.role) == Some(domain)
            })
            .map(|index| &self.records[index])
    }

    pub fn source_password_knowledge(
        &self,
        domain: super::super::KeyDomainRole,
        user_password: Option<&[u8]>,
    ) -> super::super::SourcePasswordKnowledge {
        let Some(record) = self.record_for_domain(domain) else {
            return super::super::SourcePasswordKnowledge::Unknown;
        };
        if record
            .verified_file_key(Some(super::super::DEFAULT_KEY_DOMAIN_PASSWORD))
            .is_ok()
        {
            return super::super::SourcePasswordKnowledge::DefaultVerified;
        }
        if user_password.is_some_and(|password| record.verified_file_key(Some(password)).is_ok()) {
            return super::super::SourcePasswordKnowledge::UserVerified;
        }
        super::super::SourcePasswordKnowledge::Unknown
    }

    pub fn record(&self, role: PartitionRole) -> Option<&ExistingPartitionRecord> {
        self.profile
            .partitions
            .iter()
            .position(|part| part.role == role)
            .map(|index| &self.records[index])
    }

    pub fn confirm_filesystem(
        &mut self,
        role: PartitionRole,
        format: FilesystemKind,
    ) -> Result<(), String> {
        let part = self
            .profile
            .partitions
            .iter_mut()
            .find(|part| part.role == role)
            .ok_or("source has no partition with requested role")?;
        part.filesystem = Some(format);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MigrationSource {
    pub source_index: usize,
    pub region: SourceRegion,
    pub transform: MigrationTransform,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetPartitionPlan {
    pub geometry: TargetPartitionGeometry,
    pub action: PartitionAction,
    pub disposition: RegionDisposition,
    pub source_password_knowledge: Option<super::super::SourcePasswordKnowledge>,
    pub target_password_policy: Option<super::super::TargetPasswordPolicy>,
    pub reason: String,
    /// Source regions that require file-level/data migration into this target.
    /// Execution remains fail-closed unless application preflight/staging completes.
    pub migration_sources: Vec<MigrationSource>,
    /// Only present when a verified source key record belongs to this exact
    /// target geometry. The writer re-encodes it for the target slot.
    pub preserved_record: Option<ExistingPartitionRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetProvisionPlan {
    pub mode: OfficialPartitionMode,
    pub partitions: Vec<TargetPartitionPlan>,
    pub unallocated_sectors: u64,
}

impl TargetProvisionPlan {
    pub fn build(
        source: Option<&ParsedExistingProvision>,
        mode: OfficialPartitionMode,
        targets: &[TargetPartitionGeometry],
        usable_end_lba: u64,
        key_domains: &super::super::KeyDomainSecrets,
    ) -> Result<Self, String> {
        if targets.len() != mode.partition_types().len() {
            return Err("target partition count does not match official mode".into());
        }
        let gap = validate_target_geometry(targets, usable_end_lba)?;
        let migration_plan = if let Some(source) = source {
            if source.profile.partitions.len() != source.records.len() {
                return Err("source partition/record count mismatch".into());
            }
            let source_regions = source
                .profile
                .partitions
                .iter()
                .copied()
                .zip(source.records.iter().copied())
                .map(|(partition, record)| SourceRegion::from_existing(partition, record))
                .collect::<Vec<_>>();
            let target_regions = targets
                .iter()
                .copied()
                .map(TargetRegion::from_target)
                .collect::<Vec<_>>();
            let plan = RegionMappingPlanner::map(&source_regions, &target_regions);
            Some((source_regions, plan))
        } else {
            None
        };
        let mut partitions = Vec::with_capacity(targets.len());
        for (index, target) in targets.iter().enumerate() {
            if target.partition_type != mode.partition_types()[index] {
                return Err(format!(
                    "target partition type at slot {index} does not match official mode"
                ));
            }
            let mut disposition = RegionDisposition::Rebuild;
            let mut source_password_knowledge = None;
            let mut target_password_policy =
                super::super::KeyDomainRole::from_partition_role(target.role)
                    .map(|_| super::super::TargetPasswordPolicy::InitializeNew);
            let mut reason = "无全兼容来源分区；目标区域必须重建".to_string();
            let mut preserved_record = None;
            let migration_sources = migration_plan
                .as_ref()
                .map(|(source_regions, plan)| {
                    plan.mappings
                        .iter()
                        .filter(|mapping| {
                            mapping.target_index == Some(index)
                                && mapping.kind == RegionMappingKind::Migrate
                        })
                        .filter_map(|mapping| {
                            let source_index = mapping.source_index?;
                            let transform = mapping.migration_transform?;
                            source_regions.get(source_index).copied().map(|region| {
                                MigrationSource {
                                    source_index,
                                    region,
                                    transform,
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if let Some(source) = source {
                if let Some((source_index, old)) = source
                    .profile
                    .partitions
                    .iter()
                    .enumerate()
                    .find(|(_, old)| old.role == target.role)
                {
                    let record = source.records[source_index];
                    let source_region = super::super::SourceRegion::from_existing(*old, record);
                    let target_region = super::super::TargetRegion::from_target(*target);
                    let domain = super::super::KeyDomainRole::from_partition_role(target.role);
                    let user_password = key_domains.source_password(target.role);
                    if record.lba12.need_encrypt != 0 {
                        if let Some(domain) = domain {
                            source_password_knowledge =
                                Some(source.source_password_knowledge(domain, user_password));
                        }
                    }

                    let strict_compatible =
                        super::super::preserve_compatibility(source_region, target_region).is_ok();
                    let opaque_compatible = record.lba12.need_encrypt != 0
                        && source_password_knowledge
                            == Some(super::super::SourcePasswordKnowledge::Unknown)
                        && super::super::opaque_preserve_compatibility(
                            source_region,
                            target_region,
                        )
                        .is_ok();

                    if strict_compatible || opaque_compatible {
                        disposition = if opaque_compatible && !strict_compatible {
                            target_password_policy =
                                Some(super::super::TargetPasswordPolicy::PreserveOpaque);
                            RegionDisposition::PreserveOpaque
                        } else if record.lba12.need_encrypt == 0 {
                            RegionDisposition::PreserveVerified
                        } else {
                            match source_password_knowledge
                                .unwrap_or(super::super::SourcePasswordKnowledge::Unknown)
                            {
                                super::super::SourcePasswordKnowledge::Unknown => {
                                    target_password_policy =
                                        Some(super::super::TargetPasswordPolicy::PreserveOpaque);
                                    RegionDisposition::PreserveOpaque
                                }
                                super::super::SourcePasswordKnowledge::DefaultVerified => {
                                    if key_domains.target_password(target.role)
                                        == Some(super::super::DEFAULT_KEY_DOMAIN_PASSWORD)
                                    {
                                        target_password_policy =
                                            Some(super::super::TargetPasswordPolicy::ReuseVerified);
                                        RegionDisposition::PreserveVerified
                                    } else {
                                        target_password_policy = Some(
                                            super::super::TargetPasswordPolicy::ReplaceVerified,
                                        );
                                        RegionDisposition::RewrapVerified
                                    }
                                }
                                super::super::SourcePasswordKnowledge::UserVerified => {
                                    if key_domains.target_password(target.role) == user_password {
                                        target_password_policy =
                                            Some(super::super::TargetPasswordPolicy::ReuseVerified);
                                        RegionDisposition::PreserveVerified
                                    } else {
                                        target_password_policy = Some(
                                            super::super::TargetPasswordPolicy::ReplaceVerified,
                                        );
                                        RegionDisposition::RewrapVerified
                                    }
                                }
                            }
                        };
                        if disposition.preserves_extent() {
                            reason = match disposition {
                                RegionDisposition::PreserveOpaque => {
                                    "role/type/extent/physical crypto/key profile 精确兼容；来源密码未知，文件系统不解读，原 key material 与密文区域逐字节透传"
                                }
                                RegionDisposition::PreserveVerified => {
                                    "物理/语义/几何/filesystem 与来源密钥均已验证；原 FileKey 与 data extent 保持不变"
                                }
                                RegionDisposition::RewrapVerified => {
                                    "来源 FileKey 已验证；目标密码变化，只允许重包 wrapper，data extent 保持零写入"
                                }
                                _ => unreachable!(),
                            }
                            .into();
                            preserved_record = Some(record);
                        }
                    } else {
                        reason =
                            "语义、位置、大小、物理加密、文件系统或 key profile 与来源不兼容；不能进入 Preserve family"
                                .into();
                    }
                }
            }
            if !migration_sources.is_empty() {
                let sources = migration_sources
                    .iter()
                    .map(|source| source.region.role.label())
                    .collect::<Vec<_>>()
                    .join(" + ");
                disposition = RegionDisposition::Migrate;
                source_password_knowledge = None;
                target_password_policy =
                    super::super::KeyDomainRole::from_partition_role(target.role)
                        .map(|_| super::super::TargetPasswordPolicy::InitializeNew);
                preserved_record = None;
                reason = format!(
                    "来源区域 {sources} 到目标 {} 使用 K6 文件级 staging/migration",
                    target.role.label()
                );
            }
            let action = disposition.legacy_action();
            partitions.push(TargetPartitionPlan {
                geometry: *target,
                action,
                disposition,
                source_password_knowledge,
                target_password_policy,
                reason,
                migration_sources,
                preserved_record,
            });
        }
        Ok(Self {
            mode,
            partitions,
            unallocated_sectors: gap,
        })
    }

    pub fn force_rebuild_for_format(&mut self, role: PartitionRole) -> bool {
        let Some(part) = self
            .partitions
            .iter_mut()
            .find(|part| part.geometry.role == role)
        else {
            return false;
        };
        if !part.disposition.preserves_extent() && part.disposition != RegionDisposition::Migrate {
            return false;
        }
        let previous = part.disposition;
        part.action = PartitionAction::Rebuild;
        part.disposition = RegionDisposition::Rebuild;
        part.target_password_policy = super::super::KeyDomainRole::from_partition_role(role)
            .map(|_| super::super::TargetPasswordPolicy::InitializeNew);
        part.preserved_record = None;
        part.migration_sources.clear();
        part.reason = if previous == RegionDisposition::Migrate {
            "用户选择重新格式化；Migrate 已显式转为 Rebuild".into()
        } else {
            "用户选择重新格式化；Preserve family 已显式转为 Rebuild".into()
        };
        true
    }

    pub fn preserved_extents(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.partitions
            .iter()
            .filter(|part| part.action == PartitionAction::PreserveExact)
            .map(|part| (part.geometry.start_lba, part.geometry.sector_count))
    }

    pub fn has_preserved_partitions(&self) -> bool {
        self.partitions
            .iter()
            .any(|part| part.action == PartitionAction::PreserveExact)
    }
}
