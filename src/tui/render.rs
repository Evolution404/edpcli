//! Ratatui rendering for the top-level shell.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Tabs, Wrap},
    Frame,
};

use super::animation::CoreMode;
use super::state::{
    AppState, InputMode, ProvisionKind, ProvisionStage, WizardStage, Workspace, WriteKind,
};

#[path = "backups/render.rs"]
mod backups_render;
#[path = "devices/render.rs"]
mod devices_render;
#[path = "inspect/render.rs"]
mod inspect_render;
#[path = "provision/render.rs"]
mod provision_render;

use backups_render::{
    draw_backup_batch_delete, draw_backup_create_choice, draw_backup_delete, draw_backup_prune,
    draw_backups, write_progress_text,
};
use devices_render::draw_devices;
use inspect_render::draw_advanced_inspect;
use provision_render::{draw_provision, draw_scheme_picker};

fn backup_health(backup: &crate::application::BackupWorkspaceItem) -> (&'static str, Style) {
    if !backup.size_ok {
        ("大小异常", danger())
    } else {
        match backup.integrity_status {
            crate::application::BackupIntegrityStatus::Verified => ("EDPB ✓", success()),
            crate::application::BackupIntegrityStatus::Invalid => ("EDPB ✗", danger()),
        }
    }
}

fn safe(value: &str) -> String {
    crate::ui::sanitize_terminal_text(value)
}

fn fit_display_width(value: &str, width: usize) -> String {
    let value = safe(value);
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "│".into();
    }
    let current = crate::ui::disp_width(&value);
    if current <= width {
        return format!("{value}{}", " ".repeat(width - current));
    }

    let target = width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0usize;
    for ch in value.chars() {
        let ch_width = crate::ui::disp_width(&ch.to_string()).max(1);
        if used + ch_width > target {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    if width > 0 {
        out.push('…');
        used += 1;
    }
    if used < width {
        out.push_str(&" ".repeat(width - used));
    }
    out
}

fn input_value_window(value: &str, cursor: usize, width: usize, secret: bool) -> String {
    if width == 0 {
        return String::new();
    }
    let sanitized = safe(value);
    let chars = if secret {
        vec!['•'; sanitized.chars().count()]
    } else {
        sanitized.chars().collect::<Vec<_>>()
    };
    let cursor = cursor.min(chars.len());
    if width == 1 {
        return "│".into();
    }

    let window_width = |start: usize, end: usize| {
        let content = chars[start..end]
            .iter()
            .map(|ch| crate::ui::disp_width(&ch.to_string()).max(1))
            .sum::<usize>();
        1 + content + usize::from(start > 0) + usize::from(end < chars.len())
    };

    let mut start = cursor;
    let mut end = cursor;
    loop {
        let mut progressed = false;
        if start > 0 && window_width(start - 1, end) <= width {
            start -= 1;
            progressed = true;
        }
        if end < chars.len() && window_width(start, end + 1) <= width {
            end += 1;
            progressed = true;
        }
        if !progressed {
            break;
        }
    }

    let mut out = String::new();
    if start > 0 {
        out.push('‹');
    }
    for ch in &chars[start..cursor] {
        out.push(*ch);
    }
    out.push('│');
    for ch in &chars[cursor..end] {
        out.push(*ch);
    }
    if end < chars.len() {
        out.push('›');
    }
    out
}

fn hard_wrap_value(value: &str, width: usize) -> Vec<String> {
    let value = safe(value);
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in value.chars() {
        let char_width = crate::ui::disp_width(&ch.to_string()).max(1);
        if current_width > 0 && current_width + char_width > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += char_width;
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn wrapped_field_lines(
    label: &'static str,
    value: &str,
    content_width: usize,
) -> Vec<Line<'static>> {
    let label_width = crate::ui::disp_width(label);
    let value_width = content_width.saturating_sub(label_width).max(1);
    hard_wrap_value(value, value_width)
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            if index == 0 {
                Line::from(vec![Span::styled(label, muted()), Span::raw(chunk)])
            } else {
                Line::from(vec![Span::raw(" ".repeat(label_width)), Span::raw(chunk)])
            }
        })
        .collect()
}

fn accent() -> Style {
    super::theme::current().accent()
}

fn secondary() -> Style {
    super::theme::current().secondary_accent()
}

fn success() -> Style {
    super::theme::current().success()
}

fn warning() -> Style {
    super::theme::current().warning()
}

fn danger() -> Style {
    super::theme::current().danger()
}

fn muted() -> Style {
    super::theme::current().muted()
}

fn selected() -> Style {
    super::theme::current().selection()
}

fn selection_marker() -> Style {
    super::theme::current().selection_marker()
}

fn panel() -> Style {
    super::theme::current().panel()
}

fn focused_panel() -> Style {
    super::theme::current().focused_panel()
}

fn tab() -> Style {
    super::theme::current().tab()
}

fn active_tab() -> Style {
    super::theme::current().active_tab()
}

fn input() -> Style {
    super::theme::current().input()
}

fn input_focused() -> Style {
    super::theme::current().input_focused()
}

fn provision_kind_style(kind: ProvisionKind) -> Style {
    super::theme::current().provision_kind(kind)
}

