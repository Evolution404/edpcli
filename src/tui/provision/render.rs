use super::*;
#[path = "scheme_picker_render.rs"]
mod scheme_picker_render;

pub(super) fn draw_scheme_picker(frame: &mut Frame, state: &AppState) {
    scheme_picker_render::draw_scheme_picker(frame, state);
}

#[path = "form_render.rs"]
mod form_render;
use form_render::draw_provision_form;

#[path = "review_render.rs"]
mod review_render;
use review_render::draw_provision_review;

#[path = "result_render.rs"]
mod result_render;
use result_render::draw_provision_result;

fn provision_content_layout(
    area: ratatui::layout::Rect,
    stage: ProvisionStage,
) -> (
    ratatui::layout::Rect,
    Option<(ratatui::layout::Rect, Option<ratatui::layout::Rect>)>,
) {
    if matches!(
        stage,
        ProvisionStage::Form
            | ProvisionStage::Planning
            | ProvisionStage::Running
            | ProvisionStage::Result
    ) {
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
    (columns[0], Some((columns[1], None)))
}

fn draw_provision_breadcrumb(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let class = crate::tui::ui::ViewportClass::for_width(area.width);
    if class == crate::tui::ui::ViewportClass::Compact {
        frame.render_widget(
            Paragraph::new(safe(&state.provision_breadcrumb())).style(muted()),
            area,
        );
        return;
    }
    let parts = Layout::horizontal([Constraint::Min(0), Constraint::Length(30)]).split(area);
    frame.render_widget(
        Paragraph::new(safe(&state.provision_breadcrumb())).style(muted()),
        parts[0],
    );
    frame.render_widget(
        Paragraph::new(state.provision_escape_hint())
            .alignment(ratatui::layout::Alignment::Right)
            .style(accent()),
        parts[1],
    );
}

fn draw_provision_stepper(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let current = state.provision_step_index();
    let names = ["制盘配置", "生成计划", "计划确认", "执行", "完成"];
    let class = crate::tui::ui::ViewportClass::for_width(area.width);
    let line = if class == crate::tui::ui::ViewportClass::Compact {
        Line::from(format!("{}/5  {}", current + 1, names[current]))
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

fn draw_provision_status_modal(frame: &mut Frame, title: &str, lines: Vec<Line<'static>>) {
    let height = (lines.len() as u16).saturating_add(2).clamp(5, 10);
    let modal = crate::tui::ui::centered_modal_rect(frame.area(), 76, height);
    crate::tui::ui::render_modal(frame, modal, title, |frame, inner| {
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
    });
}

fn draw_provision_planning_modal(frame: &mut Frame, state: &AppState) {
    let modal = crate::tui::ui::centered_modal_rect(frame.area(), 36, 7);
    crate::tui::ui::render_modal(frame, modal, "", |frame, inner| {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "正在生成制盘计划",
                    accent().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    crate::tui::animation::spinner_glyph(state.animation_frame()).to_string(),
                    secondary(),
                )),
            ])
            .alignment(ratatui::layout::Alignment::Center),
            inner,
        );
    });
}

pub(super) fn draw_provision(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let provision = state.provision();
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(area);
    draw_provision_breadcrumb(frame, sections[0], state);
    draw_provision_stepper(frame, sections[1], state);
    let (main_area, sidebar) = provision_content_layout(sections[2], provision.stage);

    let target_lines = if let Some(row) = state.selected_device() {
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
                    row.confirmed_provision_kind()
                        .map(|kind| crate::tui::theme::current().provision_kind_emphasis(kind))
                        .unwrap_or_else(warning),
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

    if let Some((side_top, _)) = sidebar {
        frame.render_widget(
            Paragraph::new(target_lines)
                .block(crate::tui::ui::card("固定目标", false))
                .wrap(Wrap { trim: true }),
            side_top,
        );
    }

    match provision.stage {
        ProvisionStage::Form => {
            draw_provision_form(frame, main_area, state);
        }
        ProvisionStage::Planning => {
            draw_provision_form(frame, main_area, state);
            draw_provision_planning_modal(frame, state);
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
                .block(crate::tui::ui::card("镜像导出", true))
                .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::Exporting => {
            draw_provision_review(frame, main_area, state);
            draw_provision_status_modal(
                frame,
                "镜像导出 · 执行中",
                vec![
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在写入镜像并执行 fsync…"),
                    )),
                    Line::from("导出完成前保持当前计划不变。"),
                    Line::from(Span::styled("退出请求会等待当前导出安全结束。", muted())),
                ],
            );
        }
        ProvisionStage::Confirm => {
            draw_provision_review(frame, main_area, state);
            let target = state
                .provision_target_disk()
                .map(|disk| format!("disk{disk}"))
                .unwrap_or_else(|| "未选择目标".into());
            crate::tui::ui::render_write_confirmation_modal(
                frame,
                crate::tui::ui::WriteConfirmationSpec {
                    kind: crate::tui::ui::MediaWriteConfirmationKind::Provision,
                    title: "制盘写入确认",
                    warning: format!("确认后将直接开始向 {target} 写入"),
                    details: vec![
                        Line::from(vec![
                            Span::styled("目标设备  ", muted()),
                            Span::styled(target, secondary()),
                        ]),
                        Line::from("按已审核计划写入分区结构、文件系统与协议元数据。"),
                        Line::from(Span::styled(
                            "当前介质上的相关结构和数据可能被覆盖。",
                            warning(),
                        )),
                    ],
                    confirmation: &provision.confirmation,
                    message: provision.message.as_ref(),
                },
            );
        }
        ProvisionStage::Running => {
            if let Some(run) = provision.run.as_ref() {
                super::operation_progress_render::draw_operation_progress(
                    frame,
                    main_area,
                    run,
                    state.animation_frame(),
                );
            }
        }
        ProvisionStage::Result => {
            draw_provision_result(frame, main_area, state);
        }
    }
}
