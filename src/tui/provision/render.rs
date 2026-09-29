use super::*;
#[path = "selection_render.rs"]
mod selection_render;
use selection_render::draw_provision_selection;

#[path = "scheme_picker_render.rs"]
mod scheme_picker_render;

pub(super) fn draw_scheme_picker(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    scheme_picker_render::draw_scheme_picker(frame, area, state);
}

#[path = "form_render.rs"]
mod form_render;
use form_render::draw_provision_form;

#[path = "review_render.rs"]
mod review_render;
use review_render::draw_provision_review;

#[path = "running_render.rs"]
mod running_render;
use running_render::draw_provision_running;

fn provision_content_layout(
    area: ratatui::layout::Rect,
    stage: ProvisionStage,
) -> (
    ratatui::layout::Rect,
    Option<(ratatui::layout::Rect, Option<ratatui::layout::Rect>)>,
) {
    if stage == ProvisionStage::Running {
        return (area, None);
    }
    let class = crate::tui::ui::ViewportClass::for_width(area.width);
    if !matches!(
        class,
        crate::tui::ui::ViewportClass::Wide | crate::tui::ui::ViewportClass::UltraWide
    ) || area.height < 12
    {
        return (area, None);
    }
    let columns = Layout::horizontal([Constraint::Min(68), Constraint::Length(40)]).split(area);
    if columns[1].height >= 18 {
        let context =
            Layout::vertical([Constraint::Min(8), Constraint::Length(10)]).split(columns[1]);
        (columns[0], Some((context[0], Some(context[1]))))
    } else {
        (columns[0], Some((columns[1], None)))
    }
}

fn draw_provision_stepper(frame: &mut Frame, area: ratatui::layout::Rect, stage: ProvisionStage) {
    let current = match stage {
        ProvisionStage::SelectDisk => 0,
        ProvisionStage::Form => 1,
        ProvisionStage::Planning => 2,
        ProvisionStage::Review
        | ProvisionStage::ExportPath
        | ProvisionStage::Exporting
        | ProvisionStage::Confirm => 3,
        ProvisionStage::Running => 4,
        ProvisionStage::Result => 5,
    };
    let names = [
        "选择设备",
        "制盘配置",
        "分区预览",
        "计划确认",
        "执行",
        "完成",
    ];
    let class = crate::tui::ui::ViewportClass::for_width(area.width);
    let line = if class == crate::tui::ui::ViewportClass::Compact {
        Line::from(format!("{}/6  {}", current + 1, names[current]))
    } else {
        Line::from(
            names
                .into_iter()
                .enumerate()
                .flat_map(|(index, name)| {
                    let tone = if index == current {
                        accent()
                    } else if index < current {
                        success()
                    } else {
                        muted()
                    };
                    let mut spans = Vec::new();
                    if index > 0 {
                        spans.push(Span::styled(" ─ ", muted()));
                    }
                    spans.push(Span::styled(format!("{} {name}", index + 1), tone));
                    spans
                })
                .collect::<Vec<_>>(),
        )
    };
    frame.render_widget(Paragraph::new(line), area);
}

