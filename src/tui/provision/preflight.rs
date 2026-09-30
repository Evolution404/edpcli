use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvisionPreflightKind {
    Waiting,
    Passthrough,
    Rewrap,
    Preserve,
    Rebuild,
    BlockedNeedsFormat,
    BlockedNeedsTargetPassword,
    PendingBackend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionPartitionPreflight {
    pub(crate) role: crate::provision::PartitionRole,
    pub(crate) kind: ProvisionPreflightKind,
    pub(crate) reason: String,
    target_password_requested: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ProvisionPreflight {
    partitions: Vec<ProvisionPartitionPreflight>,
    geometry_error: Option<String>,
}

impl ProvisionPreflight {
    pub(crate) fn partition(
        &self,
        role: crate::provision::PartitionRole,
    ) -> Option<&ProvisionPartitionPreflight> {
        self.partitions.iter().find(|part| part.role == role)
    }

    pub(crate) fn validate_for_submit(&self) -> Result<(), String> {
        if let Some(message) = &self.geometry_error {
            return Err(message.clone());
        }
        for part in &self.partitions {
            match part.kind {
                ProvisionPreflightKind::Waiting => return Err(part.reason.clone()),
                ProvisionPreflightKind::BlockedNeedsFormat => {
                    return Err(format!(
                        "{}当前为“需重建”，但尚未获得用户的格式化授权；请主动勾选{}格式化后再继续",
                        part.role.label(),
                        part.role.label()
                    ));
                }
                ProvisionPreflightKind::BlockedNeedsTargetPassword => {
                    return Err(format!(
                        "{}已选择格式化，但原密码未验证；请按 i 设置新密码后再继续",
                        part.role.label()
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn target_password_requested(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> bool {
        self.partitions.iter().any(|part| {
            crate::provision::KeyDomainRole::from_partition_role(part.role) == Some(domain)
                && part.target_password_requested
        })
    }
}

impl AppState {
    pub(crate) fn provision_preflight(&self) -> Result<ProvisionPreflight, String> {
        let (resolved, source) = self.provision_resolved_prefill()?;
        let parts = resolved.draft_partitions(crate::common::SECTOR as u64)?;
        let geometry_error =
            crate::provision::validate_target_geometry(&parts, resolved.usable_end_lba).err();
        let partitions = parts
            .iter()
            .map(|part| self.provision_partition_preflight(source.as_ref(), part))
            .collect();
        Ok(ProvisionPreflight {
            partitions,
            geometry_error,
        })
    }

    fn provision_partition_preflight(
        &self,
        source: Option<&crate::provision::ExistingProvisionProfile>,
        target: &crate::provision::TargetPartitionGeometry,
    ) -> ProvisionPartitionPreflight {
        use crate::provision::{KeyDomainRole, PartitionRole};
        use password_verification::{PasswordIntent, SourcePasswordState};

        let role = target.role;
        let source_part = source.and_then(|profile| profile.partition(role));
        let assessment =
            crate::application::provision::PreserveAssessment::for_partition(source_part, target);
        let format_selected = match role {
            PartitionRole::Boot => self.provision.form.format_boot,
            PartitionRole::Share | PartitionRole::BootShareCombined => {
                self.provision.form.format_share
            }
            PartitionRole::Encrypt => self.provision.form.format_encrypt,
            PartitionRole::CompatibilityReserve => false,
        };

        if role == PartitionRole::CompatibilityReserve {
            return ProvisionPartitionPreflight {
                role,
                kind: ProvisionPreflightKind::Rebuild,
                reason: "兼容保留区按协议固定重建".into(),
                target_password_requested: false,
            };
        }

        if source_part.is_none() {
            if let Some(domain) = KeyDomainRole::from_partition_role(role) {
                let intent = self.provision_password_intent(domain, format_selected);
                let (kind, reason, target_password_requested) = match intent {
                    PasswordIntent::Waiting => (
                        ProvisionPreflightKind::Waiting,
                        format!("{}原密码正在只读验证，请稍候再生成计划", role.label()),
                        false,
                    ),
                    PasswordIntent::BlockedNeedsExplicitPassword => (
                        ProvisionPreflightKind::BlockedNeedsTargetPassword,
                        format!(
                            "{}已选择格式化，但原密码未验证且新密码仍为透传；请先设置新密码",
                            role.label()
                        ),
                        false,
                    ),
                    PasswordIntent::BlockedNeedsFormat => (
                        ProvisionPreflightKind::BlockedNeedsFormat,
                        "原密码未验证，无法无损改密；必须明确授权格式化重建".into(),
                        false,
                    ),
                    PasswordIntent::Rebuild => (
                        ProvisionPreflightKind::Rebuild,
                        "用户已明确选择格式化；目标密码域将重建并生成新密钥".into(),
                        true,
                    ),
                    PasswordIntent::Rewrap => (
                        ProvisionPreflightKind::PendingBackend,
                        "来源几何缓存不足以确认原位 Rewrap；后台计划将复核来源协议".into(),
                        true,
                    ),
                    PasswordIntent::Passthrough => (
                        ProvisionPreflightKind::PendingBackend,
                        "来源几何缓存不足以确认透传/迁移；后台计划将复核来源协议".into(),
                        false,
                    ),
                };
                return ProvisionPartitionPreflight {
                    role,
                    kind,
                    reason,
                    target_password_requested,
                };
            }
            return ProvisionPartitionPreflight {
                role,
                kind: if format_selected {
                    ProvisionPreflightKind::Rebuild
                } else {
                    ProvisionPreflightKind::PendingBackend
                },
                reason: if format_selected {
                    "用户已明确选择格式化；目标区域将重建".into()
                } else {
                    "来源无相同语义分区；迁移或重建动作需后台计划进一步确认".into()
                },
                target_password_requested: false,
            };
        }

        let Some(domain) = KeyDomainRole::from_partition_role(role) else {
            return ProvisionPartitionPreflight {
                role,
                kind: if format_selected {
                    ProvisionPreflightKind::Rebuild
                } else if assessment.candidate {
                    ProvisionPreflightKind::Preserve
                } else {
                    ProvisionPreflightKind::BlockedNeedsFormat
                },
                reason: if format_selected {
                    "用户已明确选择格式化；目标区域将重建".into()
                } else {
                    assessment.reason().into()
                },
                target_password_requested: false,
            };
        };

        if !assessment.candidate {
            let intent = self.provision_password_intent(domain, format_selected);
            let (kind, target_password_requested) = if format_selected {
                match intent {
                    PasswordIntent::Waiting => (ProvisionPreflightKind::Waiting, false),
                    PasswordIntent::BlockedNeedsExplicitPassword => {
                        (ProvisionPreflightKind::BlockedNeedsTargetPassword, false)
                    }
                    PasswordIntent::BlockedNeedsFormat => {
                        (ProvisionPreflightKind::BlockedNeedsFormat, false)
                    }
                    PasswordIntent::Rebuild | PasswordIntent::Rewrap => {
                        (ProvisionPreflightKind::Rebuild, true)
                    }
                    PasswordIntent::Passthrough => (ProvisionPreflightKind::Rebuild, false),
                }
            } else {
                (ProvisionPreflightKind::BlockedNeedsFormat, false)
            };
            return ProvisionPartitionPreflight {
                role,
                kind,
                reason: if format_selected {
                    "目标几何已改变；用户已明确授权格式化重建".into()
                } else {
                    format!(
                        "{}；原密码域/data extent 无法原样保留，必须明确授权格式化重建",
                        assessment.reason()
                    )
                },
                target_password_requested,
            };
        }

        let intent = self.provision_password_intent(domain, format_selected);
        let source_state = self.provision_source_password_state(domain);
        let opaque_profile = match domain {
            KeyDomainRole::Share => self.provision.form.share_opaque_profile,
            KeyDomainRole::Encrypt => self.provision.form.encrypt_opaque_profile,
        };
        let (kind, reason, target_password_requested) = match intent {
            PasswordIntent::Waiting => (
                ProvisionPreflightKind::Waiting,
                format!("{}原密码正在只读验证，请稍候再生成计划", role.label()),
                false,
            ),
            PasswordIntent::BlockedNeedsExplicitPassword => (
                ProvisionPreflightKind::BlockedNeedsTargetPassword,
                format!(
                    "{}已选择格式化，但原密码未验证且新密码仍为透传；请先设置新密码",
                    role.label()
                ),
                false,
            ),
            PasswordIntent::BlockedNeedsFormat => (
                ProvisionPreflightKind::BlockedNeedsFormat,
                "原密码未验证，无法无损改密；必须明确授权格式化重建".into(),
                false,
            ),
            PasswordIntent::Rebuild => (
                ProvisionPreflightKind::Rebuild,
                "用户已明确选择格式化；目标密码域将重建并生成新密钥".into(),
                true,
            ),
            PasswordIntent::Rewrap => (
                ProvisionPreflightKind::Rewrap,
                "来源 FileKey 已验证；仅 Rewrap 到新密码，data extent 保持不变".into(),
                true,
            ),
            PasswordIntent::Passthrough if source_state.is_verified() => (
                ProvisionPreflightKind::Passthrough,
                "来源密码与布局均已验证；原密码域、FileKey 与 data extent 透传".into(),
                false,
            ),
            PasswordIntent::Passthrough
                if matches!(
                    source_state,
                    SourcePasswordState::Failed | SourcePasswordState::Unknown
                ) && opaque_profile =>
            {
                (
                    ProvisionPreflightKind::Passthrough,
                    "来源密码未知但 key profile 与几何兼容；原 key material 与密文区域逐字节透传"
                        .into(),
                    false,
                )
            }
            PasswordIntent::Passthrough if source_state == SourcePasswordState::Unknown => (
                ProvisionPreflightKind::PendingBackend,
                "来源密码状态尚未定论；后台计划将复核 key profile 后决定透传或重建".into(),
                false,
            ),
            PasswordIntent::Passthrough => (
                ProvisionPreflightKind::BlockedNeedsFormat,
                "当前密码域不满足透传条件；必须明确授权格式化重建".into(),
                false,
            ),
        };
        ProvisionPartitionPreflight {
            role,
            kind,
            reason,
            target_password_requested,
        }
    }
}
