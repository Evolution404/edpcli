use super::*;
use ratatui::widgets::{List, ListItem, ListState};

fn shortcut(kind: ProvisionKind) -> &'static str {
    match kind {
        ProvisionKind::Mode0 => "0",
        ProvisionKind::Mode1 => "1",
        ProvisionKind::Mode2 => "2",
        ProvisionKind::Mode3 => "3",
        ProvisionKind::Plain => "P",
    }
}

pub(super) fn draw_scheme_picker(frame: &mut Frame, state: &AppState) {
    let popup = crate::tui::ui::centered_modal_rect(frame.area(), 78, 17);
    crate::tui::ui::render_modal(frame, popup, "选择制盘方案", |frame, inner| {
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(inner);

        let target = state.selected_device();
        let target_line = target
            .map(|row| {
                Line::from(vec![
                    Span::styled(
                        format!("disk{}", row.disk),
                        accent().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!(
                        "  ·  {}  ·  {}:{}",
                        crate::common::fmt_capacity(row.size),
                        safe(&row.vid),
                        safe(&row.pid)
                    )),
                ])
            })
            .unwrap_or_else(|| Line::from(Span::styled("未固定目标 USB", danger())));
        frame.render_widget(
            Paragraph::new(vec![
                target_line,
                Line::from(Span::styled("选择方案后直接进入参数表单", muted())),
            ]),
            rows[0],
        );

        let selected_index = state.provision_scheme_selected();
        let items = ProvisionKind::ALL
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                let active = index == selected_index;
                let mode_style = if active {
                    crate::tui::theme::current().provision_kind_active(kind.disk_kind())
                } else {
                    crate::tui::theme::current().provision_kind(kind.disk_kind())
                };
                ListItem::new(vec![
                    Line::from(vec![
                        if active {
                            Span::styled("▌ ", mode_style)
                        } else {
                            Span::raw("  ")
                        },
                        Span::styled(format!("{}  ", shortcut(kind)), mode_style),
                        Span::styled(safe(kind.title()), mode_style),
                    ]),
                    Line::from(vec![
                        Span::raw("     "),
                        Span::styled(safe(kind.description()), muted()),
                    ]),
                ])
            })
            .collect::<Vec<_>>();

        let list = List::new(items);
        let mut list_state = ListState::default();
        list_state.select(Some(selected_index));
        frame.render_stateful_widget(list, rows[1], &mut list_state);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("j/k", secondary()),
                Span::raw(" / ↑↓ 选择   "),
                Span::styled("Enter", secondary()),
                Span::raw(" 确认   "),
                Span::styled("Esc", secondary()),
                Span::raw(" 取消"),
            ]))
            .alignment(Alignment::Center),
            rows[2],
        );
    });
}
