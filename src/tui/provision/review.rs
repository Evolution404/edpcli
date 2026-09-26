use super::*;

impl AppState {
    pub fn provision_review_summary_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "✓ 计划已通过全部只读校验".into(),
            self.provision.kind.title().into(),
        ];
        match self.provision.prepared.as_ref() {
            Some(ProvisionPrepared::Plain(prepared)) => {
                lines.push(format!(
                    "目标: disk{} · 恢复普通盘 · {} 个 MBR 主分区 · {} sectors",
                    prepared.disk,
                    prepared.plan.partitions.len(),
                    prepared.plan.total_sectors
                ));
                lines.push(format!(
                    "来源状态: {} · 来源 LCE cleanup: {}",
                    prepared.source_kind.short_name(),
                    prepared
                        .source_lce_start_lba
                        .and_then(|lba| lba.checked_add(6).and_then(|end| {
                            crate::application::inspect_tree::format_lba_closed_range(lba, end)
                        }))
                        .unwrap_or_else(|| "无".into())
                ));
                lines.push(format!(
                    "事务触碰: {} sectors · 最高写入 LBA: {}",
                    prepared.write_plan.touched_sector_count(),
                    prepared.write_plan.highest_touched_lba().unwrap_or(0)
                ));
                lines.push("LBA3 已从目标盘捕获并绑定；写入前将再次复核。".into());
            }
            Some(ProvisionPrepared::Official(prepared)) => {
                lines.extend([
                    format!("目标: disk{} · {}", prepared.disk, prepared.device_id),
                    format!(
                        "容量: {} sectors · LCE: LBA{}",
                        prepared.write_image.total_sectors, prepared.lce_start_lba
                    ),
                    format!(
                        "事务触碰: {} sectors · 最高写入 LBA: {}",
                        prepared.write_image.touched_sector_count(),
                        prepared.write_image.highest_touched_lba().unwrap_or(0)
                    ),
                    "LBA3 已从目标盘捕获并绑定；写入前将再次复核。".into(),
                    format!(
                        "初始化密码强制修改: {}",
                        if prepared.force_change_password {
                            "是"
                        } else {
                            "否"
                        }
                    ),
                    format!(
                        "取消密码复杂性验证: {}",
                        if prepared.pass_info_policy.cancel_password_complexity_check {
                            "是"
                        } else {
                            "否"
                        }
                    ),
                    format!(
                        "交换区密码最大错误次数: {}",
                        prepared.pass_info_policy.max_share_password_errors
                    ),
                    format!(
                        "保密区密码最大错误次数: {}",
                        prepared.pass_info_policy.max_encrypt_password_errors
                    ),
                ]);
            }
            None => lines.push("计划对象尚未准备。".into()),
        }
        if let Some(message) = &self.provision.message {
            lines.push(message.clone());
        }
        lines.extend([
            "Enter 进入最终 YES 确认".into(),
            "e 导出目标绑定镜像 · Esc 返回修改".into(),
        ]);
        lines
    }

    pub fn provision_review_change_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        match self.provision.prepared.as_ref() {
            Some(ProvisionPrepared::Plain(prepared)) => {
                for (index, part) in prepared.plan.partitions.iter().enumerate() {
                    lines.push(format!(
                        "P{} {} · {} sectors · {} · 卷标:{}",
                        index + 1,
                        part.end_exclusive()
                            .ok()
                            .and_then(|end| {
                                crate::application::inspect_tree::format_lba_closed_range(
                                    part.start_lba,
                                    end,
                                )
                            })
                            .unwrap_or_else(|| "[无效范围]".into()),
                        part.sector_count,
                        part.filesystem.windows_format_name(),
                        part.volume_label
                    ));
                }
                for gap in &prepared.plan.gaps {
                    lines.push(format!(
                        "空闲 {} · {} sectors",
                        gap.start_lba
                            .checked_add(gap.sector_count)
                            .and_then(|end| {
                                crate::application::inspect_tree::format_lba_closed_range(
                                    gap.start_lba,
                                    end,
                                )
                            })
                            .unwrap_or_else(|| "[无效范围]".into()),
                        gap.sector_count
                    ));
                }
                lines.push("⚠ 将清除 EDP 协议状态并重建上述普通分区；这不是安全擦除。".into());
            }
            Some(ProvisionPrepared::Official(prepared)) => {
                lines.push("先写协议/LCE 并验证，再对勾选的分区单独格式化并验证。".into());
                lines.push("制盘后格式化:".into());
                for choice in &prepared.format_targets {
                    let target = &choice.target;
                    lines.push(format!(
                        "{} {} type{} {} {}{} 卷标:{}",
                        if !target.format_capable {
                            "—"
                        } else if choice.selected {
                            "☑"
                        } else {
                            "☐"
                        },
                        target.role.label(),
                        target.geometry.partition_type.raw(),
                        if !target.format_capable {
                            "不可格式化"
                        } else if target.physically_encrypted {
                            "加密"
                        } else {
                            "明文"
                        },
                        choice
                            .filesystem
                            .map(|format| format.windows_format_name())
                            .unwrap_or("—"),
                        target
                            .visible_mbr_type
                            .map(|mbr| format!(" / MBR 0x{mbr:02X}"))
                            .unwrap_or_default(),
                        if target.format_capable {
                            choice.volume_label.as_str()
                        } else {
                            "—"
                        }
                    ));
                }
                if let Some(target_plan) = &prepared.target_plan {
                    lines.push(format!(
                        "未分配空间: {} sectors",
                        target_plan.unallocated_sectors
                    ));
                    for part in &target_plan.partitions {
                        let action = match part.action {
                            crate::provision::PartitionAction::PreserveExact => {
                                "原数据可保留 · 复用原 FileKey · 不写数据区"
                            }
                            crate::provision::PartitionAction::Rebuild => {
                                "将重建 · 原数据不可原样保留"
                            }
                        };
                        lines.push(format!(
                            "{} {} ({} sectors): {}",
                            part.geometry.role.label(),
                            part.geometry
                                .start_lba
                                .checked_add(part.geometry.sector_count)
                                .and_then(|end| {
                                    crate::application::inspect_tree::format_lba_closed_range(
                                        part.geometry.start_lba,
                                        end,
                                    )
                                })
                                .unwrap_or_else(|| "[无效范围]".into()),
                            part.geometry.sector_count,
                            action
                        ));
                        lines.push(format!("  {}", part.reason));
                    }
                    if target_plan
                        .partitions
                        .iter()
                        .all(|part| part.action == crate::provision::PartitionAction::Rebuild)
                    {
                        lines.push("e 导出与该目标绑定的稀疏制盘镜像".into());
                    }
                }
            }
            None => lines.push("暂无变更明细。".into()),
        }
        lines
    }
}
