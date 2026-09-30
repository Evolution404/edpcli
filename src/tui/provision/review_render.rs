use super::*;
use crate::tui::state::{ProvisionReviewRowKind, ProvisionReviewTone};

pub(super) fn draw_provision_review(
    frame: &mut Frame,
    main_area: ratatui::layout::Rect,
    state: &AppState,
) {
    let provision = state.provision();
    let focused_pane = state.provision_focused_pane();
    let class = crate::tui::ui::ViewportClass::for_width(main_area.width);
    let wide = matches!(
        class,
        crate::tui::ui::ViewportClass::Wide | crate::tui::ui::ViewportClass::UltraWide
    );
    let (summary_area, layout_area, changes_area) = if wide {
        let areas = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(main_area);
        (Some(areas[0]), Some(areas[1]), Some(areas[2]))
    } else {
        match focused_pane {
            crate::tui::pane::PaneId::ProvisionDiskLayout => (None, Some(main_area), None),
            crate::tui::pane::PaneId::ProvisionChanges => (None, None, Some(main_area)),
            _ => (Some(main_area), None, None),
        }
    };

    if let Some(summary_area) = summary_area {
        let summary = state.provision_review_summary_rows();
        let lines = summary
            .iter()
            .map(|row| {
                let style = match row.tone {
                    ProvisionReviewTone::Muted => match row.kind {
                        ProvisionReviewRowKind::Action => accent(),
                        _ => muted(),
                    },
                    ProvisionReviewTone::Accent => accent(),
                    ProvisionReviewTone::Success => success(),
                    ProvisionReviewTone::Warning => warning(),
                    ProvisionReviewTone::Danger => danger(),
                };
                Line::from(Span::styled(safe(&row.text), style))
            })
            .collect::<Vec<_>>();
        let scroll = state
            .pane_viewport(crate::tui::pane::PaneId::ProvisionSummary)
            .scroll_y
            .offset
            .min(lines.len().saturating_sub(1));
        frame.render_widget(
            Paragraph::new(lines)
                .block(crate::tui::ui::card(
                    "计划摘要",
                    focused_pane == crate::tui::pane::PaneId::ProvisionSummary,
                ))
                .scroll((scroll.min(u16::MAX as usize) as u16, 0))
                .wrap(Wrap { trim: false }),
            summary_area,
        );
    }

    if let Some(layout_area) = layout_area {
        let layout_model = state.provision_layout_model();
        let layout_summary = format!(
            "{} · {}",
            provision.kind.title(),
            AppState::format_sector_size(layout_model.total_sectors)
        );
        layout_model.render_pane(
            frame,
            layout_area,
            crate::tui::disk_layout::DiskLayoutPane {
                title: "磁盘布局",
                summary: &layout_summary,
                details: &[],
                focused: focused_pane == crate::tui::pane::PaneId::ProvisionDiskLayout,
                scroll_y: state
                    .pane_viewport(crate::tui::pane::PaneId::ProvisionDiskLayout)
                    .scroll_y
                    .offset,
                profile: crate::tui::disk_layout::DiskLayoutProfile::EditorExact,
                tail: state.disk_layout_tail_expansion(),
                selected_segment: state.disk_layout_selected(),
                map_selection: None,
                show_map_marker: false,
                show_linked_selection: false,
            },
        );
    }

    if let Some(changes_area) = changes_area {
        let changes = state.provision_review_change_rows();
        let lines = changes
            .iter()
            .map(|row| {
                let style = match row.tone {
                    ProvisionReviewTone::Muted => muted(),
                    ProvisionReviewTone::Accent => accent(),
                    ProvisionReviewTone::Success => success(),
                    ProvisionReviewTone::Warning => warning(),
                    ProvisionReviewTone::Danger => danger(),
                };
                let mut line = Line::from(Span::styled(safe(&row.text), style));
                if let Some(label) = row.badge {
                    let tone = match row.tone {
                        ProvisionReviewTone::Success => crate::tui::ui::BadgeTone::Success,
                        ProvisionReviewTone::Warning => crate::tui::ui::BadgeTone::Warning,
                        ProvisionReviewTone::Danger => crate::tui::ui::BadgeTone::Danger,
                        ProvisionReviewTone::Accent => crate::tui::ui::BadgeTone::Accent,
                        ProvisionReviewTone::Muted => crate::tui::ui::BadgeTone::Neutral,
                    };
                    let mut badge = crate::tui::ui::status_badge(label, tone);
                    line.spans.insert(0, Span::raw(" "));
                    line.spans.splice(0..0, badge.spans.drain(..));
                }
                line
            })
            .collect::<Vec<_>>();
        let scroll = state
            .pane_viewport(crate::tui::pane::PaneId::ProvisionChanges)
            .scroll_y
            .offset
            .min(lines.len().saturating_sub(1));
        frame.render_widget(
            Paragraph::new(lines)
                .block(crate::tui::ui::card(
                    "变更明细",
                    focused_pane == crate::tui::pane::PaneId::ProvisionChanges,
                ))
                .scroll((scroll.min(u16::MAX as usize) as u16, 0))
                .wrap(Wrap { trim: false }),
            changes_area,
        );
    }
}
