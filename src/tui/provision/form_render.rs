use super::*;
use crate::tui::state::{
    ProvisionFieldId, ProvisionFieldSection, ProvisionPasswordVerificationState,
};
use std::collections::HashMap;

#[path = "form_special_rows.rs"]
mod form_special_rows;
use form_special_rows::{advanced_settings_row, password_domain_row};

const INPUT_EDITING_SLACK: usize = 2;

pub(super) fn draw_provision_form(
    frame: &mut Frame,
    main_area: ratatui::layout::Rect,
    state: &AppState,
) {
    let provision = state.provision();
    let class = crate::tui::ui::ViewportClass::for_width(main_area.width);
    let wide = matches!(
        class,
        crate::tui::ui::ViewportClass::Wide | crate::tui::ui::ViewportClass::UltraWide
    );
    let focused_pane = state.provision_focused_pane();
    let parameters_focused = focused_pane == crate::tui::pane::PaneId::ProvisionParameters;
    let (form_area, layout_area) = if wide {
        let areas = Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)])
            .split(main_area);
        (Some(areas[0]), Some(areas[1]))
    } else if focused_pane == crate::tui::pane::PaneId::ProvisionDiskLayout {
        (None, Some(main_area))
    } else {
        (Some(main_area), None)
    };
    let form_geometry = form_area.unwrap_or(main_area);
    let content_width = form_geometry.width.saturating_sub(2) as usize;
    let separator = " │ ";
    let separator_width = crate::ui::disp_width(separator);

    let fields = state.provision_visible_fields();
    let rows = state.provision_compact_field_rows_typed();
    let mut section_metrics: HashMap<ProvisionFieldSection, (usize, usize, usize, usize)> =
        HashMap::new();
    for (section, indexes) in &rows {
        let entry = section_metrics.entry(*section).or_insert((0, 0, 0, 0));
        for (position, index) in indexes.iter().copied().enumerate() {
            let (label, value, secret) = &fields[index];
            let label_width = crate::ui::disp_width(label);
            let shown_width = if value.is_empty() {
                crate::ui::disp_width("〈请输入〉")
            } else if *secret {
                value.chars().count()
            } else {
                crate::ui::disp_width(&safe(value))
            }
            .saturating_add(INPUT_EDITING_SLACK)
            .clamp(8, 26);
            if position == 0 {
                entry.0 = entry.0.max(label_width);
                entry.2 = entry.2.max(shown_width);
            } else {
                entry.1 = entry.1.max(label_width);
                entry.3 = entry.3.max(shown_width);
            }
        }
    }

    let mut form_lines = vec![Line::from(vec![
        Span::styled(
            provision.kind.title(),
            crate::tui::theme::current().provision_kind_emphasis(provision.kind.disk_kind()),
        ),
        Span::raw("  "),
        Span::styled(provision.kind.description(), muted()),
    ])];
    let mut current_section: Option<ProvisionFieldSection> = None;
    let mut selected_line = 0usize;
    for (section, indexes) in rows {
        if current_section != Some(section) {
            if section != ProvisionFieldSection::AdvancedIdentity {
                form_lines.push(Line::from(""));
                form_lines.push(Line::from(Span::styled(section.label(), secondary())));
            }
            current_section = Some(section);
        }
        if indexes.len() == 1
            && state.provision_field_id(indexes[0]) == Some(ProvisionFieldId::AdvancedSection)
        {
            if indexes[0] == provision.field_selected {
                selected_line = form_lines.len();
            }
            form_lines.push(advanced_settings_row(state, indexes[0], parameters_focused));
            continue;
        }
        if section == ProvisionFieldSection::PasswordDomain && indexes.len() == 2 {
            if indexes.contains(&provision.field_selected) {
                selected_line = form_lines.len();
            }
            if let Some(line) =
                password_domain_row(state, &indexes, &fields, content_width, parameters_focused)
            {
                form_lines.push(line);
                continue;
            }
        }
        let mut spans = Vec::new();
        let row_selected = indexes.contains(&provision.field_selected);
        if row_selected {
            selected_line = form_lines.len();
        }
        let two_columns = indexes.len() == 2;
        for (position, index) in indexes.into_iter().enumerate() {
            let (label, value, secret) = &fields[index];
            let active = index == provision.field_selected;
            if position > 0 {
                spans.push(Span::styled(separator, muted()));
            }
            let metrics = section_metrics
                .get(&section)
                .copied()
                .unwrap_or((0, 0, 8, 8));
            let min_right_width = 2 + metrics.1 + 1 + metrics.3.max(8);
            let desired_left_width = 2 + metrics.0 + 1 + metrics.2.max(8);
            let max_left_width = content_width
                .saturating_sub(separator_width)
                .saturating_sub(min_right_width)
                .max(8);
            let section_left_width = desired_left_width.min(max_left_width);
            let cell_width = if two_columns {
                if position == 0 {
                    section_left_width
                } else {
                    content_width
                        .saturating_sub(section_left_width)
                        .saturating_sub(separator_width)
                }
            } else {
                content_width
            };
            let label_width = if position == 0 { metrics.0 } else { metrics.1 };
            let label_width = label_width.min(cell_width.saturating_sub(4));
            let value_width = cell_width
                .saturating_sub(2)
                .saturating_sub(label_width)
                .saturating_sub(1)
                .max(1);

            let focused_active = active && parameters_focused;
            if section == ProvisionFieldSection::AdvancedIdentity {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                if focused_active { "▌ " } else { "  " },
                if focused_active {
                    selection_marker()
                } else {
                    Style::default()
                },
            ));
            spans.push(Span::styled(fit_display_width(label, label_width), muted()));
            spans.push(Span::raw(" "));

            let editable_active = active && state.provision_selected_field_is_editable();
            let editing_active = editable_active && state.input_mode() == InputMode::Insert;
            let shown = if editing_active {
                input_value_window(
                    value,
                    state.provision_field_cursor(),
                    value_width.saturating_sub(2),
                    *secret,
                )
            } else if value.is_empty() {
                "〈请输入〉".to_string()
            } else if *secret {
                "•".repeat(value.chars().count())
            } else {
                fit_display_width(value, value_width).trim_end().to_string()
            };
            if editing_active {
                let occupied = crate::ui::disp_width(&shown)
                    .saturating_add(2)
                    .min(value_width);
                spans.push(Span::styled("[", accent()));
                spans.push(Span::styled(shown, input_focused()));
                spans.push(Span::styled("]", accent()));
                if occupied < value_width {
                    spans.push(Span::raw(" ".repeat(value_width - occupied)));
                }
            } else {
                let occupied = crate::ui::disp_width(&shown).min(value_width);
                spans.push(Span::styled(
                    shown,
                    if focused_active {
                        accent().add_modifier(Modifier::BOLD)
                    } else {
                        crate::tui::theme::current().secondary_text()
                    },
                ));
                if occupied < value_width {
                    spans.push(Span::raw(" ".repeat(value_width - occupied)));
                }
            }
        }
        form_lines.push(Line::from(spans));
    }
    if let Some(hint) = state.provision_field_hint(provision.field_selected) {
        form_lines.push(Line::from(""));
        form_lines.push(Line::from(vec![
            Span::styled("提示  ", secondary()),
            Span::styled(safe(&hint), muted()),
        ]));
    }
    if let Some(message) = &provision.message {
        form_lines.push(Line::from(Span::styled(safe(message), danger())));
    }

    let visible_height = form_geometry.height.saturating_sub(2) as usize;
    let scroll = selected_line.saturating_sub(visible_height.saturating_sub(3));
    if let Some(form_area) = form_area {
        frame.render_widget(
            Paragraph::new(form_lines)
                .block(crate::tui::ui::card(
                    "参数",
                    focused_pane == crate::tui::pane::PaneId::ProvisionParameters,
                ))
                .scroll((scroll as u16, 0))
                .wrap(Wrap { trim: false }),
            form_area,
        );
    }

    if let Some(layout_area) = layout_area {
        let layout_model = state.provision_layout_model();
        let map_selection = parameters_focused
            .then(|| state.provision_field_region_selection(&layout_model))
            .flatten();
        let show_linked_selection = parameters_focused && map_selection.is_some();
        let layout_details = state.provision_layout_editor_details();
        let layout_summary = state.selected_device().map_or_else(
            || {
                format!(
                    "{} · {}",
                    provision.kind.title(),
                    AppState::format_sector_size(layout_model.total_sectors)
                )
            },
            |device| {
                format!(
                    "disk{} · {} · {} · {}:{} · {}",
                    device.disk,
                    crate::common::fmt_capacity(device.size),
                    safe(&device.proto),
                    safe(&device.vid),
                    safe(&device.pid),
                    provision.kind.title()
                )
            },
        );
        layout_model.render_pane(
            frame,
            layout_area,
            crate::tui::disk_layout::DiskLayoutPane {
                title: "目标与磁盘布局",
                summary: &layout_summary,
                details: &layout_details,
                focused: focused_pane == crate::tui::pane::PaneId::ProvisionDiskLayout,
                scroll_y: state
                    .pane_viewport(crate::tui::pane::PaneId::ProvisionDiskLayout)
                    .scroll_y
                    .offset,
                profile: crate::tui::disk_layout::DiskLayoutProfile::EditorExact,
                tail: state.disk_layout_tail_expansion(),
                selected_segment: state.disk_layout_selected(),
                map_selection,
                show_map_marker: true,
                show_linked_selection,
            },
        );
    }
}
