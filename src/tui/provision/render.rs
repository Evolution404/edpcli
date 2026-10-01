use super::*;
#[path = "scheme_picker_render.rs"]
mod scheme_picker_render;

pub(super) fn draw_scheme_picker(frame: &mut Frame, state: &AppState) {
    scheme_picker_render::draw_scheme_picker(frame, state);
}

#[path = "confirmation_render.rs"]
mod confirmation_render;
use confirmation_render::provision_confirmation_details;

#[path = "form_render.rs"]
mod form_render;
use form_render::draw_provision_form;

#[path = "review_layout_render.rs"]
mod review_layout_render;
#[path = "review_plan_render.rs"]
mod review_plan_render;
#[path = "review_render.rs"]
mod review_render;
#[path = "review_render_style.rs"]
mod review_render_style;
#[path = "review_summary_render.rs"]
mod review_summary_render;
#[path = "review_target_render.rs"]
mod review_target_render;
use review_render::draw_provision_review;

#[path = "result_render.rs"]
mod result_render;
use result_render::draw_provision_result;

fn provision_content_layout(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    area
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
    let main_area = provision_content_layout(sections[2]);

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
            let view = state.provision_confirmation_view_model().ok();
            let details = view
                .as_ref()
                .map(provision_confirmation_details)
                .unwrap_or_else(|| {
                    vec![
                        Line::from(vec![
                            Span::styled("目标设备  ", muted()),
                            Span::styled("目标身份不可用", danger()),
                        ]),
                        Line::from(vec![
                            Span::styled("写入影响  ", muted()),
                            Span::styled("无法计算", danger()),
                        ]),
                    ]
                });
            crate::tui::ui::render_write_confirmation_modal(
                frame,
                crate::tui::ui::WriteConfirmationSpec {
                    kind: crate::tui::ui::MediaWriteConfirmationKind::Provision,
                    title: "制盘写入确认",
                    warning: "写入开始后不能撤销".into(),
                    details,
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
