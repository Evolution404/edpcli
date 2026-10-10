use super::*;

impl AppState {
    pub(crate) fn provision_field_region_selection(
        &self,
        model: &crate::tui::disk_layout::DiskLayoutModel,
    ) -> Option<crate::tui::disk_layout::DiskCapacitySelection> {
        use crate::tui::disk_layout::{DiskCapacitySelection, DiskRegionKind};

        let focus = self
            .provision_field_id(self.provision.field_selected)
            .map(ProvisionFieldId::region_focus)
            .unwrap_or(ProvisionRegionFocus::None);
        let segment = model.segments.iter().find(|segment| match focus {
            ProvisionRegionFocus::None => false,
            ProvisionRegionFocus::Boot => segment.kind == DiskRegionKind::Boot,
            ProvisionRegionFocus::Share => {
                matches!(
                    segment.kind,
                    DiskRegionKind::Share | DiskRegionKind::Combined
                )
            }
            ProvisionRegionFocus::Encrypt => segment.kind == DiskRegionKind::Encrypt,
        })?;
        DiskCapacitySelection::from_segment(segment)
    }

    pub fn provision_layout_editor_details(
        &self,
    ) -> Vec<crate::tui::disk_layout::DiskLayoutDetail> {
        use crate::tui::disk_layout::{
            DiskLayoutDetail as Detail, DiskLayoutDetailTone as Tone, DiskRegionKind, TailExpansion,
        };

        let Some(device) = self.selected_device() else {
            return vec![Detail::warning("未选择目标盘")];
        };
        let total = device
            .layout_geometry()
            .map(|geometry| geometry.native_sector_count)
            .unwrap_or_default();
        let model = self.provision_layout_model();
        let visible = match self.disk_layout_tail_expansion() {
            TailExpansion::Collapsed => model.collapsed_tail_model(),
            TailExpansion::Expanded => model.clone(),
        };
        let layout_selected = self
            .disk_layout_selected()
            .min(visible.segments.len().saturating_sub(1));
        let selected =
            if self.provision_focused_pane() == crate::tui::pane::PaneId::ProvisionDiskLayout {
                layout_selected
            } else if let Some(selection) = self.provision_field_region_selection(&visible) {
                visible
                    .segments
                    .iter()
                    .position(|segment| {
                        segment.start_lba == selection.start_lba
                            && segment
                                .end_exclusive()
                                .is_ok_and(|end| end == selection.end_exclusive)
                    })
                    .unwrap_or(layout_selected)
            } else {
                layout_selected
            };

        let mut rows = Vec::new();
        let mut partition_status = std::collections::BTreeMap::<
            (u64, u64),
            (
                String,
                Tone,
                Option<crate::provision::PartitionRole>,
                String,
            ),
        >::new();
        let mut usable_summary = format!("整盘 {}", self.provision_display_capacity(total));
        if model.total_sectors == 0 && total != 0 {
            return vec![Detail::danger("当前来源几何未经认证，无法生成容量预览")];
        }

        if self.provision.kind == ProvisionKind::Plain {
            let Ok(plan) = self.provision.plain_form.plan(total) else {
                return vec![
                    Detail::muted(usable_summary),
                    Detail::danger("普通盘布局无效"),
                ];
            };
            let free = plan.gaps.iter().map(|gap| gap.sector_count).sum::<u64>();
            usable_summary = format!(
                "整盘 {} · 空闲 {}",
                self.provision_display_capacity(total),
                self.provision_display_capacity(free)
            );
            for part in &plan.partitions {
                partition_status.insert(
                    (part.start_lba, part.sector_count),
                    (
                        "⚠ 需重建".into(),
                        Tone::Warning,
                        None,
                        "目标普通分区将重建".into(),
                    ),
                );
            }
        } else {
            let Ok((resolved, _)) = self.provision_resolved_prefill() else {
                return vec![
                    Detail::muted(usable_summary),
                    Detail::danger("目标布局尚未通过校验"),
                ];
            };
            let Ok(mut parts) = resolved.draft_partitions(u64::from(resolved.logical_sector_bytes))
            else {
                return vec![
                    Detail::muted(usable_summary),
                    Detail::danger("目标分区几何无效"),
                ];
            };
            parts.sort_by_key(|part| part.start_lba);
            let geometry_validation =
                crate::provision::validate_target_geometry(&parts, resolved.usable_end_lba);
            let unallocated = geometry_validation.as_ref().copied().unwrap_or_default();
            usable_summary = format!(
                "可分区 LBA {}–{} · {}",
                crate::provision::OFFICIAL_PARTITION_START_SECTOR,
                resolved.usable_end_lba.saturating_sub(1),
                if geometry_validation.is_ok() {
                    format!("剩余 {}", self.provision_display_capacity(unallocated))
                } else {
                    "当前草稿有冲突".to_string()
                }
            );
            let preflight = self.provision_preflight();
            let preflight_error = preflight.as_ref().err().cloned();
            for part in &parts {
                let decision = preflight
                    .as_ref()
                    .ok()
                    .and_then(|preflight| preflight.partition(part.role));
                let (status, tone, reason) = match decision {
                    Some(decision) => {
                        use preflight::ProvisionPreflightKind as Kind;
                        let (status, tone) = match decision.kind {
                            Kind::Waiting => (
                                format!(
                                    "{} 验证中",
                                    crate::tui::animation::spinner_glyph(self.animation_frame())
                                ),
                                Tone::Accent,
                            ),
                            Kind::Passthrough => ("✓ 透传".into(), Tone::Success),
                            Kind::Rewrap => ("✓ 改密".into(), Tone::Success),
                            Kind::Preserve => ("✓ 候选保留".into(), Tone::Success),
                            Kind::Rebuild => {
                                let required = preflight
                                    .as_ref()
                                    .ok()
                                    .and_then(|value| value.format_disposition(part.role))
                                    == Some(preflight::ProvisionFormatDisposition::RequiredRebuild);
                                (
                                    if required {
                                        "⚠ 需重建"
                                    } else {
                                        "⚠ 重建"
                                    }
                                    .into(),
                                    Tone::Warning,
                                )
                            }
                            Kind::BlockedNeedsFormat | Kind::BlockedNeedsTargetPassword => {
                                ("⚠ 需重建".into(), Tone::Warning)
                            }
                        };
                        (status, tone, decision.reason.clone())
                    }
                    None => (
                        "⚠ 计划异常".into(),
                        Tone::Danger,
                        preflight_error.clone().unwrap_or_else(|| {
                            format!("内部错误：{}缺少同步预检结论", part.role.label())
                        }),
                    ),
                };
                partition_status.insert(
                    (part.start_lba, part.sector_count),
                    (status, tone, Some(part.role), reason),
                );
            }
        }

        rows.push(Detail::muted(usable_summary));
        if model.logical_sector_bytes != crate::common::SECTOR as u32 {
            rows.push(Detail::warning(format!(
                "{}B 原生布局 · 来源密码独立验证 · 正式写入将再次核验身份、几何及WAL",
                model.logical_sector_bytes
            )));
        }
        if let Some(note) = super::mode2_geometry_note::editor_note(self) {
            rows.push(Detail::accent(format!("说明  {note}")));
        }
        rows.push(Detail::region_header());

        for (index, segment) in visible.segments.iter().enumerate() {
            let role_for_kind = match segment.kind {
                DiskRegionKind::Boot => Some(crate::provision::PartitionRole::Boot),
                DiskRegionKind::Share => Some(crate::provision::PartitionRole::Share),
                DiskRegionKind::Combined => {
                    Some(crate::provision::PartitionRole::BootShareCombined)
                }
                DiskRegionKind::Encrypt => Some(crate::provision::PartitionRole::Encrypt),
                DiskRegionKind::Compatibility => {
                    Some(crate::provision::PartitionRole::CompatibilityReserve)
                }
                _ => None,
            };
            let fallback_role_status = || {
                role_for_kind.and_then(|role| {
                    partition_status
                        .values()
                        .find(|(_, _, candidate, _)| *candidate == Some(role))
                        .cloned()
                })
            };
            let (status, tone, _, _) = partition_status
                .get(&(segment.start_lba, segment.sector_count))
                .cloned()
                .or_else(fallback_role_status)
                .unwrap_or_else(|| match segment.kind {
                    DiskRegionKind::Conflict => (
                        "✗ 冲突".into(),
                        Tone::Danger,
                        None,
                        "当前草稿有多个区域覆盖同一 LBA 范围".into(),
                    ),
                    DiskRegionKind::Reserved => ("● 保留".into(), Tone::Muted, None, String::new()),
                    DiskRegionKind::Free => ("○ 空闲".into(), Tone::Muted, None, String::new()),
                    DiskRegionKind::Unknown => (
                        "⚠ 计划异常".into(),
                        Tone::Danger,
                        None,
                        "目标布局存在未分类区域".into(),
                    ),
                    DiskRegionKind::Plain => (
                        "⚠ 需重建".into(),
                        Tone::Warning,
                        None,
                        "目标普通分区将重建".into(),
                    ),
                    _ => ("● 固定".into(), Tone::Muted, None, String::new()),
                });
            rows.push(Detail::region_columns(
                segment.kind,
                index == selected,
                segment.label.clone(),
                self.provision_display_capacity(segment.sector_count),
                format!(
                    "LBA {}–{}",
                    segment.start_lba,
                    segment.end_exclusive().unwrap_or(segment.start_lba + 1) - 1
                ),
                status,
                tone,
            ));
        }

        if let Some(segment) = visible.segments.get(selected) {
            let (status, tone, role, _) = partition_status
                .get(&(segment.start_lba, segment.sector_count))
                .cloned()
                .unwrap_or_else(|| match segment.kind {
                    DiskRegionKind::Reserved => ("● 保留".into(), Tone::Muted, None, String::new()),
                    DiskRegionKind::Free => ("○ 空闲".into(), Tone::Muted, None, String::new()),
                    DiskRegionKind::Unknown => (
                        "⚠ 计划异常".into(),
                        Tone::Danger,
                        None,
                        "目标布局存在未分类区域".into(),
                    ),
                    _ => ("● 固定".into(), Tone::Muted, None, String::new()),
                });
            rows.push(Detail::muted(""));
            let title = format!("当前区域  {} · {}", segment.label, status);
            rows.push(match tone {
                Tone::Warning => Detail::warning(title),
                Tone::Danger => Detail::danger(title),
                Tone::Success => Detail::success(title),
                Tone::Accent => Detail::accent(title),
                Tone::Muted => Detail::accent(title),
            });
            rows.push(Detail::muted(format!(
                "LBA {}–{} · {}",
                segment.start_lba,
                segment.end_exclusive().unwrap_or(segment.start_lba + 1) - 1,
                self.provision_display_capacity(segment.sector_count)
            )));

            if let (Some(role), Ok(Some((limit_role, current, max, ..)))) =
                (role, self.provision_selected_capacity_limit())
            {
                if role == limit_role {
                    rows.push(Detail::muted(format!(
                        "当前容量 {} · 最大 {}",
                        self.provision_display_capacity(current),
                        self.provision_display_capacity(max)
                    )));
                }
            }
        }

        if self.provision.kind != ProvisionKind::Plain {
            if let Ok((resolved, _)) = self.provision_resolved_prefill() {
                if let Ok(parts) =
                    resolved.draft_partitions(u64::from(resolved.logical_sector_bytes))
                {
                    match crate::provision::validate_target_geometry(
                        &parts,
                        resolved.usable_end_lba,
                    ) {
                        Ok(_) => rows.push(Detail::success("✓ 当前布局无重叠、未越界")),
                        Err(message) => {
                            rows.push(Detail::danger(format!("✗ 当前草稿布局无效: {message}")))
                        }
                    }
                }
            }
        } else {
            rows.push(Detail::success("✓ 当前布局无重叠、未越界"));
        }
        rows
    }
}
