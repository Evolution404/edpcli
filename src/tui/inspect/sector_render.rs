use super::super::*;
use super::inspect_field_status_style;

pub(super) fn draw_sector_inspector(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    use crate::tui::state::SectorInspectMode;

    let Some(sector) = state.advanced_inspect_sector() else {
        return;
    };
    let item = state.advanced_inspect_sector_item();
    let active_field = state.advanced_inspect_sector_active_field();
    let absolute = (sector.lba as u128) * crate::common::SECTOR as u128 + sector.cursor as u128;
    let decode_issue = item.and_then(|item| item.decode_error.as_deref());
    let breadcrumb = state.advanced_inspect_breadcrumb();
    let header = vec![
        Line::from(
            breadcrumb
                .as_ref()
                .map(|model| safe(&model.display()))
                .unwrap_or_default(),
        ),
        Line::from(vec![
            Span::styled(
                format!("LBA{}  ", sector.lba),
                secondary().add_modifier(Modifier::BOLD),
            ),
            Span::styled(sector.mode.label(), accent().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                "  · byte +0x{:03X} / absolute 0x{:X} / row {:02}/32{}",
                sector.cursor,
                absolute,
                sector.cursor / 16 + 1,
                if sector.pending { "  · 读取中" } else { "" }
            )),
        ]),
        Line::from(format!(
            "{} · {}",
            breadcrumb
                .as_ref()
                .map(|model| model.escape_hint())
                .unwrap_or_else(|| "Esc 返回".into()),
            sector
                .error
                .as_deref()
                .or(decode_issue)
                .map(|value| format!("decode: {}", safe(value)))
                .unwrap_or_else(|| "decode: available/raw".into())
        )),
    ];
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(header).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Sector Inspector"),
        ),
        vertical[0],
    );

    let class = crate::tui::ui::ViewportClass::for_width(area.width);
    let main = if matches!(
        class,
        crate::tui::ui::ViewportClass::Wide | crate::tui::ui::ViewportClass::UltraWide
    ) {
        let parts = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(vertical[1]);
        (parts[0], parts[1])
    } else {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(10)])
            .split(vertical[1]);
        (parts[0], parts[1])
    };

    let mut hex_lines = Vec::new();
    if let Some(item) = item {
        let decoded = item.decoded.as_deref();
        let display = match sector.mode {
            SectorInspectMode::Raw => item.raw.as_slice(),
            SectorInspectMode::Decode | SectorInspectMode::Mixed => {
                decoded.unwrap_or(item.raw.as_slice())
            }
        };
        for row in 0..32usize {
            let offset = row * 16;
            let row_abs = (sector.lba as u128) * crate::common::SECTOR as u128 + offset as u128;
            let mut spans = vec![
                Span::styled(format!("+0x{offset:03X}  "), muted()),
                Span::styled(format!("0x{row_abs:012X}  "), muted()),
            ];
            for column in 0..16usize {
                let index = offset + column;
                let byte = display[index];
                let absolute = sector
                    .lba
                    .saturating_mul(crate::common::SECTOR as u64)
                    .saturating_add(index as u64);
                let active_status = active_field.as_ref().and_then(|field| {
                    (absolute >= field.range.start && absolute < field.range.end_exclusive)
                        .then_some(field.status)
                });
                let style = if index == sector.cursor {
                    crate::tui::theme::current().cursor()
                } else if let Some(status) = active_status {
                    inspect_field_status_style(status)
                } else if sector.mode == SectorInspectMode::Mixed
                    && decoded.is_some_and(|decoded| decoded[index] != item.raw[index])
                {
                    secondary()
                } else {
                    Style::default()
                };
                spans.push(Span::styled(format!("{byte:02X} "), style));
                if column == 7 {
                    spans.push(Span::raw(" "));
                }
            }
            let ascii = display[offset..offset + 16]
                .iter()
                .map(|byte| {
                    if (0x20..=0x7e).contains(byte) {
                        *byte as char
                    } else {
                        '.'
                    }
                })
                .collect::<String>();
            spans.push(Span::styled(format!(" |{ascii}|"), muted()));
            hex_lines.push(Line::from(spans));
        }
    } else {
        hex_lines.push(Line::from(if sector.pending {
            "正在后台读取当前扇区…"
        } else {
            "当前扇区没有可显示的数据。"
        }));
    }
    let visible_rows = main.0.height.saturating_sub(2).max(1) as usize;
    let cursor_row = sector.cursor / 16;
    let max_scroll = 32usize.saturating_sub(visible_rows);
    let scroll = cursor_row
        .saturating_sub(visible_rows / 2)
        .min(max_scroll)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(hex_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("32 × 16 Hex + ASCII"),
            )
            .scroll((scroll, 0)),
        main.0,
    );

    let mut details = Vec::new();
    if let Some(item) = item {
        let raw = item.raw.get(sector.cursor).copied().unwrap_or_default();
        let decoded = item
            .decoded
            .as_ref()
            .and_then(|bytes| bytes.get(sector.cursor))
            .copied();
        details.push(Line::from(format!("Raw byte: 0x{raw:02X} ({raw})")));
        if let Some(decoded) = decoded {
            details.push(Line::from(format!("Decoded:  0x{decoded:02X} ({decoded})")));
        }
        details.push(Line::from(""));
        if let Some(field) = active_field.as_ref() {
            details.push(Line::from(Span::styled(
                safe(&field.label),
                accent().add_modifier(Modifier::BOLD),
            )));
            details.push(Line::from(format!("Value: {}", safe(&field.value))));
            details.push(Line::from(vec![
                Span::raw(format!("Type: {:?} / Status: ", field.field_type)),
                Span::styled(
                    format!("{:?}", field.status),
                    inspect_field_status_style(field.status).add_modifier(Modifier::BOLD),
                ),
            ]));
            details.push(Line::from(format!(
                "Range: 0x{:X}..0x{:X}",
                field.range.start, field.range.end_exclusive
            )));
            if sector.field_expanded {
                details.push(Line::from(""));
                details.push(Line::from(format!(
                    "bits: b7={} b6={} b5={} b4={} b3={} b2={} b1={} b0={}",
                    (raw >> 7) & 1,
                    (raw >> 6) & 1,
                    (raw >> 5) & 1,
                    (raw >> 4) & 1,
                    (raw >> 3) & 1,
                    (raw >> 2) & 1,
                    (raw >> 1) & 1,
                    raw & 1
                )));
                for child in &field.children {
                    details.push(Line::from(format!(
                        "{} = {}",
                        safe(&child.label),
                        safe(&child.value)
                    )));
                }
            } else {
                details.push(Line::from("o 展开 bit / child 详情"));
            }
        } else {
            details.push(Line::from(Span::styled("Unknown byte", muted())));
            details.push(Line::from("当前 byte 不属于已知 Field；不推测语义。"));
            if sector.field_expanded {
                details.push(Line::from(format!("bits: {:08b}", raw)));
            }
        }
    } else if let Some(error) = sector.error.as_deref() {
        details.push(Line::from(Span::styled(safe(error), danger())));
    }
    if let Some(yank) = state.advanced_inspect_yank_register() {
        details.push(Line::from(""));
        details.push(Line::from(vec![
            Span::styled("Yank  ", muted()),
            Span::styled(safe(yank), secondary()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(details)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Typed / Field"),
            )
            .wrap(Wrap { trim: false }),
        main.1,
    );

    frame.render_widget(
        Paragraph::new(Line::from(
            "h/l byte · j/k ±16B · 0/$ 行首尾 · gg/G 扇区首尾 · Ctrl-u/d 半页 · PgUp/PgDn 整页 · [/] 前后 sector · v mode · Space/o bit · / n/N 搜索 · J 跳转 · Esc 返回树",
        ))
        .block(Block::default().borders(Borders::TOP)),
        vertical[2],
    );
}