fn device_status_style(row: &crate::disk_scan::Row) -> Style {
    if row.proto != "USB" || row.denied {
        warning()
    } else if row.probe_error.is_some() {
        danger()
    } else {
        match row.confirmed_provision_kind() {
            Some(crate::provision::DiskProvisionKind::Plain) => muted(),
            Some(_) => accent(),
            None => warning(),
        }
    }
}

fn device_status(row: &crate::disk_scan::Row) -> String {
    if row.proto != "USB" {
        "非 USB / 不支持".into()
    } else if row.denied {
        "需要管理员权限".into()
    } else if let Some(error) = &row.probe_error {
        format!("读取异常: {}", safe(error))
    } else {
        "可用".into()
    }
}

fn visible_window(selected: usize, total: usize, area_height: u16) -> std::ops::Range<usize> {
    let capacity = usize::from(area_height.saturating_sub(3)).max(1);
    let start = selected
        .saturating_sub(capacity / 2)
        .min(total.saturating_sub(capacity));
    start..(start + capacity).min(total)
}

fn draw_command_palette(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let commands = [
        "devices  切到设备",
        "backups  切到备份",
        "provision 制盘",
        "inspect  全盘结构树 / Sector Inspector",
        "restore  Restore 安全向导",
        "backup-create  备份当前设备",
        "backup-verify  校验当前备份",
        "backup-delete  删除当前备份",
        "batch-delete  删除空格勾选的多份备份",
        "backup-prune   keep-N 清理旧备份",
        "refresh  刷新当前工作区",
        "help     帮助",
        "quit/q   退出",
    ];
    let mut lines = vec![
        Line::from(format!(":{}", safe(state.input_buffer()))),
        Line::from(""),
    ];
    lines.extend(commands.into_iter().map(Line::from));
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Command Palette"),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_wizard(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(wizard) = state.wizard() else {
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    let title = if wizard.kind == WriteKind::Restore {
        "恢复向导"
    } else {
        "备份向导"
    };

    let breadcrumb = match wizard.stage {
        WizardStage::Review => "备份 > 恢复 > 确认",
        WizardStage::Confirm => {
            if wizard.kind == WriteKind::Restore {
                "备份 > 恢复 > 最终确认"
            } else {
                "备份 > 创建 > 确认"
            }
        }
        WizardStage::Running => {
            if wizard.kind == WriteKind::Restore {
                "备份 > 恢复 > 执行"
            } else {
                "备份 > 创建 > 执行"
            }
        }
        WizardStage::PostRestore => "备份 > 恢复 > 恢复后处理",
        WizardStage::VolumeLabelInput => "备份 > 恢复 > 恢复后处理 > 卷标",
        WizardStage::PasswordInput => "备份 > 恢复 > 恢复后处理 > 原密码",
        WizardStage::EncryptedFormatConfirm => "备份 > 恢复 > 恢复后处理 > 加密格式化确认",
        WizardStage::FormatConfirm => "备份 > 恢复 > 恢复后处理 > 格式化确认",
        WizardStage::Formatting => "备份 > 恢复 > 恢复后处理 > 格式化",
        WizardStage::ReinitializePassword => "备份 > 恢复 > 恢复后处理 > 设置新密码",
        WizardStage::ReinitializePasswordConfirm => "备份 > 恢复 > 恢复后处理 > 确认新密码",
        WizardStage::ReinitializeConfirm => "备份 > 恢复 > 恢复后处理 > 重建确认",
        WizardStage::Reinitializing => "备份 > 恢复 > 恢复后处理 > 重建加密分区",
        WizardStage::Result => {
            if wizard.kind == WriteKind::Restore {
                "备份 > 恢复 > 结果"
            } else {
                "备份 > 创建 > 结果"
            }
        }
    };
    lines.push(Line::from(Span::styled(
        breadcrumb,
        accent().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    match wizard.stage {
        WizardStage::Review => {
            lines.push(Line::from(Span::styled(
                "恢复目标",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(vec![
                Span::styled(format!("disk{}", wizard.disk), accent()),
                Span::styled(
                    "  当前系统设备节点；编号可随重新插拔变化，不参与物理身份判断",
                    muted(),
                ),
            ]));
            lines.push(Line::from(""));

            lines.push(Line::from(Span::styled(
                "恢复来源",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            if let Some(path) = &wizard.backup {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<无效文件名>");
                lines.push(Line::from(Span::styled(safe(name), secondary())));
                lines.push(Line::from(Span::styled(
                    "文件名中的 diskN 仅记录备份时系统编号，不参与介质身份认证。",
                    muted(),
                )));
                if wizard.detail_expanded {
                    lines.push(Line::from(vec![
                        Span::styled("路径  ", muted()),
                        Span::styled(safe(&path.display().to_string()), muted()),
                    ]));
                }
            }
            lines.push(Line::from(""));

            lines.push(Line::from(Span::styled(
                "⚠ 将执行元数据恢复",
                warning().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(vec![
                Span::styled("恢复  ", success()),
                Span::raw("分区结构、磁盘身份相关元数据"),
            ]));
            lines.push(Line::from(vec![
                Span::styled("不恢复  ", muted()),
                Span::styled("文件系统、目录、文件内容", muted()),
            ]));
            lines.push(Line::from(""));

            let (identity_text, identity_style) = if wizard.expected_identity.is_some() {
                ("目标身份已固定 ✓", success())
            } else {
                ("目标身份尚未固定", warning())
            };
            lines.push(Line::from(vec![
                Span::styled("安全检查  ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(identity_text, identity_style),
                Span::raw("  "),
                Span::styled("备份/几何将在写前复核 ✓", success()),
            ]));
            if wizard.detail_expanded {
                for item in [
                    "系统盘 / USB 整盘检查",
                    "selector pinning",
                    "写前保护",
                    "卸载 / 锁卷",
                    "reopen 身份复核",
                    "atomic write",
                    "sync / readback / rollback",
                ] {
                    lines.push(Line::from(vec![
                        Span::styled("  ✓ ", success()),
                        Span::styled(item, muted()),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled("o 展开安全链详情", muted())));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Enter", accent().add_modifier(Modifier::BOLD)),
                Span::raw(" 继续    "),
                Span::styled("Esc", muted()),
                Span::raw(" 返回"),
            ]));
        }
        WizardStage::Confirm => {
            let destructive = wizard.kind == WriteKind::Restore;
            lines.push(Line::from(Span::styled(
                if destructive {
                    format!("⚠ 即将修改 disk{}", wizard.disk)
                } else {
                    format!("即将创建 disk{} 的元数据备份", wizard.disk)
                },
                if destructive {
                    warning().add_modifier(Modifier::BOLD)
                } else {
                    accent().add_modifier(Modifier::BOLD)
                },
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(if destructive {
                "当前分区结构将被备份中的结构替换；该操作不会恢复文件内容。"
            } else {
                "仅读取设备元数据，不会卸载或写入 U 盘。"
            }));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("输入 YES 继续", warning())));
            lines.push(Line::from(Span::styled(
                format!(" {} ", safe(&wizard.confirmation)),
                super::theme::current().input_focused(),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), danger())));
            }
        }
        WizardStage::Running => {
            lines.push(Line::from(Span::styled(
                if wizard.kind == WriteKind::Restore {
                    "元数据恢复进行中"
                } else {
                    "元数据备份进行中"
                },
                accent().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                if wizard.kind == WriteKind::Restore {
                    "关键事务执行期间 q / Esc / Ctrl-C 不会中断写入、读回或回滚。"
                } else {
                    "只读备份任务执行期间可等待安全结束点。"
                },
                muted(),
            )));
            lines.push(Line::from(""));
            if let Some(event) = wizard.progress.as_ref() {
                lines.push(Line::from(vec![
                    Span::styled("● ", accent()),
                    Span::raw(safe(&write_progress_text(event))),
                ]));
            } else if let Some(message) = &wizard.message {
                lines.push(Line::from(vec![
                    Span::styled("● ", accent()),
                    Span::raw(safe(message)),
                ]));
            }
            if wizard.detail_expanded {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("详细日志", muted())));
                for event in wizard.progress_log.iter().rev().take(8).rev() {
                    lines.push(Line::from(vec![
                        Span::styled("  · ", muted()),
                        Span::styled(safe(&write_progress_text(event)), muted()),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled("o 展开详细日志", muted())));
            }
        }
        WizardStage::PostRestore => {
            lines.push(Line::from(Span::styled(
                "元数据恢复成功 ✓",
                success().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(
                "分区结构与元数据已完成写入并通过读回校验；文件系统内容没有恢复。",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "恢复后分区状态",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            if let Some(outcome) = wizard.restore_outcome.as_ref() {
                if outcome.assessment.partitions.is_empty() {
                    lines.push(Line::from(Span::styled("  没有可显示的分区状态", muted())));
                }
                for (index, partition) in outcome.assessment.partitions.iter().enumerate() {
                    use crate::application::post_restore::PostRestorePartitionState;
                    let (state_text, state_style) = match partition.state {
                        PostRestorePartitionState::Usable => ("可用 ✓", success()),
                        PostRestorePartitionState::NeedsFormat => ("需要格式化", warning()),
                        PostRestorePartitionState::PasswordRequired => ("需要原密码", warning()),
                        PostRestorePartitionState::CryptoMetadataInvalid => {
                            ("加密元数据异常", danger())
                        }
                        PostRestorePartitionState::Unsupported => ("暂不支持", muted()),
                    };
                    let marker = if index == wizard.post_restore_selected {
                        ">"
                    } else {
                        " "
                    };
                    let capacity = crate::common::fmt_capacity(
                        partition
                            .sector_count
                            .saturating_mul(crate::common::SECTOR as u64),
                    );
                    let line_style = if index == wizard.post_restore_selected {
                        selected()
                    } else {
                        Style::default()
                    };
                    lines.push(
                        Line::from(vec![
                            Span::styled(format!("{marker} "), selection_marker()),
                            Span::styled(format!("分区 {}  ", partition.index), line_style),
                            Span::styled(format!("{capacity:<10} "), line_style),
                            Span::styled(state_text, state_style),
                        ])
                        .style(line_style),
                    );
                    if index == wizard.post_restore_selected {
                        lines.push(Line::from(vec![
                            Span::styled("    ", muted()),
                            Span::styled(
                                format!("LBA{} + {}", partition.start_lba, partition.sector_count),
                                muted(),
                            ),
                        ]));
                        if wizard.detail_expanded {
                            lines.push(Line::from(vec![
                                Span::styled("    ", muted()),
                                Span::styled(safe(&partition.detail), muted()),
                            ]));
                        }
                    }
                }
            }
            lines.push(Line::from(""));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), secondary())));
            }
            lines.push(Line::from(vec![
                Span::styled("Enter", accent().add_modifier(Modifier::BOLD)),
                Span::raw(" 处理选中分区    "),
                Span::styled("j/k", muted()),
                Span::raw(" 选择    "),
                Span::styled("o", muted()),
                Span::raw(" 详情    "),
                Span::styled("Esc", muted()),
                Span::raw(" 完成"),
            ]));
        }
        WizardStage::VolumeLabelInput => {
            let request = wizard.pending_format.as_ref();
            let original_label = wizard.restore_outcome.as_ref().and_then(|outcome| {
                request.and_then(|request| {
                    outcome
                        .partitions
                        .iter()
                        .find(|partition| partition.index == request.partition_index)
                        .and_then(|partition| partition.volume_label_hint.as_deref())
                })
            });
            lines.push(Line::from(Span::styled(
                "恢复后的卷标",
                accent().add_modifier(Modifier::BOLD),
            )));
            if let Some(request) = request {
                lines.push(Line::from(vec![
                    Span::styled("文件系统  ", muted()),
                    Span::styled(request.filesystem.config_token(), accent()),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("来源  ", muted()),
                Span::styled(
                    if original_label.is_some() {
                        "备份中的原卷标"
                    } else {
                        "备份没有卷标提示"
                    },
                    secondary(),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("卷标", muted())));
            lines.push(Line::from(Span::styled(
                format!(" {} ", safe(&wizard.volume_label_input)),
                super::theme::current().input_focused(),
            )));
            lines.push(Line::from(Span::styled(
                "留空表示创建无用户卷标的文件系统；程序不会自动生成占位名称。",
                muted(),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), warning())));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Enter", accent().add_modifier(Modifier::BOLD)),
                Span::raw(" 继续    "),
                Span::styled("Esc", muted()),
                Span::raw(" 返回"),
            ]));
        }
        WizardStage::PasswordInput => {
            lines.push(Line::from(Span::styled(
                "验证原密码",
                warning().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(
                "需要验证原密码后才能解包并校验原 FileKey；验证失败不会写盘。",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("原密码", muted())));
            lines.push(Line::from(Span::styled(
                format!(" {} ", "•".repeat(state.wizard_secret_len())),
                super::theme::current().input_focused(),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), warning())));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Enter", accent()),
                Span::raw(" 验证并继续    "),
                Span::styled("Esc", muted()),
                Span::raw(" 返回"),
            ]));
        }
        WizardStage::EncryptedFormatConfirm => {
            let request = wizard.pending_format.as_ref();
            let partition = wizard.restore_outcome.as_ref().and_then(|outcome| {
                request.and_then(|request| {
                    outcome
                        .assessment
                        .partitions
                        .iter()
                        .find(|partition| partition.index == request.partition_index)
                })
            });
            lines.push(Line::from(Span::styled(
                "使用原密钥域格式化",
                warning().add_modifier(Modifier::BOLD),
            )));
            if let (Some(request), Some(partition)) = (request, partition) {
                lines.push(Line::from(format!(
                    "分区 {}  ·  LBA{} + {}  ·  {}",
                    partition.index,
                    partition.start_lba,
                    partition.sector_count,
                    request.filesystem.config_token()
                )));
            }
            lines.push(Line::from(vec![
                Span::styled("卷标  ", muted()),
                Span::styled(
                    if wizard.volume_label_input.is_empty() {
                        "(无卷标)"
                    } else {
                        wizard.volume_label_input.as_str()
                    },
                    secondary(),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "✓ 不生成新 FileKey，不修改原密码或密钥记录",
                success(),
            )));
            lines.push(Line::from(
                "将使用验证后的原 FileKey 创建新的空加密文件系统；原文件内容不会恢复。",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "输入 YES 独立确认加密格式化",
                warning(),
            )));
            lines.push(Line::from(Span::styled(
                format!(" {} ", safe(&wizard.confirmation)),
                super::theme::current().input_focused(),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), secondary())));
            }
        }
        WizardStage::ReinitializePassword | WizardStage::ReinitializePasswordConfirm => {
            let confirm = wizard.stage == WizardStage::ReinitializePasswordConfirm;
            lines.push(Line::from(Span::styled(
                if confirm {
                    "再次输入新密码"
                } else {
                    "设置新的加密分区密码"
                },
                danger().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "⚠ 这是密钥域重建流程：旧 FileKey 与旧密码将永久失效。",
                danger(),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                if confirm {
                    "确认新密码"
                } else {
                    "新密码"
                },
                muted(),
            )));
            lines.push(Line::from(Span::styled(
                format!(" {} ", "•".repeat(state.wizard_secret_len())),
                super::theme::current().input_focused(),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), warning())));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Enter", accent()),
                Span::raw(if confirm {
                    " 校验两次密码    "
                } else {
                    " 下一步    "
                }),
                Span::styled("Esc", muted()),
                Span::raw(" 返回"),
            ]));
        }
        WizardStage::ReinitializeConfirm => {
            let request = wizard.pending_format.as_ref();
            lines.push(Line::from(Span::styled(
                "清空并重建加密分区",
                danger().add_modifier(Modifier::BOLD),
            )));
            if let Some(request) = request {
                lines.push(Line::from(format!(
                    "分区 {}  ·  新文件系统 {}",
                    request.partition_index,
                    request.filesystem.config_token()
                )));
            }
            lines.push(Line::from(vec![
                Span::styled("卷标  ", muted()),
                Span::styled(
                    if wizard.volume_label_input.is_empty() {
                        "(无卷标)"
                    } else {
                        wizard.volume_label_input.as_str()
                    },
                    secondary(),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "⚠ 将生成新的 FileKey，更新 LBA7/LBA12 密钥域，并创建新的空加密文件系统。",
                danger(),
            )));
            lines.push(Line::from(
                "该动作不可恢复旧密钥域；元数据恢复本身的成功结果不会因此改变。",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("输入 YES 最终确认重建", danger())));
            lines.push(Line::from(Span::styled(
                format!(" {} ", safe(&wizard.confirmation)),
                super::theme::current().input_focused(),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), danger())));
            }
        }
        WizardStage::Reinitializing => {
            lines.push(Line::from(Span::styled(
                "正在重建加密分区",
                danger().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(
                "生成新 FileKey → 更新密钥记录 → 创建空加密文件系统 → 读回 → 使用新密码重新评估。",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("● ", danger()),
                Span::raw(wizard.message.as_deref().unwrap_or("正在执行密钥域重建…")),
            ]));
            lines.push(Line::from(Span::styled(
                "q / Esc / Ctrl-C 将延迟到安全结束点。",
                muted(),
            )));
        }
        WizardStage::FormatConfirm => {
            let request = wizard.pending_format.as_ref();
            let partition = wizard.restore_outcome.as_ref().and_then(|outcome| {
                request.and_then(|request| {
                    outcome
                        .assessment
                        .partitions
                        .iter()
                        .find(|partition| partition.index == request.partition_index)
                })
            });
            lines.push(Line::from(Span::styled(
                "格式化分区",
                warning().add_modifier(Modifier::BOLD),
            )));
            if let (Some(request), Some(partition)) = (request, partition) {
                lines.push(Line::from(vec![
                    Span::styled("分区  ", muted()),
                    Span::raw(partition.index.to_string()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("范围  ", muted()),
                    Span::raw(format!(
                        "LBA{} + {}",
                        partition.start_lba, partition.sector_count
                    )),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("文件系统  ", muted()),
                    Span::styled(request.filesystem.config_token(), accent()),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("卷标  ", muted()),
                Span::styled(
                    if wizard.volume_label_input.is_empty() {
                        "(无卷标)"
                    } else {
                        wizard.volume_label_input.as_str()
                    },
                    secondary(),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "⚠ 将创建新的空文件系统，不会恢复原文件或目录。",
                warning(),
            )));
            lines.push(Line::from(
                "这是独立于元数据恢复的第二次破坏性操作，必须再次确认。",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("输入 YES 确认格式化", warning())));
            lines.push(Line::from(Span::styled(
                format!(" {} ", safe(&wizard.confirmation)),
                super::theme::current().input_focused(),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(Span::styled(safe(message), danger())));
            }
        }
        WizardStage::Formatting => {
            lines.push(Line::from(Span::styled(
                "正在格式化选中分区",
                warning().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(
                "格式化使用独立安全链：身份复核 → 状态复核 → 卸载/锁卷 → reopen → 写入 → 读回 → 重新评估。",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("● ", accent()),
                Span::raw(
                    wizard
                        .message
                        .as_deref()
                        .unwrap_or("正在创建新的空文件系统…"),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "q / Esc / Ctrl-C 将延迟到安全结束点。",
                muted(),
            )));
        }
        WizardStage::Result => {
            let ok = wizard
                .message
                .as_deref()
                .is_some_and(|message| !message.starts_with("错误"));
            lines.push(Line::from(Span::styled(
                if ok { "操作完成" } else { "操作结束" },
                if ok { success() } else { danger() }.add_modifier(Modifier::BOLD),
            )));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(safe(message)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Enter / Esc 返回", muted())));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            if matches!(
                wizard.stage,
                WizardStage::Confirm
                    | WizardStage::EncryptedFormatConfirm
                    | WizardStage::FormatConfirm
                    | WizardStage::Formatting
                    | WizardStage::ReinitializePassword
                    | WizardStage::ReinitializePasswordConfirm
                    | WizardStage::ReinitializeConfirm
                    | WizardStage::Reinitializing
            ) {
                warning()
            } else {
                super::theme::current().focused_panel()
            },
        )
        .title(title);
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

pub fn draw(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(super::theme::current().background()),
        area,
    );
    let core_mode = if state.is_critical_operation() {
        CoreMode::Guard
    } else if (state.workspace() == Workspace::Inspect && state.advanced_inspect().is_some())
        || state.active_scan_pending()
        || state.wizard().is_some()
        || (state.workspace() == Workspace::Provision
            && state.provision().stage != ProvisionStage::SelectDisk)
    {
        CoreMode::Busy
    } else {
        CoreMode::Stable
    };
    let has_notice = state.notice().is_some();
    let mut constraints = vec![
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ];
    if has_notice {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    super::shell::header(frame, chunks[0], state, core_mode);
    super::shell::navigation(frame, chunks[1], state);

    let content_area = chunks[2];

    if state.workspace() == Workspace::Inspect && state.advanced_inspect().is_some() {
        draw_advanced_inspect(frame, content_area, state);
    } else if state.backup_create_choice().is_some() {
        draw_backup_create_choice(frame, content_area, state);
    } else if state.backup_delete().is_some() {
        draw_backup_delete(frame, content_area, state);
    } else if state.backup_batch_delete().is_some() {
        draw_backup_batch_delete(frame, content_area, state);
    } else if state.backup_prune().is_some() {
        draw_backup_prune(frame, content_area, state);
    } else if state.wizard().is_some() {
        draw_wizard(frame, content_area, state);
    } else {
        match state.input_mode() {
            InputMode::Command => {
                draw_command_palette(frame, content_area, state);
            }
            InputMode::Help => {
                let mut help_lines = vec![Line::from(Span::styled("Vim 键位", accent()))];
                help_lines.extend(
                    super::keymap::NORMAL_HELP
                        .iter()
                        .map(|binding| Line::from(format!("{}  {}", binding.keys, binding.label))),
                );
                help_lines.push(Line::from(
                    "顶层标签：设备 ↔ 备份 · 一级 Tab/Shift-Tab 或 gt/gT 切换 · 二级 Tab/Shift-Tab 切当前页焦点 · Esc 返回上一层",
                ));
                help_lines.push(Line::from(
                    "设备: Enter 从列表进入信息树/从树进入详情 · Ctrl-w 切 Pane · 树内 j/k 选择、gg/G 首尾、o 展开 · 详情内 j/k 滚动、gg/G 顶底 · p 制盘 · i 检查 · b 备份 · 备份页: Enter/i 检查 · v 校验 · R 恢复 · d 删除",
                ));
                help_lines.push(Line::from(
                    "检查: / 搜索 · n/N 匹配 · gl 跳转 · Sector 0/$、gg/G、v",
                ));
                help_lines.push(Line::from(
                    "制盘: Normal 下 i 编辑、Enter 生成计划；Insert 下 Tab/Shift-Tab 完成编辑并移焦点，Enter/Esc 完成编辑；物理写盘保持精确输入 YES 的安全确认",
                ));
                let help = Paragraph::new(help_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("帮助")
                            .title_style(secondary()),
                    )
                    .wrap(Wrap { trim: true });
                frame.render_widget(help, content_area);
            }
            _ => match state.workspace() {
                Workspace::Devices => draw_devices(frame, content_area, state),
                Workspace::Inspect => frame.render_widget(
                    Paragraph::new("检查：请在设备或备份页选定对象后按 i 进入。")
                        .block(super::ui::panel("检查", true)),
                    content_area,
                ),
                Workspace::Backups => draw_backups(frame, content_area, state),
                Workspace::Provision => draw_provision(frame, content_area, state),
            },
        }
    }

    if state.provision_scheme_picker_open() {
        draw_scheme_picker(frame, content_area, state);
    }

    let status = if state.is_critical_operation() && state.backup_delete().is_some() {
        "备份删除正在执行：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.is_critical_operation() && state.backup_batch_delete().is_some() {
        "批量备份删除正在执行：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.is_critical_operation() && state.backup_prune().is_some() {
        "备份清理正在执行：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.is_critical_operation() {
        "关键写盘阶段：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.provision_scheme_picker_open() {
        "制盘方案：j/k 或 ↑/↓ 选择 · Enter 确认进入制盘表单 · Esc 取消".to_string()
    } else if let Some(advanced) = state.advanced_inspect() {
        use super::state::AdvancedInspectStage;
        match advanced.stage {
            AdvancedInspectStage::Running => "全盘检查后台只读建立结构树…".to_string(),
            AdvancedInspectStage::Browser => {
                let escape = state
                    .advanced_inspect_breadcrumb()
                    .map(|model| model.escape_hint())
                    .unwrap_or_else(|| "Esc 返回".into());
                if let Some((query, index, total)) = state.advanced_inspect_search_status() {
                    format!(
                        "检查：1/2/3/4 业务/原始/Hex/布局 · Tab/Shift-Tab 切 Pane · Ctrl-w h/j/k/l Pane · j/k 当前 Pane · o 展开/折叠 · Enter 查看 · {escape} · q 退出 · 当前 {index}/{total}: {}",
                        safe(query)
                    )
                } else {
                    format!("检查：1/2/3/4 业务/原始/Hex/布局 · Tab/Shift-Tab 切 Pane · Ctrl-w h/j/k/l Pane · j/k 当前 Pane · o 展开/折叠 · Enter 查看 · {escape} · q 退出")
                }
            }
        }
    } else if state.input_mode() == InputMode::Search {
        format!(
            "/{}  ·  输入即过滤  ·  Enter 确认  ·  Esc 取消编辑",
            safe(state.input_buffer())
        )
    } else if state.input_mode() == InputMode::Command {
        format!(
            ":{}  ·  Enter 执行  ·  Backspace 删除  ·  Esc 取消",
            safe(state.input_buffer())
        )
    } else if state.input_mode() == InputMode::Help {
        "Esc 返回  ·  q 退出".to_string()
    } else if state.wizard().is_some() {
        match state.wizard().unwrap().stage {
            WizardStage::Review => "Enter 继续 · o 详情 · Esc 返回".to_string(),
            WizardStage::Confirm => "输入 YES · Backspace 删除 · Enter 执行 · Esc 返回".to_string(),
            WizardStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            WizardStage::PostRestore => "j/k 选择 · Enter 处理 · o 详情 · Esc 完成".to_string(),
            WizardStage::VolumeLabelInput => {
                "输入卷标 · Backspace 删除 · Enter 继续 · Esc 返回".to_string()
            }
            WizardStage::PasswordInput => "输入原密码 · Enter 继续 · Esc 返回".to_string(),
            WizardStage::EncryptedFormatConfirm => {
                "输入 YES · Enter 加密格式化 · Esc 返回".to_string()
            }
            WizardStage::FormatConfirm => {
                "输入 YES · Backspace 删除 · Enter 格式化 · Esc 返回".to_string()
            }
            WizardStage::Formatting => "q / Ctrl-C 延迟退出".to_string(),
            WizardStage::ReinitializePassword => "输入新密码 · Enter 下一步 · Esc 返回".to_string(),
            WizardStage::ReinitializePasswordConfirm => {
                "再次输入新密码 · Enter 校验 · Esc 返回".to_string()
            }
            WizardStage::ReinitializeConfirm => {
                "输入 YES · Enter 重建密钥域 · Esc 返回".to_string()
            }
            WizardStage::Reinitializing => "q / Ctrl-C 延迟退出".to_string(),
            WizardStage::Result => "Enter / Esc 关闭".to_string(),
        }
    } else if let Some(delete) = state.backup_delete() {
        match delete.stage {
            WizardStage::Confirm => "输入 YES · Backspace 删除 · Enter 删除 · Esc 取消".to_string(),
            WizardStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            WizardStage::Result => "Enter / Esc 关闭".to_string(),
            _ => "Esc 返回".to_string(),
        }
    } else if let Some(batch) = state.backup_batch_delete() {
        use super::state::BackupBatchDeleteStage;
        match batch.stage {
            BackupBatchDeleteStage::Planning => "正在生成删除计划…".to_string(),
            BackupBatchDeleteStage::Review => "Enter 确认 · Esc 取消".to_string(),
            BackupBatchDeleteStage::Confirm => {
                "输入 YES · Backspace 删除 · Enter 执行 · Esc 返回".to_string()
            }
            BackupBatchDeleteStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            BackupBatchDeleteStage::Result => "Enter / Esc 关闭".to_string(),
        }
    } else if let Some(prune) = state.backup_prune() {
        use super::state::BackupPruneStage;
        match prune.stage {
            BackupPruneStage::Input => {
                "输入保留份数 · Backspace 删除 · Enter 预览 · Esc 取消".to_string()
            }
            BackupPruneStage::Planning => "正在生成清理计划…".to_string(),
            BackupPruneStage::Review => "Enter 确认 · Esc 取消".to_string(),
            BackupPruneStage::Confirm => {
                "输入 YES · Backspace 删除 · Enter 执行 · Esc 返回".to_string()
            }
            BackupPruneStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            BackupPruneStage::Result => "Enter / Esc 关闭".to_string(),
        }
    } else {
        match state.workspace() {
            Workspace::Devices => "? 帮助".to_string(),
            Workspace::Inspect => {
                if state.active_table_kind()
                    == Some(crate::tui::table_layout::TableKind::InspectFields)
                {
                    let kind = crate::tui::table_layout::TableKind::InspectFields;
                    let total = crate::tui::table_layout::layout_for(kind).specs().len();
                    format!(
                        "检查字段表：Tab/Shift-Tab 切 Pane · j/k 行 · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认排序 · {}/{} 列 · o 展开/折叠 · Enter 查看 · Esc 返回 · q 退出",
                        state.table_active_column(kind) + 1,
                        total
                    )
                } else {
                    "检查：Tab/Shift-Tab 切 Pane · j/k 当前 Pane · Ctrl-w 切 Pane · o 展开/折叠 · Enter 查看 · Esc 返回 · q 退出"
                        .to_string()
                }
            }
            Workspace::Backups => {
                if state.selected_backup().is_some() {
                    let kind = crate::tui::table_layout::TableKind::Backups;
                    let total = crate::tui::table_layout::layout_for(kind).specs().len();
                    format!(
                        "Tab/Shift-Tab 或 gt/gT 标签 · j/k 行 · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认排序 · {}/{} 列 · Space 勾选 · Enter/i 检查 · b 新建 · v 校验 · R 恢复 · d 删除 · Esc 当前标签 · q 退出",
                        state.table_active_column(kind) + 1,
                        total
                    )
                } else {
                    "Tab/Shift-Tab 或 gt/gT 标签 · b 新建 · r 刷新 · Esc 当前标签 · q 退出"
                        .to_string()
                }
            }
            Workspace::Provision => {
                let provision_status = match state.provision().stage {
                ProvisionStage::SelectDisk => {
                    let kind = crate::tui::table_layout::TableKind::ProvisionDevices;
                    let total = crate::tui::table_layout::layout_for(kind).specs().len();
                    format!(
                        "制盘选盘：j/k 行 · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认排序 · {}/{} 列 · Enter 固定目标 · Esc 返回设备页",
                        state.table_active_column(kind) + 1,
                        total
                    )
                }
                ProvisionStage::Form if state.input_mode() == InputMode::Insert => {
                    "INSERT · ←/→ 光标 · Home/End 首尾 · 输入/Backspace 编辑 · Tab/Shift-Tab 完成并移焦点 · Enter/Esc 完成编辑"
                        .to_string()
                }
                ProvisionStage::Form if state.provision_selected_field_is_editable() => {
                    let unit_key = if state
                        .provision_field_hint(state.provision().field_selected)
                        .is_some_and(|hint| hint.starts_with("Space 切换 MiB / GiB / sector"))
                    {
                        " · Space 单位 · f 填满"
                    } else {
                        ""
                    };
                    format!("NORMAL · Tab/Shift-Tab 字段/布局焦点 · j/k 字段 · i 编辑{unit_key} · Enter 生成计划 · Esc 返回")
                }
                ProvisionStage::Form => {
                    "NORMAL · Tab/Shift-Tab 字段/布局焦点 · j/k 字段 · i 编辑 · h/l 或 Space 切换 · Enter 生成计划 · Esc 返回"
                        .to_string()
                }
                ProvisionStage::Planning => "正在生成只读计划…".to_string(),
                ProvisionStage::Review => {
                    "Tab/Shift-Tab 切 Pane  ·  Enter 最终确认  ·  e 导出镜像  ·  Esc 返回修改".to_string()
                }
                ProvisionStage::ExportPath => {
                    "输入导出路径  ·  Enter 导出  ·  Esc 返回计划".to_string()
                }
                ProvisionStage::Exporting => "镜像正在后台导出…".to_string(),
                ProvisionStage::Confirm => "输入 YES + Enter 执行  ·  Esc 返回计划".to_string(),
                ProvisionStage::Running => {
                    "安全事务执行中；Esc 不退出，q / Ctrl-C 的退出请求延迟到安全检查点".to_string()
                }
                ProvisionStage::Result => "Enter / Esc 返回制盘中心".to_string(),
                };
                provision_status
            }
        }
    };
    let status = if state.input_mode() == InputMode::Normal
        && state.active_table_kind().is_some()
        && state.workspace() != Workspace::Devices
        && !state.is_critical_operation()
    {
        format!("y 单元格 · Y 整行 · {status}")
    } else {
        status
    };
    super::shell::footer(frame, chunks[usize::from(has_notice) + 3], &status);
    if let Some(message) = state.notice() {
        frame.render_widget(
            super::ui::notice_banner(safe(message), super::ui::BannerTone::Warning),
            chunks[3],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{hard_wrap_value, input_value_window, visible_window, wrapped_field_lines};

    #[test]
    fn hard_wrap_breaks_unspaced_values_by_terminal_display_width() {
        assert_eq!(
            hard_wrap_value("abcdefghijkl", 5),
            vec!["abcde", "fghij", "kl"]
        );
        assert_eq!(hard_wrap_value("江苏省电力", 6), vec!["江苏省", "电力"]);
    }

    #[test]
    fn wrapped_field_keeps_the_first_value_chunk_on_the_label_line() {
        let lines = wrapped_field_lines("文件  ", "abcdefghijkl", 10);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans.len(), 2);
        assert_eq!(lines[0].spans[0].content.as_ref(), "文件  ");
        assert_eq!(lines[0].spans[1].content.as_ref(), "abcd");
        assert_eq!(lines[1].spans[1].content.as_ref(), "efgh");
        assert_eq!(lines[2].spans[1].content.as_ref(), "ijkl");
    }

    #[test]
    fn active_input_window_does_not_pad_selected_background_to_cell_width() {
        assert_eq!(input_value_window("abc", 3, 12, false), "abc│");
        assert_eq!(input_value_window("secret", 6, 12, true), "••••••│");
        assert_eq!(
            input_value_window("1486288249", 10, 12, false),
            "1486288249│"
        );
        assert_eq!(
            crate::ui::disp_width(&input_value_window("1486288249", 10, 10, false)),
            10
        );
    }

    #[test]
    fn large_tables_only_build_the_rows_visible_in_the_viewport() {
        let window = visible_window(50_000, 100_000, 24);
        assert!(window.contains(&50_000));
        assert_eq!(window.len(), 21);
    }
}
