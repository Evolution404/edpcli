use super::*;
use crate::application::inspect_tree::InspectNodeKind;
use crate::tui::state::AdvancedInspectTreeRow;

pub(super) fn draw_inspect_tree_pane(
    frame: &mut Frame,
    tree_area: ratatui::layout::Rect,
    rows: &[AdvancedInspectTreeRow],
    selected_index: usize,
    tree_focus: bool,
) {
    let visible = visible_window(selected_index, rows.len(), tree_area.height);
    let tree_lines = visible.map(|index| {
        let row = &rows[index];
        let indent = "  ".repeat(row.depth);
        let marker = if row.expandable {
            if row.expanded {
                "− "
            } else {
                "+ "
            }
        } else {
            "· "
        };
        let icon = match row.kind {
            InspectNodeKind::Device => "◆ ",
            InspectNodeKind::Region => "◇ ",
            InspectNodeKind::Extent => "▰ ",
            InspectNodeKind::Sector => "□ ",
            InspectNodeKind::Structure => "▱ ",
            InspectNodeKind::Group => "≡ ",
            InspectNodeKind::Field => "• ",
            InspectNodeKind::Partition => "▣ ",
            InspectNodeKind::UnknownRange => "? ",
        };
        let kind_style = match row.kind {
            InspectNodeKind::Device => secondary().add_modifier(Modifier::BOLD),
            InspectNodeKind::Region | InspectNodeKind::Partition => accent(),
            InspectNodeKind::Extent | InspectNodeKind::Structure => success(),
            InspectNodeKind::Sector | InspectNodeKind::Field => Style::default(),
            InspectNodeKind::Group | InspectNodeKind::UnknownRange => muted(),
        };
        let content = if row.kind == InspectNodeKind::Sector {
            format!("{marker}{icon}{}", safe(&row.label))
        } else {
            let range = crate::application::inspect_tree::format_lba_closed_range(
                row.range.start_lba,
                row.range.end_lba_exclusive(),
            )
            .unwrap_or_else(|| "[空区间]".into());
            format!("{marker}{icon}{} {range}", safe(&row.label))
        };
        let available = usize::from(tree_area.width)
            .saturating_sub(2)
            .saturating_sub(row.depth.saturating_mul(2))
            .saturating_sub(2)
            .saturating_sub(1);
        let content = crate::tui::table_layout::truncate_cell(
            &content,
            available,
            crate::tui::table_layout::TruncatePolicy::Ellipsis,
        );
        let focused = tree_focus && index == selected_index;
        Line::from(vec![
            Span::raw(indent),
            Span::styled(
                if focused { "▌ " } else { "  " },
                if focused {
                    selection_marker()
                } else {
                    Style::default()
                },
            ),
            Span::styled(content, if focused { selected() } else { kind_style }),
            Span::raw(" "),
        ])
    });
    frame.render_widget(
        Paragraph::new(tree_lines.collect::<Vec<_>>())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if tree_focus { focused_panel() } else { panel() })
                    .title(format!(
                        "结构树  {}/{}",
                        selected_index.saturating_add(1),
                        rows.len()
                    )),
            )
            .wrap(Wrap { trim: false }),
        tree_area,
    );
}