pub(super) fn draw_provision(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let provision = state.provision();
    let sections = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
    draw_provision_stepper(frame, sections[0], provision.stage);
    let (main_area, sidebar) = provision_content_layout(sections[1], provision.stage);

    let target_lines = if let Some(row) = if provision.stage == ProvisionStage::SelectDisk {
        state.provision_device_at(state.selected())
    } else {
        state.selected_device()
    } {
        vec![
            Line::from(vec![
                Span::styled(format!("disk{}", row.disk), accent()),
                Span::raw(format!("  {}", crate::common::fmt_capacity(row.size))),
            ]),
            Line::from(vec![
                Span::styled("接口  ", muted()),
                Span::styled(safe(&row.proto), secondary()),
                Span::raw("   "),
                Span::styled(format!("{}:{}", safe(&row.vid), safe(&row.pid)), muted()),
            ]),
            Line::from(vec![
                Span::styled("盘型  ", muted()),
                Span::styled(
                    row.confirmed_provision_kind()
                        .map(|kind| kind.full_name())
                        .unwrap_or("未知 / 未确认"),
                    device_status_style(row),
                ),
            ]),
            Line::from(vec![
                Span::styled("标签  ", muted()),
                Span::raw(safe(row.onlyid.as_deref().unwrap_or("未读取"))),
            ]),
            Line::from(vec![
                Span::styled("用户  ", muted()),
                Span::raw(safe(row.user.as_deref().unwrap_or("未读取"))),
            ]),
        ]
    } else {
        vec![
            Line::from(Span::styled("未固定目标 USB", danger())),
            Line::from("请在左侧列表选择可用 USB 目标盘。"),
        ]
    };

    if let Some((side_top, side_bottom)) = sidebar {
        frame.render_widget(
            Paragraph::new(target_lines)
                .block(crate::tui::ui::card("固定目标", false))
                .wrap(Wrap { trim: true }),
            side_top,
        );
        if let Some(side_bottom) = side_bottom {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("安全不变量", warning())),
                    Line::from("• 仅允许 USB 整盘目标"),
                    Line::from("• LBA3 厂商数据原样保留"),
                    Line::from("• 写前固定硬件身份/容量"),
                    Line::from("• MBR 最后提交"),
                    Line::from("• 协议写入失败回滚；格式化失败保留制盘"),
                    Line::from("• 保留分区保持原位置与密钥材料"),
                ])
                .block(crate::tui::ui::card("写盘保护", false))
                .wrap(Wrap { trim: true }),
                side_bottom,
            );
        }
    }

    match provision.stage {
        ProvisionStage::SelectDisk => {
            draw_provision_selection(frame, main_area, state);
        }
        ProvisionStage::Form => {
            draw_provision_form(frame, main_area, state);
        }
        ProvisionStage::Planning => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈  正在生成精确计划", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在只读检查目标盘…"),
                    )),
                    Line::from("此阶段不写盘；正在计算 LCE、分区边界与协议元数据。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(focused_panel())
                        .title("只读规划"),
                ),
                main_area,
            );
        }
        ProvisionStage::Review => {
            draw_provision_review(frame, main_area, state);
        }
        ProvisionStage::ExportPath => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("导出目标绑定制盘镜像", secondary())),
                    Line::from(""),
                    Line::from("镜像包含目标盘硬件身份和原始 LBA3，不应写入另一块不同 U 盘。"),
                    Line::from(vec![
                        Span::styled("输出路径  ", muted()),
                        Span::styled(safe(&provision.export_path), input_focused()),
                    ]),
                    Line::from(""),
                    Line::from("直接输入编辑路径 · Backspace 删除 · Enter 开始导出 · Esc 返回"),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(focused_panel())
                        .title("镜像导出"),
                )
                .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::Exporting => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈ 正在导出稀疏制盘镜像", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在写入镜像并执行 fsync…"),
                    )),
                    Line::from("导出完成前保持当前计划不变。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(focused_panel())
                        .title("镜像导出"),
                ),
                main_area,
            );
        }
        ProvisionStage::Confirm => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("破坏性写盘最终确认", danger())),
                    Line::from(""),
                    Line::from("请重新核对目标盘和计划。此操作会修改真实物理介质。"),
                    Line::from(vec![
                        Span::raw("精确输入 "),
                        Span::styled("YES", danger()),
                        Span::raw(" 后按 Enter： "),
                        Span::styled(safe(&provision.confirmation), input_focused()),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled("Esc 返回计划页，不会写盘。", warning())),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(danger())
                        .title("最终确认"),
                )
                .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::Running => {
            draw_provision_running(frame, main_area, state);
        }
        ProvisionStage::Result => {
            use crate::application::provision::ProvisionExecutionStatus as Status;
            let result_style = match provision.result_status {
                Some(Status::Success) => success(),
                Some(Status::CompletedWithWarnings | Status::PartialFormatFailure) => warning(),
                Some(Status::FatalFailure) | None => danger(),
            };
            let result_title = match provision.result_status {
                Some(Status::Success) => "制盘成功",
                Some(Status::CompletedWithWarnings) => "制盘完成，存在警告",
                Some(Status::PartialFormatFailure) => "部分完成：格式化失败",
                Some(Status::FatalFailure) | None => "制盘失败",
            };
            let badge_tone = match provision.result_status {
                Some(Status::Success) => crate::tui::ui::BadgeTone::Success,
                Some(Status::CompletedWithWarnings | Status::PartialFormatFailure) => {
                    crate::tui::ui::BadgeTone::Warning
                }
                Some(Status::FatalFailure) | None => crate::tui::ui::BadgeTone::Danger,
            };
            let mut lines = vec![
                crate::tui::ui::status_badge(result_title, badge_tone),
                Line::from(""),
            ];
            lines.extend(
                provision
                    .message
                    .as_deref()
                    .unwrap_or("操作结束")
                    .lines()
                    .map(|line| Line::from(safe(line))),
            );
            if let Some(run) = &provision.run {
                let elapsed = run
                    .last_activity_at
                    .duration_since(run.started_at)
                    .as_secs();
                lines.push(Line::from(format!("总耗时  {elapsed} 秒")));
                let mut phases = std::collections::BTreeMap::new();
                for event in &run.log {
                    phases
                        .entry(event.phase)
                        .and_modify(|last: &mut (std::time::Instant, std::time::Instant)| {
                            last.1 = event.emitted_at
                        })
                        .or_insert((event.emitted_at, event.emitted_at));
                }
                for (phase, (first, last)) in phases {
                    lines.push(Line::from(format!(
                        "{}  {} 秒",
                        phase.label(),
                        last.duration_since(first).as_secs()
                    )));
                }
                lines.push(Line::from(Span::styled("最近进度事件", accent())));
                for event in run.log.iter().rev().take(6).rev() {
                    lines.push(Line::from(safe(&format!(
                        "[{}/{}] {}  {}",
                        event.current,
                        event.total,
                        event.phase.label(),
                        event.step.label()
                    ))));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Enter / Esc 返回制盘中心",
                accent(),
            )));
            frame.render_widget(
                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .block(crate::tui::ui::card("制盘结果", true).border_style(result_style)),
                main_area,
            );
        }
    }
}
