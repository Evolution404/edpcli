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
    crate::tui::ui::render_modal(frame, modal, "制盘计划 · 生成中", |frame, inner| {
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

/// TUI 4Kn审核与正式可写确认页面严格分离。摘要只读且不能被转换
/// 为 `PreparedProvision`，Enter/导出键均不会进入物理写盘流程。
fn draw_native_4kn_readonly_review(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    review: &crate::application::provision::native_preflight::Native4knReadOnlyPreflight,
) {
    let lines = vec![
        Line::from(Span::styled(
            "4Kn Mode1 · 已认证来源 · 只读计划",
            secondary(),
        )),
        Line::from(""),
        Line::from(format!(
            "目标：disk{} · {}",
            review.disk, review.device_identity
        )),
        Line::from(format!(
            "原生容量：{} 块 × {}B",
            review.total_sectors, review.logical_sector_bytes
        )),
        Line::from(format!(
            "已核对来源块：{}；计划写集：{} 个完整块",
            review.verified_original_blocks, review.write_blocks
        )),
        Line::from(format!(
            "二合一区：LBA{}，{} 块；exFAT 元数据：{} 块",
            review.first_partition_lba, review.first_partition_sectors, review.format_block_count
        )),
        Line::from(format!(
            "原保密区起点：LBA{}（保持保留）",
            review.preserved_encrypted_partition_lba
        )),
        Line::from(""),
        Line::from("EDPB SHA-256："),
        Line::from(review.source_backup_sha256.clone()),
        Line::from("写集 SHA-256："),
        Line::from(review.planned_write_sha256.clone()),
        Line::from(""),
        Line::from(Span::styled(
            "只读审核 · 未卸载、未写盘、未授权物理提交",
            danger(),
        )),
        Line::from("Enter / 导出不可执行写盘 · Esc 返回配置"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card("制盘计划 · 4Kn只读审核", true))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// Source/target choice is independent from the limited, source-authenticated
/// conversion producer. Geometry drafts are never allowed to reach commit.
fn draw_native_geometry_readonly_review(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    review: &crate::tui::state::NativeGeometryReadOnlyReview,
) {
    let mut lines = vec![
        Line::from(Span::styled(
            "任意来源 → 目标模式 · 原生几何只读审核",
            secondary(),
        )),
        Line::from(""),
        Line::from(format!(
            "disk{} · {} → {}",
            review.disk,
            review.source_kind.target().full_name(),
            review.target_kind.title()
        )),
        Line::from(format!(
            "容量：{} × {}B",
            review.total_sectors, review.sector_bytes
        )),
        Line::from(""),
        Line::from("拟建目标分区（仅几何，不代表数据保留或文件系统可挂载）："),
    ];
    for (label, start, count) in &review.partitions {
        lines.push(Line::from(format!(
            "{} · LBA {}–{} · {} 原生块",
            label,
            start,
            start.saturating_add(*count).saturating_sub(1),
            count
        )));
    }
    if let Some(lce) = review.lce_lba {
        lines.push(Line::from(format!(
            "LCE LBA {} · {}",
            lce,
            if review.lce_is_source_verified {
                "来源原生指针已观测，密码/LCE内容未在本草稿中认证"
            } else {
                "普通来源：暂定预留位置，非来源真实LCE"
            }
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "只读几何有效 ≠ 转换可执行。未认证来源密码、FileKey、文件系统、LCE写集和设备写权限。",
            danger(),
        )),
        Line::from("Enter / 导出均不可执行 · Esc 返回配置"),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card("制盘计划 · 原生只读草稿", true))
            .wrap(Wrap { trim: true }),
        area,
    );
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
            if let Some(readonly) = provision.native_readonly_review.as_ref() {
                draw_native_4kn_readonly_review(frame, main_area, readonly);
            } else if let Some(readonly) = provision.native_geometry_review.as_ref() {
                draw_native_geometry_readonly_review(frame, main_area, readonly);
            } else {
                draw_provision_review(frame, main_area, state);
            }
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
                    target: state
                        .provision_target_disk()
                        .map(|disk| state.confirmation_target(disk))
                        .unwrap_or_else(|| "未选择设备".into()),
                    detail_scroll: state.confirmation_offset(),
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
                    state.provision_run_log_start(),
                );
            }
        }
        ProvisionStage::Result => {
            draw_provision_result(frame, main_area, state);
        }
    }
}
