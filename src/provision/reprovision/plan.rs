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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetPartitionPlan {
    pub geometry: TargetPartitionGeometry,
    pub action: PartitionAction,
    pub disposition: RegionDisposition,
    pub password_disposition: Option<super::super::PasswordDisposition>,
    pub source_password_knowledge: Option<super::super::SourcePasswordKnowledge>,
    pub target_password_policy: Option<super::super::TargetPasswordPolicy>,
    pub reason: String,
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
        Self::build_with_plain_extents(source, &[], mode, targets, usable_end_lba, key_domains)
    }

    pub fn build_with_plain_extents(
        source: Option<&ParsedExistingProvision>,
        plain_source_extents: &[PlainSourceExtent],
        mode: OfficialPartitionMode,
        targets: &[TargetPartitionGeometry],
        usable_end_lba: u64,
        key_domains: &super::super::KeyDomainSecrets,
    ) -> Result<Self, String> {
        if targets.len() != mode.partition_types().len() {
            return Err("target partition count does not match official mode".into());
        }
        let gap = validate_target_geometry(targets, usable_end_lba)?;
        if let Some(source) = source {
            if source.profile.partitions.len() != source.records.len() {
                return Err("source partition/record count mismatch".into());
            }
        }
        let mut partitions = Vec::with_capacity(targets.len());
        for (index, target) in targets.iter().enumerate() {
            if target.partition_type != mode.partition_types()[index] {
                return Err(format!(
                    "target partition type at slot {index} does not match official mode"
                ));
            }
            let mut disposition = RegionDisposition::Rebuild;
            let mut password_disposition =
                super::super::KeyDomainRole::from_partition_role(target.role)
                    .map(|_| super::super::PasswordDisposition::Blocked);
            let mut source_password_knowledge = None;
            let mut target_password_policy =
                super::super::KeyDomainRole::from_partition_role(target.role)
                    .map(|_| super::super::TargetPasswordPolicy::InitializeNew);
            let mut reason = "无全兼容来源分区；目标区域必须重建".to_string();
            let mut preserved_record = None;
            if source.is_none() && plain_extent_preserve_candidate(plain_source_extents, target) {
                disposition = RegionDisposition::PreserveVerified;
                reason = "普通盘存在与目标 LBA 范围完全一致的物理分区；数据范围候选保留".into();
            }
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
                            password_disposition =
                                if key_domains.target_password(target.role).is_some() {
                                    Some(super::super::PasswordDisposition::Blocked)
                                } else {
                                    Some(super::super::PasswordDisposition::Passthrough(
                                        super::super::PassthroughBasis::OpaqueCompatible,
                                    ))
                                };
                            RegionDisposition::PreserveOpaque
                        } else if record.lba12.need_encrypt == 0 {
                            password_disposition = domain.map(|_| {
                                super::super::PasswordDisposition::Passthrough(
                                    super::super::PassthroughBasis::Verified,
                                )
                            });
                            RegionDisposition::PreserveVerified
                        } else {
                            match source_password_knowledge
                                .unwrap_or(super::super::SourcePasswordKnowledge::Unknown)
                            {
                                super::super::SourcePasswordKnowledge::Unknown => {
                                    target_password_policy =
                                        Some(super::super::TargetPasswordPolicy::PreserveOpaque);
                                    password_disposition =
                                        if key_domains.target_password(target.role).is_some() {
                                            Some(super::super::PasswordDisposition::Blocked)
                                        } else {
                                            Some(super::super::PasswordDisposition::Passthrough(
                                                super::super::PassthroughBasis::OpaqueCompatible,
                                            ))
                                        };
                                    RegionDisposition::PreserveOpaque
                                }
                                super::super::SourcePasswordKnowledge::DefaultVerified => {
                                    if key_domains.target_password(target.role).is_none()
                                        || key_domains.target_password(target.role)
                                            == Some(super::super::DEFAULT_KEY_DOMAIN_PASSWORD)
                                    {
                                        target_password_policy =
                                            Some(super::super::TargetPasswordPolicy::ReuseVerified);
                                        password_disposition =
                                            Some(super::super::PasswordDisposition::Passthrough(
                                                super::super::PassthroughBasis::Verified,
                                            ));
                                        RegionDisposition::PreserveVerified
                                    } else {
                                        target_password_policy = Some(
                                            super::super::TargetPasswordPolicy::ReplaceVerified,
                                        );
                                        password_disposition =
                                            Some(super::super::PasswordDisposition::Rewrap);
                                        RegionDisposition::RewrapVerified
                                    }
                                }
                                super::super::SourcePasswordKnowledge::UserVerified => {
                                    if key_domains.target_password(target.role).is_none()
                                        || key_domains.target_password(target.role) == user_password
                                    {
                                        target_password_policy =
                                            Some(super::super::TargetPasswordPolicy::ReuseVerified);
                                        password_disposition =
                                            Some(super::super::PasswordDisposition::Passthrough(
                                                super::super::PassthroughBasis::Verified,
                                            ));
                                        RegionDisposition::PreserveVerified
                                    } else {
                                        target_password_policy = Some(
                                            super::super::TargetPasswordPolicy::ReplaceVerified,
                                        );
                                        password_disposition =
                                            Some(super::super::PasswordDisposition::Rewrap);
                                        RegionDisposition::RewrapVerified
                                    }
                                }
                            }
                        };
                        if disposition.preserves_extent() {
                            reason = match disposition {
                                RegionDisposition::PreserveOpaque => {
                                    if password_disposition
                                        == Some(super::super::PasswordDisposition::Blocked)
                                    {
                                        "来源密码未知且请求了新密码；无法执行无损改密，必须由用户明确授权重建"
                                    } else {
                                        "分区角色、类型、LBA 范围、物理加密和密钥配置完全兼容；来源密码未知，文件系统不解读，原密钥材料与密文区域逐字节透传"
                                    }
                                }
                                RegionDisposition::PreserveVerified => {
                                    "物理结构、语义、分区几何、文件系统与来源密钥均已验证；原 FileKey 与数据范围保持不变"
                                }
                                RegionDisposition::RewrapVerified => {
                                    "来源 FileKey 已验证；目标密码变化，仅更新密码封装，数据范围保持零写入"
                                }
                                _ => unreachable!(),
                            }
                            .into();
                            preserved_record = Some(record);
                        }
                    } else {
                        reason =
                            "语义、位置、大小、物理加密、文件系统或密钥配置与来源不兼容；不能进入保留类处理"
                                .into();
                    }
                }
            }
            let action = disposition.legacy_action();
            partitions.push(TargetPartitionPlan {
                geometry: *target,
                action,
                disposition,
                password_disposition,
                source_password_knowledge,
                target_password_policy,
                reason,
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
        if part.disposition == RegionDisposition::Rebuild {
            part.password_disposition = super::super::KeyDomainRole::from_partition_role(role)
                .map(|_| super::super::PasswordDisposition::Rebuild);
            part.target_password_policy = super::super::KeyDomainRole::from_partition_role(role)
                .map(|_| super::super::TargetPasswordPolicy::InitializeNew);
            part.reason = "用户明确选择重新格式化；目标区域执行重建".into();
            return true;
        }
        if !part.disposition.preserves_extent() {
            return false;
        }
        part.action = PartitionAction::Rebuild;
        part.disposition = RegionDisposition::Rebuild;
        part.password_disposition = super::super::KeyDomainRole::from_partition_role(role)
            .map(|_| super::super::PasswordDisposition::Rebuild);
        part.target_password_policy = super::super::KeyDomainRole::from_partition_role(role)
            .map(|_| super::super::TargetPasswordPolicy::InitializeNew);
        part.preserved_record = None;
        part.reason = "用户选择重新格式化；保留类处理已明确转为重建".into();
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
