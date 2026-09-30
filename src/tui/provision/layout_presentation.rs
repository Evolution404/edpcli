use super::*;

impl AppState {
    pub fn provision_layout_editor_details(
        &self,
    ) -> Vec<crate::tui::disk_layout::DiskLayoutDetail> {
        use crate::tui::disk_layout::{
            DiskLayoutDetail as Detail, DiskLayoutDetailTone as Tone, DiskRegionKind, TailExpansion,
        };

        let Some(device) = self.selected_device() else {
            return vec![Detail::warning("未选择目标盘")];
        };
        let total = device.size / crate::common::SECTOR as u64;
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
            } else if let Ok(Some((role, ..))) = self.provision_selected_capacity_limit() {
                visible
                    .segments
                    .iter()
                    .position(|segment| segment.kind == DiskRegionKind::from_partition_role(role))
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
        let mut usable_summary = format!("整盘 {}", Self::format_sector_size(total));

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
                Self::format_sector_size(total),
                Self::format_sector_size(free)
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
            let Ok((resolved, source)) = self.provision_resolved_prefill() else {
                return vec![
                    Detail::muted(usable_summary),
                    Detail::danger("目标布局尚未通过校验"),
                ];
            };
            let Ok(mut parts) = resolved.target_partitions(crate::common::SECTOR as u64) else {
                return vec![
                    Detail::muted(usable_summary),
                    Detail::danger("目标分区几何无效"),
                ];
            };
            parts.sort_by_key(|part| part.start_lba);
            let unallocated =
                crate::provision::validate_target_geometry(&parts, resolved.usable_end_lba)
                    .unwrap_or_default();
            usable_summary = format!(
                "可分区 LBA {}–{} · 剩余 {}",
                crate::provision::OFFICIAL_PARTITION_START_SECTOR,
                resolved.usable_end_lba.saturating_sub(1),
                Self::format_sector_size(unallocated)
            );
            for part in &parts {
                let assessment = crate::application::provision::PreserveAssessment::for_partition(
                    source
                        .as_ref()
                        .and_then(|profile| profile.partition(part.role)),
                    part,
                );
                partition_status.insert(
                    (part.start_lba, part.sector_count),
                    (
                        if assessment.candidate {
                            "✓ 候选保留".into()
                        } else {
                            "⚠ 需重建".into()
                        },
                        if assessment.candidate {
                            Tone::Success
                        } else {
                            Tone::Warning
                        },
                        Some(part.role),
                        assessment.reason().to_string(),
                    ),
                );
            }
        }

        rows.push(Detail::muted(usable_summary));
        rows.push(Detail::accent(
            "区域              容量          LBA 范围               处理",
        ));

        for (index, segment) in visible.segments.iter().enumerate() {
            let (status, tone, _, _) = partition_status
                .get(&(segment.start_lba, segment.sector_count))
                .cloned()
                .unwrap_or_else(|| match segment.kind {
                    DiskRegionKind::Free => ("空闲".into(), Tone::Muted, None, String::new()),
                    DiskRegionKind::Unknown => {
                        ("待确认".into(), Tone::Warning, None, String::new())
                    }
                    DiskRegionKind::Plain => (
                        "⚠ 需重建".into(),
                        Tone::Warning,
                        None,
                        "目标普通分区将重建".into(),
                    ),
                    _ => ("固定".into(), Tone::Muted, None, String::new()),
                });
            rows.push(Detail::region_columns(
                segment.kind,
                index == selected,
                segment.label.clone(),
                Self::format_sector_size(segment.sector_count),
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
                    DiskRegionKind::Free => ("空闲".into(), Tone::Muted, None, String::new()),
                    DiskRegionKind::Unknown => {
                        ("待确认".into(), Tone::Warning, None, String::new())
                    }
                    _ => ("固定".into(), Tone::Muted, None, String::new()),
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
                Self::format_sector_size(segment.sector_count)
            )));

            if let (Some(role), Ok(Some((limit_role, current, max, ..)))) =
                (role, self.provision_selected_capacity_limit())
            {
                if role == limit_role {
                    rows.push(Detail::muted(format!(
                        "当前容量 {} · 最大 {}",
                        Self::format_sector_size(current),
                        Self::format_sector_size(max)
                    )));
                }
            }
        }

        rows.push(Detail::success("✓ 当前布局无重叠、未越界"));
        rows
    }
}
