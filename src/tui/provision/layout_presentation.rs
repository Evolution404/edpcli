use super::*;

impl AppState {
    pub fn provision_layout_editor_details(
        &self,
    ) -> Vec<crate::tui::disk_layout::DiskLayoutDetail> {
        use crate::tui::disk_layout::{DiskLayoutDetail as Detail, DiskLayoutDetailTone as Tone};
        let Some(device) = self.selected_device() else {
            return vec![Detail::warning("未选择目标盘")];
        };
        let total = device.size / crate::common::SECTOR as u64;
        let mut rows = vec![
            Detail::muted(format!("设备      disk{}", device.disk)),
            Detail::muted(format!(
                "整盘      {}    {} sector",
                Self::format_sector_size(total),
                total
            )),
        ];
        if self.provision.kind == ProvisionKind::Plain {
            let Ok(plan) = self.provision.plain_form.plan(total) else {
                rows.push(Detail::danger("普通盘布局无效"));
                return rows;
            };
            rows.push(Detail::muted(format!(
                "已分配    {}",
                Self::format_sector_size(plan.partitions.iter().map(|p| p.sector_count).sum())
            )));
            rows.push(Detail::muted(format!(
                "空闲      {}",
                Self::format_sector_size(plan.gaps.iter().map(|g| g.sector_count).sum())
            )));
            rows.push(Detail::accent(
                "分区               LBA 范围                  容量          处理",
            ));
            for (index, part) in plan.partitions.iter().enumerate() {
                rows.push(Detail::partition_columns(
                    format!("P{}", index + 1),
                    format!(
                        "LBA {}–{}",
                        part.start_lba,
                        part.end_lba().unwrap_or(part.start_lba)
                    ),
                    Self::format_sector_size(part.sector_count),
                    "重建",
                    Tone::Warning,
                ));
            }
            rows.push(Detail::success("✓ 当前布局无重叠、未越界"));
            return rows;
        }

        let Ok((resolved, source)) = self.provision_resolved_prefill() else {
            rows.push(Detail::danger("目标布局尚未通过校验"));
            return rows;
        };
        let Ok(mut parts) = resolved.target_partitions(crate::common::SECTOR as u64) else {
            rows.push(Detail::danger("目标分区几何无效"));
            return rows;
        };
        parts.sort_by_key(|part| part.start_lba);
        let allocated = parts.iter().map(|part| part.sector_count).sum::<u64>();
        let unallocated =
            crate::provision::validate_target_geometry(&parts, resolved.usable_end_lba)
                .unwrap_or_default();
        rows.extend([
            Detail::muted(format!(
                "可分区    LBA {}–{}",
                crate::provision::OFFICIAL_PARTITION_START_SECTOR,
                resolved.usable_end_lba.saturating_sub(1)
            )),
            Detail::muted(format!("已分配    {}", Self::format_sector_size(allocated))),
            Detail::muted(format!(
                "未分配    {}",
                Self::format_sector_size(unallocated)
            )),
            Detail::accent("分区               LBA 范围                  容量          处理"),
        ]);
        for part in &parts {
            let assessment = crate::application::provision::PreserveAssessment::for_partition(
                source
                    .as_ref()
                    .and_then(|profile| profile.partition(part.role)),
                part,
            );
            rows.push(Detail::partition_columns(
                part.role.label(),
                format!(
                    "LBA {}–{}",
                    part.start_lba,
                    part.start_lba + part.sector_count - 1
                ),
                Self::format_sector_size(part.sector_count),
                if assessment.candidate {
                    "✓ 候选保留"
                } else {
                    "⚠ 需重建"
                },
                if assessment.candidate {
                    Tone::Success
                } else {
                    Tone::Warning
                },
            ));
        }
        if let Ok(Some((role, current, max, limiter, end))) =
            self.provision_selected_capacity_limit()
        {
            if let Some(part) = parts.iter().find(|part| part.role == role) {
                let assessment = crate::application::provision::PreserveAssessment::for_partition(
                    source.as_ref().and_then(|profile| profile.partition(role)),
                    part,
                );
                rows.push(Detail::accent(format!("当前分区  {}", role.label())));
                rows.push(Detail::muted(format!("原因      {}", assessment.reason())));
                rows.push(Detail::muted(format!(
                    "大小      {} ({} sector)",
                    Self::format_sector_size(current),
                    current
                )));
                rows.push(Detail::muted(format!(
                    "最大可设  {} ({} sector)",
                    Self::format_sector_size(max),
                    max
                )));
                rows.push(Detail::muted(format!(
                    "还能增加  {}",
                    Self::format_sector_size(max.saturating_sub(current))
                )));
                rows.push(Detail::muted(match limiter {
                    Some((next_role, next_start)) => format!(
                        "限制      后续{}固定起点 LBA {next_start}",
                        next_role.label()
                    ),
                    None => format!("限制      可分区末端 LBA {}", end.saturating_sub(1)),
                }));
            }
        }
        rows.push(Detail::success("✓ 当前布局无重叠、未越界"));
        rows
    }
}
