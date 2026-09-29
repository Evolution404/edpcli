use super::*;
use ratatui::widgets::{Clear, List, ListItem, ListState};

fn centered_modal(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let width = area.width.saturating_sub(2).clamp(1, 78);
    let height = area.height.saturating_sub(2).clamp(1, 17);
    ratatui::layout::Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn shortcut(kind: ProvisionKind) -> &'static str {
    match kind {
        ProvisionKind::Mode0 => "0",
        ProvisionKind::Mode1 => "1",
        ProvisionKind::Mode2 => "2",
        ProvisionKind::Mode3 => "3",
        ProvisionKind::Plain => "P",
    }
}

pub(super) fn draw_scheme_picker(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let popup = centered_modal(area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focused_panel())
        .title(Span::styled(
            " 选择制盘方案 ",
            accent().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

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
                    "  ·  {:.2} GiB  ·  {}:{}",
                    row.size as f64 / 1_073_741_824.0,
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

    let items = ProvisionKind::ALL
        .into_iter()
        .map(|kind| {
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}  ", shortcut(kind)),
                        provision_kind_style(kind).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        safe(kind.title()),
                        provision_kind_style(kind).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(safe(kind.description()), muted()),
                ]),
            ])
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .highlight_style(selected().add_modifier(Modifier::BOLD))
        .highlight_symbol("▌ ");
    let mut list_state = ListState::default();
    list_state.select(Some(state.provision_scheme_selected()));
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
}
