use super::*;

type ProvisionCapacityLimit = (
    crate::provision::PartitionRole,
    u64,
    u64,
    Option<(crate::provision::PartitionRole, u64)>,
    u64,
);

impl AppState {
    pub(super) fn provision_target_mode(&self) -> Option<crate::provision::OfficialPartitionMode> {
        self.provision.kind.target().official_mode()
    }

    /// Target layout projection uses the same native translated-CHS LCE
    /// locator as the official application plan. A source LCE is verified
    /// separately during source classification, even for destructive rebuild.
    /// Projection itself never grants a write lease or bypasses WAL/identity.
    pub(super) fn provision_preview_geometry(&self) -> Result<(u64, u32, u64), String> {
        let row = self.selected_device().ok_or("目标 USB 已不存在")?;
        let geometry = row.layout_geometry()?;
        let total = geometry.native_sector_count;
        // Destructive target layout uses the *same* translated CHS LCE
        // locator as the native application writer. Source LCE belongs to
        // input classification, not to the target's writable capacity.
        let lce_start = crate::application::provision_geometry::native_compatibility_extent(
            total,
            geometry.logical_sector_bytes,
        )?
        .start_lba;
        Ok((total, geometry.logical_sector_bytes, lce_start))
    }

    pub(super) fn provision_initial_prefill(
        &self,
        kind: ProvisionKind,
    ) -> Option<crate::provision::ProvisionPrefill> {
        let target_mode = kind.target().official_mode()?;
        let row = self.selected_device()?;
        let source = row.existing_profile_for_prefill();
        let (_, logical_bytes, lce_start) = self.provision_preview_geometry().ok()?;
        crate::provision::prefill_for_target_mode(
            source.as_ref(),
            target_mode,
            lce_start,
            u64::from(logical_bytes),
        )
        .ok()
    }

    pub(super) fn provision_resolved_prefill(
        &self,
    ) -> Result<
        (
            crate::provision::ProvisionPrefill,
            Option<crate::provision::ExistingProvisionProfile>,
        ),
        String,
    > {
        use crate::provision::{
            apply_target_geometry_overrides_draft, CapacityInput, CapacityInputMode,
            CapacitySource, QuickCapacityUnit, TargetGeometryOverrides,
        };

        let row = self
            .selected_device()
            .ok_or_else(|| "请先选择 USB 目标盘".to_string())?;
        let target_mode = self
            .provision_target_mode()
            .ok_or_else(|| "离线快照工具不使用物理制盘表单".to_string())?;
        let (_, logical_bytes, lce_start) = self.provision_preview_geometry()?;
        let source = row.existing_profile_for_prefill();
        // All target modes may be evaluated in memory. A valid draft is NOT
        // physical write authorization, which stays behind the commit gate.
        let base = crate::provision::prefill_for_target_mode(
            source.as_ref(),
            target_mode,
            lce_start,
            u64::from(logical_bytes),
        )?;
        let form = &self.provision.form;
        let capacity = |mode: CapacityInputMode,
                        unit: QuickCapacityUnit,
                        quick: &str,
                        exact: &str,
                        edited: bool,
                        original: Option<CapacityInput>,
                        label: &str|
         -> Result<CapacityInput, String> {
            let provisional = match mode {
                CapacityInputMode::Exact => CapacityInput::from_exact(
                    exact
                        .parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| format!("{label}必须为正整数 sector"))?,
                    CapacitySource::UserEdited,
                )?,
                CapacityInputMode::Quick => CapacityInput::from_quick_sectors(
                    ProvisionForm::resolve_quick_sectors_native(
                        quick,
                        exact,
                        unit,
                        edited,
                        label,
                        logical_bytes,
                    )?,
                    CapacitySource::UserEdited,
                )?,
            };
            let source = original
                .filter(|value| value.sectors() == provisional.sectors())
                .map(CapacityInput::source)
                .unwrap_or(CapacitySource::UserEdited);
            match mode {
                CapacityInputMode::Exact => {
                    CapacityInput::from_exact(provisional.sectors(), source)
                }
                CapacityInputMode::Quick => match unit {
                    QuickCapacityUnit::MiB => {
                        CapacityInput::from_quick_sectors(provisional.sectors(), source)
                    }
                    QuickCapacityUnit::GiB => {
                        CapacityInput::from_quick_sectors(provisional.sectors(), source)
                    }
                },
            }
        };
        let parse_start = |active: bool,
                           value: &str,
                           original: Option<u64>,
                           label: &str|
         -> Result<Option<u64>, String> {
            if !active {
                return Ok(None);
            }
            let parsed = value
                .parse::<u64>()
                .map_err(|_| format!("{label}起点必须为整数 LBA"))?;
            Ok((Some(parsed) != original).then_some(parsed))
        };

