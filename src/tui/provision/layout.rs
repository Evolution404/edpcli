use super::*;

impl AppState {
    pub fn provision_geometry_preview_lines(&self) -> Vec<String> {
        use crate::provision::PartitionRole;
        let (resolved, source) = match self.provision_resolved_prefill() {
            Ok(value) => value,
            Err(message) => return vec![format!("布局无效: {message}")],
        };
        let parts = match resolved.target_partitions(crate::common::SECTOR as u64) {
            Ok(parts) => parts,
            Err(message) => return vec![format!("布局无效: {message}")],
        };
        let mut lines = Vec::new();
        let mut cursor = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
        for part in &parts {
            if part.start_lba > cursor {
                lines.push(format!("空隙  {} sector", part.start_lba - cursor));
            }
            let end = part.start_lba + part.sector_count - 1;
            let candidate = source
                .as_ref()
                .and_then(|profile| profile.partition(part.role))
                .is_some_and(|old| {
                    part.role != PartitionRole::CompatibilityReserve
                        && old.partition_type == part.partition_type
                        && old.start_lba == part.start_lba
                        && old.sector_count == part.sector_count
                        && old.physically_encrypted == part.physically_encrypted
                });
            let action = if candidate {
                "可保留（待校验）"
            } else {
                "重建"
            };
            lines.push(format!(
                "{}  {} sector (~{} MiB)  ·  LBA {}–{}  ·  {}",
                part.role.label(),
                part.sector_count,
                part.sector_count / 2048,
                part.start_lba,
                end,
                action
            ));
            cursor = part.start_lba + part.sector_count;
        }
        let unallocated =
            crate::provision::validate_target_geometry(&parts, resolved.usable_end_lba)
                .unwrap_or_default();
        lines.push(format!("未分配  {unallocated} sector"));
        lines
    }

    fn format_sector_size(sectors: u64) -> String {
        let mib = sectors as f64 / 2048.0;
        if mib >= 1024.0 {
            format!("{:.2} GiB", mib / 1024.0)
        } else if mib >= 1.0 {
            format!("{mib:.1} MiB")
        } else {
            format!("{sectors} sector")
        }
    }

    fn provision_plain_layout_editor_lines(&self) -> Vec<String> {
        let Some(row) = self.selected_device() else {
            return vec!["未选择目标盘".into()];
        };
        let total_sectors = row.size / crate::common::SECTOR as u64;
        let mut lines = vec![format!(
            "disk{}  整盘 {}  ·  {} sector",
            row.disk,
            Self::format_sector_size(total_sectors),
            total_sectors
        )];
        let plan = match self.provision.plain_form.plan(total_sectors) {
            Ok(plan) => plan,
            Err(message) => {
                lines.push(format!("✗ 布局无效: {message}"));
                lines.push("Insert 添加分区 · Delete 删除当前分区".into());
                return lines;
            }
        };

        let allocated = plan
            .partitions
            .iter()
            .map(|part| part.sector_count)
            .sum::<u64>();
        let unallocated = plan.gaps.iter().map(|gap| gap.sector_count).sum::<u64>();
        lines.push(format!(
            "已分配 {}  ·  空闲 {}",
            Self::format_sector_size(allocated),
            Self::format_sector_size(unallocated)
        ));
        lines.push(String::new());

        let mut entries = Vec::<(u64, String)>::new();
        for gap in &plan.gaps {
            entries.push((
                gap.start_lba,
                format!(
                    "空闲  LBA {}–{}  ·  {}",
                    gap.start_lba,
                    gap.end_lba(),
                    Self::format_sector_size(gap.sector_count)
                ),
            ));
        }
        for (index, part) in plan.partitions.iter().enumerate() {
            entries.push((
                part.start_lba,
                format!(
                    "P{}  LBA {}–{}  ·  {}  ·  {}",
                    index + 1,
                    part.start_lba,
                    part.end_lba().unwrap_or(part.start_lba),
                    Self::format_sector_size(part.sector_count),
                    part.filesystem.windows_format_name()
                ),
            ));
        }
        entries.sort_by_key(|(start, _)| *start);
        lines.extend(entries.into_iter().map(|(_, text)| text));

        lines.push(String::new());
        if let Some(ProvisionFieldId::Plain { partition, .. }) =
            self.provision_field_id(self.provision.field_selected)
        {
            if let Some(part) = plan.partitions.get(partition) {
                if let Ok(max_sectors) = plan.max_sector_count(partition) {
                    lines.push(format!("当前: P{}", partition + 1));
                    lines.push(format!(
                        "大小 {} ({} sector)",
                        Self::format_sector_size(part.sector_count),
                        part.sector_count
                    ));
                    lines.push(format!(
                        "最大可设 {} ({} sector)",
                        Self::format_sector_size(max_sectors),
                        max_sectors
                    ));
                    if let Some(next) = plan
                        .partitions
                        .iter()
                        .filter(|candidate| candidate.start_lba > part.start_lba)
                        .min_by_key(|candidate| candidate.start_lba)
                    {
                        lines.push(format!("限制: 下一分区固定起点 LBA {}", next.start_lba));
                    } else {
                        lines.push(format!(
                            "限制: 磁盘末端 LBA {}",
                            total_sectors.saturating_sub(1)
                        ));
                    }
                }
            }
        }
        lines.push("✓ 当前布局无重叠、未越界；编辑一个分区不会移动其它分区".into());
        lines.push("Insert 添加分区 · Delete 删除当前分区".into());
        lines
    }

