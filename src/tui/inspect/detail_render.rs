use super::*;
use crate::application::inspect::AdvancedInspectWorkspace;
use crate::application::inspect_tree::InspectNodeKind;
use crate::tui::state::{
    AdvancedInspectPanel, AdvancedInspectPrompt, AdvancedInspectState, AdvancedInspectTreeRow,
};

pub(super) fn draw_inspect_object_panes(
    frame: &mut Frame,
    state: &AppState,
    workspace: &AdvancedInspectWorkspace,
    advanced: &AdvancedInspectState,
    selected_row: Option<&AdvancedInspectTreeRow>,
    overview_area: Option<ratatui::layout::Rect>,
    detail_area: Option<ratatui::layout::Rect>,
) {
    let mut overview_lines = Vec::new();
    let mut detail_lines = Vec::new();
    if let Some(row) = selected_row {
        let item = matches!(row.kind, InspectNodeKind::Sector | InspectNodeKind::Field)
            .then(|| {
                workspace
                    .items
                    .iter()
                    .find(|item| item.lba == row.range.start_lba)
            })
            .flatten();
        let fields: &[crate::application::inspect::InspectField] = match row.kind {
            InspectNodeKind::Sector => item.map_or(&[], |item| item.fields.as_slice()),
            InspectNodeKind::Field => item
                .and_then(|item| {
                    item.fields.iter().find(|field| {
                        row.range
                            .byte_range
                            .is_some_and(|range| range == field.range)
                    })
                })
                .map_or(&[], std::slice::from_ref),
            _ => &[],
        };
        let summary = crate::application::inspect_summary::summarize_node(
            crate::application::inspect_summary::InspectSummarySource {
                kind: row.kind,
                label: &row.label,
                range: row.range,
                decoder: row.decoder,
                status: row.status,
                region_semantic: row.region_semantic,
                fields,
                parse_state: item.map_or(
                    crate::application::inspect::InspectParseState::Parsed,
                    |item| item.parse_state,
                ),
                diagnostics: item.map_or(&[], |item| item.diagnostics.as_slice()),
            },
        );
        overview_lines.push(Line::from(Span::styled(
            safe(&summary.title),
            secondary().add_modifier(Modifier::BOLD),
        )));
        overview_lines.push(Line::from(Span::styled(
            safe(&summary.subtitle),
            if summary.alerts.is_empty() {
                muted()
            } else {
                warning()
            },
        )));
        overview_lines.push(Line::from(Span::styled(safe(&summary.location), muted())));
        for section in &summary.sections {
            overview_lines.push(Line::from(""));
            overview_lines.push(Line::from(Span::styled(safe(&section.title), accent())));
            let label_width = section
                .items
                .iter()
                .map(|item| crate::tui::table_layout::display_width(&item.label))
                .max()
                .unwrap_or(0);
            for item in &section.items {
                let padding = label_width
                    .saturating_sub(crate::tui::table_layout::display_width(&item.label))
                    + 2;
                overview_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}{}", safe(&item.label), " ".repeat(padding)),
                        muted(),
                    ),
                    Span::raw(safe(&item.value)),
                ]));
            }
        }
        for alert in &summary.alerts {
            overview_lines.push(Line::from(Span::styled(safe(&alert.message), warning())));
        }
        overview_lines.push(Line::from(vec![
            Span::styled("来源  ", muted()),
            Span::styled(safe(&workspace.source), muted()),
        ]));

        match row.kind {
            InspectNodeKind::Sector => {
                let lba = row.range.start_lba;
                if let Some(item) = workspace.items.iter().find(|item| item.lba == lba) {
                    if !item.fields.is_empty() {
                        let mut previous_group: Option<&str> = None;
                        for field in &item.fields {
                            let group = field.group.as_deref();
                            if group != previous_group {
                                if let Some(group) = group {
                                    detail_lines.push(Line::from(Span::styled(
                                        safe(group),
                                        accent().add_modifier(Modifier::BOLD),
                                    )));
                                }
                                previous_group = group;
                            }
                            detail_lines.push(Line::from(format!(
                                "{}: {}",
                                safe(&field.label),
                                safe(&field.value)
                            )));
                            for child in &field.children {
                                detail_lines.push(Line::from(format!(
                                    "  {}: {}",
                                    safe(&child.label),
                                    safe(&child.value)
                                )));
                            }
                        }
                    } else if let Some(meta_text) = &item.meta_text {
                        detail_lines.extend(meta_text.lines().map(|line| Line::from(safe(line))));
                    } else {
                        detail_lines.push(Line::from(format!(
                            "RAW SHA-256: {}",
                            safe(&item.raw_sha256)
                        )));
                    }
                    for note in &item.notes {
                        detail_lines.push(Line::from(safe(note)));
                    }
                    detail_lines.push(Line::from("Enter 打开扇区检查"));
                } else {
                    match state.advanced_inspect_preview_state(lba) {
                        crate::tui::state::PreviewLoadState::Failed { message, attempts } => {
                            overview_lines.push(Line::from(Span::styled(
                                format!("读取失败（第 {attempts} 次）：{}", safe(&message)),
                                warning(),
                            )));
                            detail_lines
                                .push(Line::from("按 r 显式重试预览，或 Enter 打开扇区检查重试。"));
                        }
                        crate::tui::state::PreviewLoadState::Pending { .. } => {
                            detail_lines.push(Line::from("正在后台读取当前扇区…"));
                        }
                        _ => {
                            detail_lines
                                .push(Line::from(Span::styled("该扇区尚未按需读取。", warning())));
                            detail_lines.push(Line::from("Enter 打开扇区检查并后台读取当前扇区。"));
                        }
                    }
                }
            }
            InspectNodeKind::Field => {
                let field = row.range.byte_range.and_then(|range| {
                    workspace
                        .items
                        .iter()
                        .flat_map(|item| item.fields.iter())
                        .find(|field| field.range == range)
                });
                if let Some(field) = field {
                    detail_lines.push(Line::from(Span::styled(
                        safe(&field.label),
                        accent().add_modifier(Modifier::BOLD),
                    )));
                    detail_lines.push(Line::from(format!("Value: {}", safe(&field.value))));
                    detail_lines.push(Line::from(format!(
                        "Source LBA: {} · Group: {}",
                        field.range.start_lba(),
                        safe(field.group.as_deref().unwrap_or("—"))
                    )));
                    detail_lines.push(Line::from(format!(
                        "Offset: 0x{:X} · Length: {} B",
                        field.range.start,
                        field.range.len()
                    )));
                    let hex = |bytes: &[u8]| {
                        bytes
                            .iter()
                            .map(|byte| format!("{byte:02X}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    detail_lines.push(Line::from(format!("Raw: {}", hex(&field.raw))));
                    detail_lines.push(Line::from(format!("Decoded: {}", hex(&field.decoded))));
                    detail_lines.push(Line::from(format!(
                        "FieldLogical: {}",
                        field
                            .field_logical
                            .as_deref()
                            .map(hex)
                            .unwrap_or_else(|| "—".into())
                    )));
                    detail_lines.push(Line::from(format!(
                        "Transform: {}",
                        field
                            .transform
                            .map(|transform| format!("{transform:?}"))
                            .unwrap_or_else(|| "—".into())
                    )));
                    detail_lines.push(Line::from(format!(
                        "Type: {:?}   Status: {:?}",
                        field.field_type, field.status
                    )));
                } else {
                    detail_lines.push(Line::from("字段详情尚未 materialize。"));
                }
            }
            InspectNodeKind::Group => {
                detail_lines.push(Line::from("分页控制节点。"));
                detail_lines.push(Line::from("Enter / o 切换当前 lazy sector 窗口。"));
            }
            _ => {
                detail_lines.push(Line::from("o 展开/折叠当前节点。"));
                detail_lines.push(Line::from("Enter 查看或进入当前节点。"));
            }
        }
    } else {
        overview_lines.push(Line::from("当前没有可选节点。"));
        detail_lines.push(Line::from("当前没有可选节点。"));
    }
    if let Some(AdvancedInspectPrompt::Search { input }) = advanced.prompt.as_ref() {
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            "结构化搜索",
            accent().add_modifier(Modifier::BOLD),
        )));
        detail_lines.push(Line::from(format!("/{}", safe(input))));
        detail_lines.push(Line::from(
            "搜索 Region / Extent / Structure / Group / Field label 与 typed value",
        ));
        detail_lines.push(Line::from("Enter 定位 · Esc 取消"));
    }

    if let Some(message) = advanced.message.as_ref() {
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            format!("{} {}", message.marker(), safe(message.text())),
            message.style(),
        )));
    }

    if let Some(overview_area) = overview_area {
        let overview_scroll = state
            .pane_viewport(crate::tui::pane::PaneId::InspectOverview)
            .scroll_y
            .offset
            .min(overview_lines.len().saturating_sub(1));
        frame.render_widget(
            Paragraph::new(overview_lines)
                .block(crate::tui::ui::card(
                    "对象摘要",
                    advanced.panel == AdvancedInspectPanel::Overview,
                ))
                .scroll((overview_scroll.min(u16::MAX as usize) as u16, 0))
                .wrap(Wrap { trim: false }),
            overview_area,
        );
    }

    if let Some(detail_area) = detail_area {
        let detail_focus = advanced.panel == AdvancedInspectPanel::Detail;
        let detail_offset = state
            .pane_viewport(crate::tui::pane::PaneId::InspectDetail)
            .scroll_y
            .offset;
        let field_item = selected_row
            .filter(|row| row.kind == InspectNodeKind::Sector)
            .and_then(|row| {
                workspace
                    .items
                    .iter()
                    .find(|item| item.lba == row.range.start_lba)
            })
            .filter(|item| !item.fields.is_empty());
        if let Some(item) = field_item {
            super::field_table_render::draw_inspect_field_table(
                frame,
                state,
                item,
                detail_area,
                detail_focus,
                detail_offset,
            );
        } else {
            let detail_scroll = detail_offset.min(detail_lines.len().saturating_sub(1));
            frame.render_widget(
                Paragraph::new(detail_lines)
                    .block(crate::tui::ui::card("字段 / 证据", detail_focus))
                    .wrap(Wrap { trim: false })
                    .scroll((detail_scroll.min(u16::MAX as usize) as u16, 0)),
                detail_area,
            );
        }
    }
}
