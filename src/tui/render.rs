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
#[path = "operation_progress_render.rs"]
mod operation_progress_render;
#[path = "provision/render.rs"]
mod provision_render;
#[path = "restore_confirmation_render.rs"]
mod restore_confirmation_render;
#[path = "wizard_result_render.rs"]
mod wizard_result_render;

use backups_render::{
    draw_backup_batch_delete, draw_backup_delete, draw_backup_prune, draw_backups,
};
use devices_render::draw_devices;
use inspect_render::draw_advanced_inspect;
use operation_progress_render::draw_operation_progress;
use provision_render::{draw_provision, draw_scheme_picker};
use restore_confirmation_render::draw_restore_write_confirmation;
use wizard_result_render::draw_wizard_result;

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

fn selection_marker() -> Style {
    super::theme::current().selection_marker()
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

fn device_status_style(row: &crate::disk_scan::Row) -> Style {
    if row.proto != "USB" || row.denied {
        warning()
    } else if row.probe_error.is_some() {
        danger()
    } else {
        success()
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

    if wizard.kind == WriteKind::Restore && wizard.stage == WizardStage::Confirm {
        draw_restore_write_confirmation(frame, area, state);
        return;
    }
    if wizard.kind == WriteKind::BackupCreate && wizard.stage == WizardStage::Confirm {
        super::ui::render_action_confirmation_modal(
            frame,
            area,
            super::ui::ActionConfirmationSpec {
                title: "创建元数据备份",
                headline: "开始只读元数据备份？",
                details: vec![
                    Line::from(format!("目标  disk{}", wizard.disk)),
                    Line::from("仅读取介质身份、几何、分区结构与协议元数据。"),
                    Line::from(Span::styled("不会向目标设备写入。", muted())),
                ],
                tone: super::ui::ConfirmationTone::Neutral,
            },
        );
        return;
    }
    if wizard.stage == WizardStage::Running {
        if let Some(run) = wizard.run.as_ref() {
            draw_operation_progress(frame, area, run);
        }
        return;
    }

    if wizard.stage == WizardStage::Result {
        draw_wizard_result(frame, area, wizard);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let title = if wizard.kind == WriteKind::Restore {
        "恢复向导"
    } else {
        "备份向导"
    };

    let breadcrumb = match wizard.stage {
        WizardStage::Confirm => {
            if wizard.kind == WriteKind::Restore {
                "备份 > 恢复 > 写入确认"
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
        WizardStage::Confirm => {
            if wizard.kind == WriteKind::Restore {
                lines.push(Line::from(Span::styled(
                    format!("恢复写入已准备 · disk{}", wizard.disk),
                    warning().add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(
                    "当前分区结构将被备份中的结构替换；文件系统、目录和用户文件不会恢复。",
                ));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("创建 disk{} 的元数据备份", wizard.disk),
                    accent().add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from("只读采集设备元数据，不会向目标 U 盘写入。"));
            }
        }
        WizardStage::Running => unreachable!("running wizard uses shared operation renderer"),
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
                    let line_style = super::theme::current().apply_selection(
                        Style::default(),
                        index == wizard.post_restore_selected,
                        true,
                    );
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
        WizardStage::Result => unreachable!("result wizard uses shared result renderer"),
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
                super::theme::current().modal_border()
            },
        )
        .title(title);
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );

    match wizard.stage {
        WizardStage::FormatConfirm => {
            let partition = wizard
                .pending_format
                .as_ref()
                .map(|request| request.partition_index);
            let target = partition
                .map(|index| format!("disk{} · 分区 {}", wizard.disk, index))
                .unwrap_or_else(|| format!("disk{}", wizard.disk));
            super::ui::render_write_confirmation_modal(
                frame,
                area,
                super::ui::WriteConfirmationSpec {
                    kind: super::ui::MediaWriteConfirmationKind::Format,
                    title: "格式化写入确认",
                    warning: format!("确认后将直接开始向 {target} 写入"),
                    details: vec![
                        Line::from(vec![
                            Span::styled("目标  ", muted()),
                            Span::styled(target, secondary()),
                        ]),
                        Line::from("创建新的空文件系统。"),
                        Line::from("该分区原有文件系统内容不会被恢复。"),
                    ],
                    confirmation: &wizard.confirmation,
                    message: wizard.message.as_deref(),
                },
            );
        }
        WizardStage::EncryptedFormatConfirm => {
            let partition = wizard
                .pending_format
                .as_ref()
                .map(|request| request.partition_index);
            let target = partition
                .map(|index| format!("disk{} · 加密分区 {}", wizard.disk, index))
                .unwrap_or_else(|| format!("disk{}", wizard.disk));
            super::ui::render_write_confirmation_modal(
                frame,
                area,
                super::ui::WriteConfirmationSpec {
                    kind: super::ui::MediaWriteConfirmationKind::EncryptedFormat,
                    title: "加密格式化写入确认",
                    warning: format!("确认后将直接开始向 {target} 写入"),
                    details: vec![
                        Line::from(vec![
                            Span::styled("目标  ", muted()),
                            Span::styled(target, secondary()),
                        ]),
                        Line::from("使用已验证原 FileKey 创建新的空加密文件系统。"),
                        Line::from("不生成新 FileKey，不修改原密码或密钥记录。"),
                        Line::from("原文件系统内容不会恢复。"),
                    ],
                    confirmation: &wizard.confirmation,
                    message: wizard.message.as_deref(),
                },
            );
        }
        WizardStage::ReinitializeConfirm => {
            let partition = wizard
                .pending_format
                .as_ref()
                .map(|request| request.partition_index);
            let target = partition
                .map(|index| format!("disk{} · 加密分区 {}", wizard.disk, index))
                .unwrap_or_else(|| format!("disk{}", wizard.disk));
            super::ui::render_write_confirmation_modal(
                frame,
                area,
                super::ui::WriteConfirmationSpec {
                    kind: super::ui::MediaWriteConfirmationKind::Reinitialize,
                    title: "加密分区重建写入确认",
                    warning: format!("确认后将直接开始向 {target} 写入"),
                    details: vec![
                        Line::from(vec![
                            Span::styled("目标  ", muted()),
                            Span::styled(target, secondary()),
                        ]),
                        Line::from("生成新 FileKey、更新密钥域并创建空加密文件系统。"),
                        Line::from(Span::styled("旧 FileKey 与旧密码将永久失效。", danger())),
                        Line::from("该操作不能恢复旧密钥域中的文件内容。"),
                    ],
                    confirmation: &wizard.confirmation,
                    message: wizard.message.as_deref(),
                },
            );
        }
        _ => {}
    }
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

    let notice = state.notice();
    let status = super::status::dynamic_status(state);
    let constraints = vec![
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    super::shell::header(frame, chunks[0], state, core_mode);
    super::shell::navigation(frame, chunks[1], state);
    let content_area = chunks[2];
    let write_confirmation_open = state.wizard().is_some_and(|wizard| {
        matches!(wizard.kind, WriteKind::Restore | WriteKind::BackupCreate)
            && wizard.stage == WizardStage::Confirm
    });

    if state.workspace() == Workspace::Inspect && state.advanced_inspect().is_some() {
        draw_advanced_inspect(frame, content_area, state);
    } else if state.backup_delete().is_some() {
        draw_backup_delete(frame, content_area, state);
    } else if state.backup_batch_delete().is_some() {
        draw_backup_batch_delete(frame, content_area, state);
    } else if state.backup_prune().is_some() {
        draw_backup_prune(frame, content_area, state);
    } else if state.wizard().is_some() && !write_confirmation_open {
        draw_wizard(frame, content_area, state);
    } else if state.input_mode() == InputMode::Command {
        draw_command_palette(frame, content_area, state);
    } else {
        match state.workspace() {
            Workspace::Devices => draw_devices(frame, content_area, state),
            Workspace::Inspect => frame.render_widget(
                Paragraph::new("检查：请在设备或备份页选定对象后按 i 进入。")
                    .block(super::ui::panel("检查", true)),
                content_area,
            ),
            Workspace::Backups => draw_backups(frame, content_area, state),
            Workspace::Provision => draw_provision(frame, content_area, state),
        }
    }

    if state.provision_scheme_picker_open() {
        draw_scheme_picker(frame, content_area, state);
    }
    if state.help_open() {
        super::help_overlay::draw_help_overlay(frame, content_area, state);
    }
    if write_confirmation_open {
        draw_wizard(frame, content_area, state);
    }

    super::shell::message_bar(frame, chunks[3], notice, status.as_deref());
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
