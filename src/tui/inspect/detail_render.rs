use super::*;
use crate::application::inspect::AdvancedInspectWorkspace;
use crate::application::inspect_tree::InspectNodeKind;
use crate::tui::state::{
    AdvancedInspectPanel, AdvancedInspectPrompt, AdvancedInspectState, AdvancedInspectTreeRow,
};

fn selected_item<'a>(
    workspace: &'a AdvancedInspectWorkspace,
    row: Option<&AdvancedInspectTreeRow>,
) -> Option<&'a crate::application::inspect::AdvancedInspectItem> {
    let row = row?;
    matches!(row.kind, InspectNodeKind::Sector | InspectNodeKind::Field)
        .then(|| {
            workspace
                .items
                .iter()
                .find(|item| item.lba == row.range.start_lba)
        })
        .flatten()
}

fn summary_lines(
    state: &AppState,
    workspace: &AdvancedInspectWorkspace,
    advanced: &AdvancedInspectState,
    row: Option<&AdvancedInspectTreeRow>,
) -> Vec<Line<'static>> {
    let Some(row) = row else {
        return vec![Line::from("当前没有可选节点。")];
    };
    let item = selected_item(workspace, Some(row));
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

    let mut lines = vec![
        Line::from(Span::styled(
            safe(&summary.title),
            secondary().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            safe(&summary.subtitle),
            if summary.alerts.is_empty() {
                muted()
            } else {
                warning()
            },
        )),
        Line::from(Span::styled(safe(&summary.location), muted())),
    ];

    let mut remaining_items = 6usize;
    for section in &summary.sections {
        if remaining_items == 0 || section.items.is_empty() {
            break;
        }
        lines.push(Line::from(Span::styled(
            safe(&section.title),
            secondary().add_modifier(Modifier::BOLD),
        )));
        let label_width = section
            .items
            .iter()
            .map(|item| crate::tui::table_layout::display_width(&item.label))
            .max()
            .unwrap_or(0);
        for value in section.items.iter().take(remaining_items.min(3)) {
            let padding = label_width
                .saturating_sub(crate::tui::table_layout::display_width(&value.label))
                + 2;
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}{}", safe(&value.label), " ".repeat(padding)),
                    muted(),
                ),
                Span::raw(safe(&value.value)),
            ]));
            remaining_items = remaining_items.saturating_sub(1);
            if remaining_items == 0 {
                break;
            }
        }
    }

    if row.kind == InspectNodeKind::Sector && item.is_none() {
        match state.advanced_inspect_preview_state(row.range.start_lba) {
            crate::tui::state::PreviewLoadState::Failed { message, attempts } => {
                lines.push(Line::from(Span::styled(
                    format!("读取失败（第 {attempts} 次）· {}", safe(&message)),
                    warning(),
                )));
            }
            crate::tui::state::PreviewLoadState::Pending { .. } => {
                lines.push(Line::from(Span::styled("正在后台读取当前扇区…", muted())));
            }
            _ => {
                lines.push(Line::from(Span::styled(
                    "当前扇区尚未按需读取；Enter 打开全屏 Hex。",
                    muted(),
                )));
            }
        }
    }

    for alert in summary.alerts.iter().take(2) {
        lines.push(Line::from(Span::styled(safe(&alert.message), warning())));
    }
    if let Some(AdvancedInspectPrompt::Search { input }) = advanced.prompt.as_ref() {
        lines.push(Line::from(vec![
            Span::styled("搜索  ", muted()),
            Span::raw(format!("/{}", safe(input))),
        ]));
    }
    if let Some(message) = advanced.message.as_ref() {
        lines.push(Line::from(Span::styled(
            format!("{} {}", message.marker(), safe(message.text())),
            message.style(),
        )));
    }
    lines.push(Line::from(vec![
        Span::styled("来源  ", muted()),
        Span::styled(safe(&workspace.source), muted()),
    ]));
    lines
}

pub(super) fn draw_inspect_object_panes(
    frame: &mut Frame,
    state: &AppState,
    workspace: &AdvancedInspectWorkspace,
    advanced: &AdvancedInspectState,
    selected_row: Option<&AdvancedInspectTreeRow>,
    overview_area: Option<ratatui::layout::Rect>,
    detail_area: Option<ratatui::layout::Rect>,
) {
    if let Some(overview_area) = overview_area {
        let overview_lines = summary_lines(state, workspace, advanced, selected_row);
        let visible_rows = usize::from(overview_area.height.saturating_sub(2)).max(1);
        let overview_scroll = state
            .pane_viewport(crate::tui::pane::PaneId::InspectOverview)
            .scroll_y
            .offset
            .min(overview_lines.len().saturating_sub(visible_rows));
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

    let Some(detail_area) = detail_area else {
        return;
    };
    let detail_focus = advanced.panel == AdvancedInspectPanel::Detail;
    let detail_offset = state
        .pane_viewport(crate::tui::pane::PaneId::InspectDetail)
        .scroll_y
        .offset;
    let item = selected_item(workspace, selected_row);
    let field_item = selected_row
        .filter(|row| row.kind == InspectNodeKind::Sector)
        .and(item)
        .filter(|item| !item.fields.is_empty());

    if let Some(item) = field_item {
        super::field_table_render::draw_inspect_field_table(
            frame,
            state,
            item,
            detail_area,
            detail_focus,
        );
    } else {
        super::evidence_table_render::draw_technical_evidence_table(
            frame,
            detail_area,
            workspace,
            selected_row,
            item,
            detail_focus,
            detail_offset,
        );
    }
}