    pub fn provision_layout_editor_lines(&self) -> Vec<String> {
        use crate::provision::PartitionRole;

        if self.provision.kind == ProvisionKind::Plain {
            return self.provision_plain_layout_editor_lines();
        }

        let Some(row) = self.selected_device() else {
            return vec!["未选择目标盘".into()];
        };
        let total_sectors = row.size / crate::common::SECTOR as u64;
        let mut lines = vec![format!(
            "disk{}  整盘 {}  ·  {} sector",
            row.disk,
            Self::format_sector_size(total_sectors),
            total_sectors
        )];
        let (resolved, source) = match self.provision_resolved_prefill() {
            Ok(value) => value,
            Err(message) => {
                lines.push(format!("✗ 布局无效: {message}"));
                return lines;
            }
        };
        let mut parts = match resolved.target_partitions(crate::common::SECTOR as u64) {
            Ok(parts) => parts,
            Err(message) => {
                lines.push(format!("✗ 布局无效: {message}"));
                return lines;
            }
        };
        parts.sort_by_key(|part| part.start_lba);
        let usable_start = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
        let usable_end_exclusive = resolved.usable_end_lba;
        let usable_sectors = usable_end_exclusive.saturating_sub(usable_start);
        let allocated = parts.iter().map(|part| part.sector_count).sum::<u64>();
        let unallocated = crate::provision::validate_target_geometry(&parts, usable_end_exclusive)
            .unwrap_or_default();
        lines.push(format!(
            "可分区 LBA {}–{}  ·  {}",
            usable_start,
            usable_end_exclusive.saturating_sub(1),
            Self::format_sector_size(usable_sectors)
        ));
        lines.push(format!(
            "已分配 {}  ·  未分配 {}",
            Self::format_sector_size(allocated),
            Self::format_sector_size(unallocated)
        ));

        lines.push(String::new());

        let mut cursor = usable_start;
        for part in &parts {
            if part.start_lba > cursor {
                let gap = part.start_lba - cursor;
                lines.push(format!(
                    "空闲  LBA {}–{}  ·  {}",
                    cursor,
                    part.start_lba - 1,
                    Self::format_sector_size(gap)
                ));
            }
            let end = part.start_lba + part.sector_count - 1;
            let candidate = source
                .as_ref()
                .and_then(|profile| profile.partition(part.role))
                .is_some_and(|old| {
                    part.role != PartitionRole::CompatibilityReserve
                        && old.partition_type == part.partition_type
                        && old.start_lba == part.start_lba
                        && old.sector_count == part.sector_count
                        && old.physically_encrypted == part.physically_encrypted
                });
            lines.push(format!(
                "{}  LBA {}–{}  ·  {}  ·  {}",
                part.role.label(),
                part.start_lba,
                end,
                Self::format_sector_size(part.sector_count),
                if candidate { "可保留" } else { "重建" }
            ));
            cursor = part.start_lba.saturating_add(part.sector_count);
        }
        if unallocated > 0 && cursor < usable_end_exclusive {
            lines.push(format!(
                "空闲  LBA {}–{}  ·  {}",
                cursor,
                usable_end_exclusive - 1,
                Self::format_sector_size(usable_end_exclusive - cursor)
            ));
        }

        lines.push(String::new());
        match self.provision_selected_capacity_limit() {
            Ok(Some((role, current_sectors, max_sectors, limiter, usable_end_lba))) => {
                let grow = max_sectors.saturating_sub(current_sectors);
                lines.push(format!("当前: {}", role.label()));
                lines.push(format!(
                    "大小 {} ({} sector)",
                    Self::format_sector_size(current_sectors),
                    current_sectors
                ));
                lines.push(format!(
                    "最大可设 {} ({} sector)",
                    Self::format_sector_size(max_sectors),
                    max_sectors
                ));
                lines.push(format!("还能增加 {}", Self::format_sector_size(grow)));
                if let Some((next_role, next_start)) = limiter {
                    lines.push(format!(
                        "限制: 后续{}固定起点 LBA {}",
                        next_role.label(),
                        next_start
                    ));
                } else {
                    lines.push(format!(
                        "限制: 可分区末端 LBA {}；后续未锚定分区可自动后移",
                        usable_end_lba.saturating_sub(1)
                    ));
                }
            }
            Ok(None) => lines.push("选中分区容量/单位/起点，可查看最大可设范围".into()),
            Err(message) => lines.push(format!("布局限制无效: {message}")),
        }
        lines.push("✓ 当前布局无重叠、未越界".into());
        lines
    }

