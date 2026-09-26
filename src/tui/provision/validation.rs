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
            apply_target_geometry_overrides, CapacityInput, CapacityInputMode, CapacitySource,
            QuickCapacityUnit, TargetGeometryOverrides,
        };

        let row = self
            .selected_device()
            .ok_or_else(|| "请先选择 USB 目标盘".to_string())?;
        let target_mode = self
            .provision_target_mode()
            .ok_or_else(|| "离线快照工具不使用物理制盘表单".to_string())?;
        let total_sectors = row.size / crate::common::SECTOR as u64;
        let lce = crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
            total_sectors,
            crate::common::SECTOR as u32,
        )
        .ok_or_else(|| "当前目标不符合已验证的 512B/255x63 USB LCE 几何".to_string())?;
        let source = row.existing_profile_for_prefill();
        let base = crate::provision::prefill_for_target_mode(
            source.as_ref(),
            target_mode,
            lce.start_lba,
            crate::common::SECTOR as u64,
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
                    ProvisionForm::resolve_quick_sectors(quick, exact, unit, edited, label)?,
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
                        "交换区",
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
                "交换区",
            )?,
            encrypt_start_lba: parse_start(
                matches!(mode, 0..=2),
                &form.encrypt_start_lba,
                base.encrypt_start_lba,
                "保密区",
            )?,
        };
        let resolved = apply_target_geometry_overrides(base, source.as_ref(), overrides)?;
        Ok((resolved, source))
    }

    pub(super) fn provision_selected_partition_role(
        &self,
    ) -> Option<crate::provision::PartitionRole> {
        use crate::provision::PartitionRole;
        let mode = self.provision.kind.mode()?;
        let slot = self.provision_field_slot(self.provision.field_selected)?;
        match slot {
            0 | 24 | 11 | 14 | 18 => Some(PartitionRole::Boot),
            1 | 25 | 12 | 15 | 19 => Some(if mode == 1 {
                PartitionRole::BootShareCombined
            } else {
                PartitionRole::Share
            }),
            2 | 26 | 13 | 16 | 20 => Some(PartitionRole::Encrypt),
            17 => Some(PartitionRole::CompatibilityReserve),
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
        let mut parts = resolved.target_partitions(crate::common::SECTOR as u64)?;
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
                crate::common::SECTOR as u64,
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
        let mode = self
            .provision
            .kind
            .mode()
            .ok_or_else(|| "免密改造不使用新盘表单".to_string())?;
        let parse_sectors = |value: &str, label: &str| -> Result<u64, String> {
            value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{label} 必须为正整数扇区"))
        };
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
                ProvisionForm::resolve_quick_sectors(quick, exact_value, unit, edited, label)?
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
            "交换区",
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
        if self.provision.form.label_id.trim().is_empty()
            || self.provision.form.user.trim().is_empty()
            || self.provision.form.dept.trim().is_empty()
            || self.provision.form.label.trim().is_empty()
            || self.provision.form.password.is_empty()
        {
            return Err("标签标识、用户、部门、标签和密码均不能为空".into());
        }
        let max_share_password_errors = self
            .provision
            .form
            .max_share_password_errors
            .trim()
            .parse::<u8>()
            .map_err(|_| "交换区密码最大错误次数必须为 0..255".to_string())?;
        let max_encrypt_password_errors = self
            .provision
            .form
            .max_encrypt_password_errors
            .trim()
            .parse::<u8>()
            .map_err(|_| "保密区密码最大错误次数必须为 0..255".to_string())?;
        Ok(crate::application::provision::OfficialProvisionRequest {
            target: self.provision.kind.target(),
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
            password: self.provision.form.password.clone(),
            volume_label: self.provision.form.volume_label.trim().to_string(),
            format: crate::application::provision::FormatOptions {
                boot: self.provision.form.format_boot,
                share: self.provision.form.format_share,
                encrypt: self.provision.form.format_encrypt,
                boot_label: self.provision.form.volume_label.trim().to_string(),
                share_label: self.provision.form.share_label.trim().to_string(),
                encrypt_label: self.provision.form.encrypt_label.trim().to_string(),
                boot_fs: self.provision.form.boot_fs,
                share_fs: self.provision.form.share_fs,
                encrypt_fs: self.provision.form.encrypt_fs,
            },
            force_change_password: Some(self.provision.form.force_change_password),
            cancel_password_complexity_check: Some(
                self.provision.form.cancel_password_complexity_check,
            ),
            max_share_password_errors: Some(max_share_password_errors),
            max_encrypt_password_errors: Some(max_encrypt_password_errors),
        })
    }
}