        let mode = self.provision.kind.mode().unwrap_or(0);
        let share_region = if mode == 1 {
            "二合一区"
        } else {
            "交换区"
        };
        let overrides = TargetGeometryOverrides {
            boot: matches!(mode, 0 | 3)
                .then(|| {
                    capacity(
                        form.boot_input_mode,
                        form.boot_quick_unit,
                        &form.boot_mib,
                        &form.boot_sectors,
                        form.boot_capacity_edited,
                        base.boot,
                        "启动区",
                    )
                })
                .transpose()?,
            share: matches!(mode, 0 | 1 | 3)
                .then(|| {
                    capacity(
                        form.share_input_mode,
                        form.share_quick_unit,
                        &form.share_mib,
                        &form.share_sectors,
                        form.share_capacity_edited,
                        base.share,
                        share_region,
                    )
                })
                .transpose()?,
            encrypt: matches!(mode, 0..=2)
                .then(|| {
                    capacity(
                        form.encrypt_input_mode,
                        form.encrypt_quick_unit,
                        &form.encrypt_mib,
                        &form.encrypt_sectors,
                        form.encrypt_capacity_edited,
                        base.encrypt,
                        "保密区",
                    )
                })
                .transpose()?,
            boot_start_lba: parse_start(
                matches!(mode, 0 | 3),
                &form.boot_start_lba,
                base.boot_start_lba,
                "启动区",
            )?,
            share_start_lba: parse_start(
                matches!(mode, 0 | 1 | 3),
                &form.share_start_lba,
                base.share_start_lba,
                share_region,
            )?,
            encrypt_start_lba: parse_start(
                matches!(mode, 0..=2),
                &form.encrypt_start_lba,
                base.encrypt_start_lba,
                "保密区",
            )?,
        };
        let resolved = apply_target_geometry_overrides_draft(base, source.as_ref(), overrides)?;
        Ok((resolved, source))
    }

    pub(super) fn provision_selected_partition_role(
        &self,
    ) -> Option<crate::provision::PartitionRole> {
        match self.provision_field_id(self.provision.field_selected)? {
            ProvisionFieldId::Capacity(role)
            | ProvisionFieldId::StartLba(role)
            | ProvisionFieldId::FormatEnabled(role)
            | ProvisionFieldId::Filesystem(role)
            | ProvisionFieldId::VolumeLabel(role) => Some(role),
            _ => None,
        }
    }

    pub(super) fn provision_selected_capacity_limit(
        &self,
    ) -> Result<Option<ProvisionCapacityLimit>, String> {
        use crate::provision::PartitionRole;

        let Some(role) = self.provision_selected_partition_role() else {
            return Ok(None);
        };
        let (resolved, source) = self.provision_resolved_prefill()?;
        let mut parts = resolved.target_partitions(u64::from(resolved.logical_sector_bytes))?;
        parts.sort_by_key(|part| part.start_lba);
        let Some((index, current)) = parts.iter().enumerate().find(|(_, part)| part.role == role)
        else {
            return Ok(None);
        };
        let base = self.provision_target_mode().and_then(|mode| {
            crate::provision::prefill_for_target_mode(
                source.as_ref(),
                mode,
                resolved.usable_end_lba,
                u64::from(resolved.logical_sector_bytes),
            )
            .ok()
        });
        let explicit_start = |candidate: PartitionRole| -> bool {
            let Some(base) = base.as_ref() else {
                return false;
            };
            let (text, original) = match candidate {
                PartitionRole::Boot | PartitionRole::CompatibilityReserve => (
                    self.provision.form.boot_start_lba.as_str(),
                    base.boot_start_lba,
                ),
                PartitionRole::Share | PartitionRole::BootShareCombined => (
                    self.provision.form.share_start_lba.as_str(),
                    base.share_start_lba,
                ),
                PartitionRole::Encrypt => (
                    self.provision.form.encrypt_start_lba.as_str(),
                    base.encrypt_start_lba,
                ),
            };
            text.parse::<u64>()
                .ok()
                .is_some_and(|start| Some(start) != original)
        };
        let anchored = |candidate: PartitionRole| {
            source
                .as_ref()
                .and_then(|profile| profile.partition(candidate))
                .is_some()
                || explicit_start(candidate)
        };

        let mut boundary = resolved.usable_end_lba;
        let mut downstream_unanchored = 0u64;
        let mut limiter = None;
        for next in parts.iter().skip(index + 1) {
            if anchored(next.role) {
                boundary = next.start_lba;
                limiter = Some((next.role, next.start_lba));
                break;
            }
            downstream_unanchored = downstream_unanchored.saturating_add(next.sector_count);
        }
        let max_sectors = boundary
            .saturating_sub(current.start_lba)
            .saturating_sub(downstream_unanchored);
        Ok(Some((
            current.role,
            current.sector_count,
            max_sectors,
            limiter,
            resolved.usable_end_lba,
        )))
    }

    pub fn provision_request(
        &mut self,
    ) -> Result<crate::application::provision::OfficialProvisionRequest, String> {
        self.provision
            .form
            .encryption_algorithm
            .validate_first_party_write()?;
        // The native CLI and TUI share the exact same application prepare/
        // commit service. Native block size is a validated geometry input,
        // not a UI-only write permission switch.
        self.selected_device()
            .ok_or("目标设备已不存在")?
            .layout_geometry()?;
        let mode = self
            .provision
            .kind
            .mode()
            .ok_or_else(|| "当前流程不使用新盘表单".to_string())?;
        let share_region = if mode == 1 {
            "二合一区"
        } else {
            "交换区"
        };
        let parse_sectors = |value: &str, label: &str| -> Result<u64, String> {
            value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{label} 必须为正整数扇区"))
        };
        let preflight = self.provision_preflight()?;
        preflight.validate_for_submit()?;
        let format_boot = preflight.format_selected(crate::provision::PartitionRole::Boot);
        let format_share = preflight.format_selected(crate::provision::PartitionRole::Share)
            || preflight.format_selected(crate::provision::PartitionRole::BootShareCombined);
        let format_encrypt = preflight.format_selected(crate::provision::PartitionRole::Encrypt);
        let share_target_requested =
            preflight.target_password_requested(crate::provision::KeyDomainRole::Share);
        let encrypt_target_requested =
            preflight.target_password_requested(crate::provision::KeyDomainRole::Encrypt);
        let share_target = share_target_requested.then(|| {
            if self.provision_target_password_mode(crate::provision::KeyDomainRole::Share)
                == password_verification::TargetPasswordMode::Explicit
            {
                self.provision.form.share_target_password.as_str()
            } else {
                self.provision.form.share_source_password.as_str()
            }
        });
        let encrypt_target = encrypt_target_requested.then(|| {
            if self.provision_target_password_mode(crate::provision::KeyDomainRole::Encrypt)
                == password_verification::TargetPasswordMode::Explicit
            {
                self.provision.form.encrypt_target_password.as_str()
            } else {
                self.provision.form.encrypt_source_password.as_str()
            }
        });
        let form = &self.provision.form;
        let exact = crate::provision::CapacityInputMode::Exact;
        let capacity_sectors = |active: bool,
                                mode: crate::provision::CapacityInputMode,
                                unit: crate::provision::QuickCapacityUnit,
                                quick: &str,
                                exact_value: &str,
                                edited: bool,
                                label: &str|
         -> Result<Option<u64>, String> {
            if !active {
                return Ok(None);
            }
            let sectors = if mode == exact {
                parse_sectors(exact_value, label)?
            } else {
                ProvisionForm::resolve_quick_sectors_native(
                    quick,
                    exact_value,
                    unit,
                    edited,
                    label,
                    form.logical_sector_bytes,
                )?
            };
            Ok(Some(sectors))
        };
        let boot_sectors = capacity_sectors(
            matches!(mode, 0 | 3),
            form.boot_input_mode,
            form.boot_quick_unit,
            &form.boot_mib,
            &form.boot_sectors,
            form.boot_capacity_edited,
            "启动区",
        )?;
        let share_sectors = capacity_sectors(
            matches!(mode, 0 | 1 | 3),
            form.share_input_mode,
            form.share_quick_unit,
            &form.share_mib,
            &form.share_sectors,
            form.share_capacity_edited,
            share_region,
        )?;
        let encrypt_sectors = capacity_sectors(
            matches!(mode, 0..=2),
            form.encrypt_input_mode,
            form.encrypt_quick_unit,
            &form.encrypt_mib,
            &form.encrypt_sectors,
            form.encrypt_capacity_edited,
            "保密区",
        )?;
        let boot_mib = None;
        let share_mib = None;
        let encrypt_mib = None;
        let (resolved, _) = self.provision_resolved_prefill()?;
        resolved.target_partitions(u64::from(resolved.logical_sector_bytes))?;
        if self.provision.form.label_id.trim().is_empty()
            || self.provision.form.user.trim().is_empty()
            || self.provision.form.dept.trim().is_empty()
            || self.provision.form.label.trim().is_empty()
        {
            return Err("标签标识、用户、部门和标签均不能为空".into());
        }
        if matches!(mode, 0 | 1 | 3) && share_target.is_some_and(str::is_empty) {
            return Err(format!("{share_region}目标密码不能为空"));
        }
        if matches!(mode, 0..=2) && encrypt_target.is_some_and(str::is_empty) {
            return Err("保密密钥域目标密码不能为空".into());
        }
        let password_error_limit = |domain, value: &str, region| -> Result<Option<u8>, String> {
            if !self.provision.kind.disk_kind().has_key_domain(domain) {
                return Ok(None);
            }
            value
                .trim()
                .parse::<u8>()
                .map(Some)
                .map_err(|_| format!("{region}密码最大错误次数必须为 0..255"))
        };
        let max_share_password_errors = password_error_limit(
            crate::provision::KeyDomainRole::Share,
            &self.provision.form.max_share_password_errors,
            share_region,
        )?;
        let max_encrypt_password_errors = password_error_limit(
            crate::provision::KeyDomainRole::Encrypt,
            &self.provision.form.max_encrypt_password_errors,
            "保密区",
        )?;
        Ok(crate::application::provision::OfficialProvisionRequest {
            target: self.provision.kind.target(),
            algorithm: self.provision.form.encryption_algorithm,
            boot_start_lba: matches!(mode, 0 | 3)
                .then_some(resolved.boot_start_lba)
                .flatten(),
            share_start_lba: matches!(mode, 0 | 1 | 3)
                .then_some(resolved.share_start_lba)
                .flatten(),
            encrypt_start_lba: matches!(mode, 0..=2)
                .then_some(resolved.encrypt_start_lba)
                .flatten(),
            boot_mib,
            boot_sectors,
            share_mib,
            share_sectors,
            encrypt_mib,
            encrypt_sectors,
            label_id: self.provision.form.label_id.trim().to_string(),
            user: self.provision.form.user.trim().to_string(),
            dept: self.provision.form.dept.trim().to_string(),
            label: self.provision.form.label.trim().to_string(),
            lba8_identity: self.provision.form.lba8_identity(),
            key_domains: crate::provision::KeyDomainSecrets::new(
                crate::provision::KeyDomainSecretPair::new(
                    (!self.provision.form.share_source_password.is_empty())
                        .then_some(self.provision.form.share_source_password.as_bytes()),
                    share_target.map(str::as_bytes),
                ),
                crate::provision::KeyDomainSecretPair::new(
                    (!self.provision.form.encrypt_source_password.is_empty())
                        .then_some(self.provision.form.encrypt_source_password.as_bytes()),
                    encrypt_target.map(str::as_bytes),
                ),
            ),
            volume_label: self.provision.form.volume_label.trim().to_string(),
            format: crate::application::provision::FormatOptions {
                boot: format_boot,
                share: format_share,
                encrypt: format_encrypt,
                boot_label: self.provision.form.volume_label.trim().to_string(),
                share_label: self.provision.form.share_label.trim().to_string(),
                encrypt_label: self.provision.form.encrypt_label.trim().to_string(),
                boot_fs: if let (Some(boot_start), Some(boot)) =
                    (resolved.boot_start_lba, resolved.boot)
                {
                    form.effective_boot_filesystem(boot_start, boot.sectors())?
                } else {
                    form.boot_fs
                },
                share_fs: self.provision.form.share_fs,
                encrypt_fs: self.provision.form.encrypt_fs,
            },
            preserve_unformatted: true,
            force_change_password: Some(self.provision.form.force_change_password),
            cancel_password_complexity_check: Some(
                self.provision.form.cancel_password_complexity_check,
            ),
            max_share_password_errors,
            max_encrypt_password_errors,
        })
    }
}