    pub fn provision_layout_model(&self) -> crate::tui::disk_layout::DiskLayoutModel {
        use crate::provision::PartitionRole;
        use crate::tui::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};

        if self.provision.kind == ProvisionKind::Plain {
            let Ok(plan) = self.provision_plain_plan() else {
                return DiskLayoutModel::new(0, Vec::new());
            };
            let mut segments = vec![DiskLayoutSegment {
                label: "MBR 保留扇区".into(),
                start_lba: 0,
                sector_count: 1,
                kind: DiskRegionKind::Reserved,
            }];
            segments.extend(plan.gaps.iter().map(|gap| DiskLayoutSegment {
                label: "空闲".into(),
                start_lba: gap.start_lba,
                sector_count: gap.sector_count,
                kind: DiskRegionKind::Free,
            }));
            segments.extend(plan.partitions.iter().enumerate().map(|(index, part)| {
                DiskLayoutSegment {
                    label: format!("普通分区[{}]", index + 1),
                    start_lba: part.start_lba,
                    sector_count: part.sector_count,
                    kind: DiskRegionKind::Plain,
                }
            }));
            let model = DiskLayoutModel::new(plan.total_sectors, segments);
            debug_assert!(model.validate_complete().is_ok());
            return model;
        }

        let total_sectors = self
            .selected_device()
            .map(|row| row.size / crate::common::SECTOR as u64)
            .unwrap_or_default();
        if total_sectors == 0 {
            return DiskLayoutModel::new(0, Vec::new());
        }
        let Some(lce) =
            crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
                total_sectors,
                crate::common::SECTOR as u32,
            )
        else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let Ok((resolved, _)) = self.provision_resolved_prefill() else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let Ok(mut parts) = resolved.target_partitions(crate::common::SECTOR as u64) else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        parts.sort_by_key(|part| part.start_lba);

        let mut claims = Vec::<DiskLayoutSegment>::new();
        let mut push = |label: &str, start_lba: u64, sector_count: u64, kind: DiskRegionKind| {
            let count = sector_count.min(total_sectors.saturating_sub(start_lba));
            if count > 0 && start_lba < total_sectors {
                claims.push(DiskLayoutSegment {
                    label: label.into(),
                    start_lba,
                    sector_count: count,
                    kind,
                });
            }
        };
        push("EDP 主协议区", 0, 13, DiskRegionKind::Protocol);
        push(
            "协议后保留区",
            13,
            crate::provision::OFFICIAL_PARTITION_START_SECTOR.saturating_sub(13),
            DiskRegionKind::Reserved,
        );
        let usable_start = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
        if resolved.usable_end_lba > usable_start {
            push(
                "可分配空间",
                usable_start,
                resolved.usable_end_lba - usable_start,
                DiskRegionKind::Free,
            );
        }
        for part in &parts {
            let kind = match part.role {
                PartitionRole::Boot => DiskRegionKind::Boot,
                PartitionRole::Share => DiskRegionKind::Share,
                PartitionRole::BootShareCombined => DiskRegionKind::Combined,
                PartitionRole::Encrypt => DiskRegionKind::Encrypt,
                PartitionRole::CompatibilityReserve => DiskRegionKind::Compatibility,
            };
            push(part.role.label(), part.start_lba, part.sector_count, kind);
        }
        push(
            "LCE legacy compatibility extent",
            lce.start_lba,
            lce.size_sectors,
            DiskRegionKind::Lce,
        );
        let lce_end = lce.start_lba.saturating_add(lce.size_sectors);
        if lce_end < total_sectors {
            push(
                "LCE 后保留区",
                lce_end,
                total_sectors - lce_end,
                DiskRegionKind::Reserved,
            );
        }
        let tail_count = total_sectors.min(crate::backup_metadata::DEVICE_TAIL_WINDOW_SECTORS);
        push(
            "盘尾区域",
            total_sectors - tail_count,
            tail_count,
            DiskRegionKind::Tail,
        );

        DiskLayoutModel::from_claims(total_sectors, claims, DiskRegionKind::Reserved)
    }

    pub fn provision_layout_bar(
        &self,
        width: usize,
    ) -> Vec<crate::tui::disk_layout::DiskRegionKind> {
        self.provision_layout_model().bar(width)
    }
}
