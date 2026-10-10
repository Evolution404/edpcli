use super::*;

impl AppState {
    pub fn provision_set_planning(&mut self) {
        self.provision.native_readonly_review = None;
        self.provision.native_geometry_review = None;
        self.provision.prepared = None;
        self.provision.review_projection = None;
        self.provision_transition_begin_planning();
    }

    /// Geometry-only draft for *any* observed source and user-selected target.
    /// Does not inspect passwords, verify a source EDPB, generate a protocol
    /// image, or create a media write intent. LCE proposed on Plain is labelled.
    #[cfg(test)]
    pub(crate) fn provision_native_geometry_readonly_plan(
        &self,
    ) -> Result<NativeGeometryReadOnlyReview, String> {
        let row = self.selected_device().ok_or("当前USB设备已不存在")?;
        let geometry = row.layout_geometry()?;
        if geometry.logical_sector_bytes == 512 {
            return Err("512B正式制盘应通过现有准备与验证流程".into());
        }
        let target = self.provision.kind;
        let (partitions, lce_lba) = if target == ProvisionKind::Plain {
            let plan = self.provision_plain_plan()?;
            (
                plan.partitions
                    .into_iter()
                    .enumerate()
                    .map(|(index, p)| {
                        (
                            format!("普通分区{}", index + 1),
                            p.start_lba,
                            p.sector_count,
                        )
                    })
                    .collect(),
                None,
            )
        } else {
            let (_, logical_bytes, lce_start) = self.provision_preview_geometry()?;
            if logical_bytes != geometry.logical_sector_bytes {
                return Err("设备原生几何在只读计划期间发生变化".into());
            }
            let (prefill, _) = self.provision_resolved_prefill()?;
            let parts = prefill.draft_partitions(u64::from(logical_bytes))?;
            crate::provision::validate_target_geometry(&parts, prefill.usable_end_lba)?;
            (
                parts
                    .into_iter()
                    .map(|p| (p.role.label().to_owned(), p.start_lba, p.sector_count))
                    .collect(),
                Some(lce_start),
            )
        };
        Ok(NativeGeometryReadOnlyReview {
            disk: row.disk,
            source_kind: row.provision_kind,
            target_kind: target,
            total_sectors: geometry.native_sector_count,
            sector_bytes: geometry.logical_sector_bytes,
            partitions,
            lce_lba,
            lce_is_source_verified: row.provision_kind
                != crate::provision::DiskProvisionKind::Plain
                && row.lce.is_some(),
        })
    }

    #[cfg(test)]
    pub(crate) fn provision_finish_native_geometry_readonly_plan(
        &mut self,
        result: Result<NativeGeometryReadOnlyReview, String>,
    ) {
        match result {
            Ok(review) => {
                if self.selected_device_disk() != Some(review.disk) {
                    self.provision_transition_plan_failed("只读草稿完成时目标盘已切换".into());
                    return;
                }
                self.provision.prepared = None;
                self.provision.review_projection = None;
                self.provision.native_readonly_review = None;
                self.provision.native_geometry_review = Some(review);
                self.provision_transition_plan_succeeded();
            }
            Err(err) => {
                self.provision.native_geometry_review = None;
                self.provision_transition_plan_failed(err);
            }
        }
    }

    /// 4Kn来源备份必须存在于备份工作区，并与当前选中硬件一致。
    /// 多份候选会导致歧义，拒绝自动选择；必须在备份治理中消除歧义。
    pub fn provision_unique_mode0_native_backup(&self) -> Result<std::path::PathBuf, String> {
        let selected = self.selected_device().ok_or("目标USB已不存在")?;
        let geometry = selected.layout_geometry()?;
        if geometry.logical_sector_bytes != 4096 {
            return Err("当前目标不是4096B原生扇区".into());
        }
        if self.provision.kind != ProvisionKind::Mode1 {
            return Err("目前仅支持从Mode0到Mode1的4Kn只读计划".into());
        }
        if selected.provision_kind != crate::provision::DiskProvisionKind::Mode0 {
            return Err("来源盘不是Mode0，拒绝重复转换；可继续只读查看现有布局".into());
        }
        let id = selected.device_id.as_deref().ok_or("USB来源设备ID缺失")?;
        let matches = self
            .backups
            .rows
            .iter()
            .filter(|backup| {
                backup.is_restorable()
                    && backup.path.extension().is_some_and(|ext| ext == "edpb")
                    && backup.provision_kind == Some(crate::provision::DiskProvisionKind::Mode0)
                    && backup.device_id.as_deref() == Some(id)
                    && backup.size_bytes == Some(selected.size)
                    && backup.vid.as_deref() == Some(selected.vid.as_str())
                    && backup.pid.as_deref() == Some(selected.pid.as_str())
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [only] => Ok(only.path.clone()),
            [] => Err("备份工作区没有唯一的已校验Mode0原生EDPB来源；请先创建并登记备份".into()),
            _ => Err("找到多份符合设备身份的Mode0备份，不能自动选择；请先消除来源歧义".into()),
        }
    }

    /// Native 4Kn summary is never promoted to PreparedProvision, and thus
    /// cannot enter confirmation, export, or the 512B physical writer.
    pub fn provision_finish_native_readonly_plan(
        &mut self,
        result: Result<
            crate::application::provision::native_preflight::Native4knReadOnlyPreflight,
            String,
        >,
    ) {
        match result {
            Ok(summary) => {
                if self.selected_device_disk() != Some(summary.disk) {
                    self.provision_transition_plan_failed("4Kn只读计划完成时目标磁盘已切换".into());
                    return;
                }
                self.provision.prepared = None;
                self.provision.review_projection = None;
                self.provision.native_readonly_review = Some(summary);
                self.provision.native_geometry_review = None;
                self.provision_transition_plan_succeeded();
            }
            Err(message) => {
                self.provision.native_readonly_review = None;
                self.provision_transition_plan_failed(message);
            }
        }
    }

    pub fn provision_finish_plan(
        &mut self,
        result: Result<crate::application::provision::PreparedProvision, String>,
    ) {
        match result {
            Ok(prepared) => {
                let projection = match ProvisionConfirmationViewModel::from_prepared(&prepared) {
                    Ok(projection) => projection,
                    Err(message) => {
                        self.provision_transition_plan_failed(format!(
                            "错误: 计划确认投影失败: {message}"
                        ));
                        return;
                    }
                };
                if let crate::application::provision::PreparedProvision::Official(official) =
                    &prepared
                {
                    if let Some(target_plan) = &official.target_plan {
                        for part in &target_plan.partitions {
                            match crate::provision::KeyDomainRole::from_partition_role(
                                part.geometry.role,
                            ) {
                                Some(crate::provision::KeyDomainRole::Share) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.share_source_knowledge = knowledge;
                                    }
                                }
                                Some(crate::provision::KeyDomainRole::Encrypt) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.encrypt_source_knowledge = knowledge;
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                }
                self.provision.review_projection = Some(projection);
                self.provision.prepared = Some(prepared);
                self.provision_transition_plan_succeeded();
            }
            Err(message) => {
                self.provision.review_projection = None;
                self.provision_transition_plan_failed(message);
            }
        }
    }
}
