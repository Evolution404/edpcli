use super::*;
use crate::application::inspect_tree::DiskRegionSemantic;
use crate::application::inspect_tree::InspectNodeKind;
use crate::tui::disk_layout::DiskRegionKind;
use crate::tui::state::AdvancedInspectTreeRow;

fn region_kind(semantic: Option<DiskRegionSemantic>) -> Option<DiskRegionKind> {
    match semantic? {
        DiskRegionSemantic::Protocol => Some(DiskRegionKind::Protocol),
        DiskRegionSemantic::Reserved => Some(DiskRegionKind::Reserved),
        DiskRegionSemantic::PartitionTable => Some(DiskRegionKind::Metadata),
        DiskRegionSemantic::PlainPartition | DiskRegionSemantic::MbrPartition { .. } => {
            Some(DiskRegionKind::Plain)
        }
        DiskRegionSemantic::Unallocated => Some(DiskRegionKind::Free),
        DiskRegionSemantic::Lce => Some(DiskRegionKind::Lce),
        DiskRegionSemantic::Tail => Some(DiskRegionKind::Tail),
        DiskRegionSemantic::TailMetadataMirror => Some(DiskRegionKind::BackupMirror),
        DiskRegionSemantic::TailRestoreNode => Some(DiskRegionKind::RestoreNode),
        DiskRegionSemantic::Partition { partition_type } => match partition_type {
            1 => Some(DiskRegionKind::Boot),
            2 => Some(DiskRegionKind::Share),
            4 => Some(DiskRegionKind::Encrypt),
            _ => Some(DiskRegionKind::Compatibility),
        },
        DiskRegionSemantic::Unknown | DiskRegionSemantic::Conflict => Some(DiskRegionKind::Unknown),
    }
}

pub(super) fn draw_inspect_tree_pane(
    frame: &mut Frame,
    tree_area: ratatui::layout::Rect,
    rows: &[AdvancedInspectTreeRow],
    selected_index: usize,
    tree_focus: bool,
    scroll_x: usize,
) {
    let visible =
        crate::tui::table_layout::table_row_window(selected_index, rows.len(), tree_area.height);
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
        let selected = index == selected_index;
        let focused = tree_focus && selected;
        let region_style = region_kind(row.region_semantic)
            .map(|kind| crate::tui::theme::current().disk_region_tree(kind, focused));
        let kind_style = region_style.unwrap_or_else(|| match row.kind {
            InspectNodeKind::Device => secondary().add_modifier(Modifier::BOLD),
            InspectNodeKind::Region | InspectNodeKind::Partition => accent(),
            InspectNodeKind::Extent | InspectNodeKind::Structure => success(),
            InspectNodeKind::Sector | InspectNodeKind::Field => Style::default(),
            InspectNodeKind::Group | InspectNodeKind::UnknownRange => muted(),
        });
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
        let base_style = if region_style.is_some() {
            kind_style
        } else if focused {
            crate::tui::theme::current().active_semantic(kind_style)
        } else {
            kind_style
        };
        let content_style =
            crate::tui::theme::current().apply_selection(base_style, selected, tree_focus);
        let marker_style = if let Some(style) = region_style {
            crate::tui::theme::current().apply_selection(style, selected, tree_focus)
        } else if focused {
            selection_marker()
        } else {
            Style::default()
        };
        Line::from(vec![
            Span::raw(indent),
            Span::styled(if focused { "▌ " } else { "  " }, marker_style),
            Span::styled(content, content_style),
            Span::raw(" "),
        ])
    });
    let content_width = usize::from(tree_area.width.saturating_sub(2).max(1));
    let max_width = super::render_helpers::inspect_tree_max_width(rows);
    let effective_scroll = scroll_x.min(max_width.saturating_sub(content_width));
    frame.render_widget(
        Paragraph::new(tree_lines.collect::<Vec<_>>())
            .block(crate::tui::ui::card(
                format!(
                    "扇区树  {}/{}",
                    selected_index.saturating_add(1),
                    rows.len()
                ),
                tree_focus,
            ))
            .scroll((0, effective_scroll.min(u16::MAX as usize) as u16)),
        tree_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_region_semantics_reuse_capacity_map_region_kinds() {
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Protocol)),
            Some(DiskRegionKind::Protocol)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Reserved)),
            Some(DiskRegionKind::Reserved)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Unallocated)),
            Some(DiskRegionKind::Free)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Partition { partition_type: 1 })),
            Some(DiskRegionKind::Boot)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Partition { partition_type: 2 })),
            Some(DiskRegionKind::Share)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Partition { partition_type: 4 })),
            Some(DiskRegionKind::Encrypt)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Tail)),
            Some(DiskRegionKind::Tail)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::Lce)),
            Some(DiskRegionKind::Lce)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::TailMetadataMirror)),
            Some(DiskRegionKind::BackupMirror)
        );
        assert_eq!(
            region_kind(Some(DiskRegionSemantic::TailRestoreNode)),
            Some(DiskRegionKind::RestoreNode)
        );
    }
}
