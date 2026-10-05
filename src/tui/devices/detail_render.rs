use super::presentation::device_detail_lines;
use super::*;

pub(super) fn draw_device_detail(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let focused = state.devices_focused_pane() == PaneId::DevicesDetail;
    let Some(row) = state.selected_device() else {
        frame.render_widget(
            Paragraph::new("选择设备后显示详情。").block(crate::tui::ui::card("设备详情", focused)),
            area,
        );
        return;
    };

    let key = state.device_info_selected_key();
    let title = state
        .device_info_tree_rows()
        .into_iter()
        .find(|node| node.key == key)
        .map(|node| node.label)
        .unwrap_or_else(|| "设备详情".into());
    if key == crate::tui::state::DeviceInfoNodeKey::Capacity {
        draw_capacity_detail(frame, area, state, row, title.as_str(), focused);
        return;
    }
    if matches!(
        key,
        crate::tui::state::DeviceInfoNodeKey::Status
            | crate::tui::state::DeviceInfoNodeKey::Backups
    ) {
        draw_status_backup_detail(frame, area, state, row, title.as_str(), focused);
        return;
    }
    let lines = device_detail_lines(state, row, key, area.width.saturating_sub(4) as usize);
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(title.as_str(), focused))
            .scroll((
                state.pane_viewport(PaneId::DevicesDetail).scroll_y.offset as u16,
                0,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_capacity_detail(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
    row: &crate::disk_scan::Row,
    title: &str,
    focused: bool,
) {
    use crate::tui::disk_layout::{DiskCapacityMap, DiskCapacityMapProfile, TailExpansion};
    use crate::tui::disk_region_list::{render_disk_region_list_body, DiskRegionListMode};

    let block = crate::tui::ui::card(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Ok(model) = row.canonical_layout() else {
        frame.render_widget(Paragraph::new("无法建立可靠容量布局。"), inner);
        return;
    };
    let regions = state.device_capacity_region_state();
    let selection = regions.selection();
    let profile = if inner.height >= 16 {
        DiskCapacityMapProfile::Full
    } else {
        DiskCapacityMapProfile::Mini
    };
    let map = DiskCapacityMap::new(&model, profile)
        .with_tail(TailExpansion::Collapsed)
        .with_selection(selection.clone())
        .with_marker(true);
    let map_height = if inner.height >= 16 { 8 } else { 2 };
    let chunks = Layout::vertical([
        Constraint::Length(map_height),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    let mut map_lines = vec![Line::from(Span::styled(
        "全盘容量地图",
        crate::tui::theme::current()
            .secondary_accent()
            .add_modifier(Modifier::BOLD),
    ))];
    map_lines.extend(map.lines(usize::from(inner.width)));
    frame.render_widget(Paragraph::new(map_lines), chunks[0]);
    render_disk_region_list_body(
        frame,
        chunks[1],
        &model,
        &regions,
        DiskRegionListMode::Interactive { focused },
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "j/k 选择区域 · gg/G 首尾区域 · Ctrl-w h 返回结构树",
            muted(),
        ))),
        chunks[2],
    );
}

fn draw_status_backup_detail(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
    row: &crate::disk_scan::Row,
    title: &str,
    focused: bool,
) {
    use crate::application::media_identity::BackupAffinity;
    use crate::tui::table_layout::{
        render_table_scrollbars, table_column_schema, table_heading, table_position_label,
        visible_cell, ColumnId, TableKind,
    };

    let block = crate::tui::ui::card(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let summary = device_detail_lines(
        state,
        row,
        crate::tui::state::DeviceInfoNodeKey::Status,
        inner.width as usize,
    );
    let summary_height = (summary.len() as u16)
        .min(inner.height.saturating_sub(4))
        .max(1);
    let sections = Layout::vertical([
        Constraint::Length(summary_height),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(summary).wrap(Wrap { trim: false }),
        sections[0],
    );

    let related = state.device_related_backups();
    if related.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                if row.n_baks + row.n_possible_baks == 0 {
                    "没有识别到与当前设备相关的备份。"
                } else {
                    "关联备份已计数，备份工作区尚未加载完整列表。"
                },
                muted(),
            )),
            sections[1],
        );
        frame.render_widget(
            Paragraph::new(Span::styled("R 恢复 · 当前没有可操作备份", muted())),
            sections[2],
        );
        return;
    }

    let kind = TableKind::RelatedBackups;
    let columns = table_column_schema(kind).expect("related backup schema");
    let headings = columns
        .iter()
        .map(|column| column.heading)
        .collect::<Vec<_>>();
    let view = state.device_related_backup_table_view();
    let order = state.table_column_order(kind);
    let layout = state.table_visual_layout(kind);
    let visual_widths = state.table_visual_widths(kind, &view.content_widths);
    let interaction = state.table_interaction(kind);
    let viewport = layout.layout_with_active(
        sections[1].width.saturating_sub(4),
        &visual_widths,
        interaction.viewport_offset(),
        Some(interaction.active_column()),
    );
    let rows = related
        .iter()
        .enumerate()
        .map(|(position, (source, affinity))| {
            let backup = &state.backups()[*source];
            let values = &view.rows[position];
            TableRow::new(
                viewport
                    .columns
                    .iter()
                    .map(|column| {
                        let logical = order[column.index];
                        let value = &values[logical];
                        let style = match columns[logical].id {
                            ColumnId::Relation => match affinity {
                                BackupAffinity::Confirmed => success(),
                                BackupAffinity::Possible => warning(),
                                BackupAffinity::Unrelated => muted(),
                            },
                            ColumnId::ProvisionKind => backup
                                .provision_kind
                                .map(|kind| crate::tui::theme::current().provision_kind(kind))
                                .unwrap_or_else(warning),
                            ColumnId::Model
                            | ColumnId::VidPid
                            | ColumnId::Onlyid
                            | ColumnId::Name => crate::tui::theme::current().table_text_muted(),
                            _ => Style::default(),
                        };
                        let style = crate::tui::theme::current().table_cell(
                            style,
                            column.index == interaction.active_column(),
                            focused,
                        );
                        Cell::from(visible_cell(value, column)).style(style)
                    })
                    .collect::<Vec<_>>(),
            )
        });
    let header = TableRow::new(
        viewport
            .columns
            .iter()
            .map(|column| {
                let logical = order[column.index];
                let label = table_heading(headings[logical], logical, interaction);
                let style = crate::tui::theme::current()
                    .table_header(column.index == interaction.active_column(), focused);
                Cell::from(visible_cell(&label, column)).style(style)
            })
            .collect::<Vec<_>>(),
    );
    let table_title = format!(
        "关联备份 · {}",
        table_position_label(&layout, interaction, &viewport)
    );
    let table = crate::tui::ui::data_table(&table_title, header, rows, viewport.widths(), focused);
    let mut table_state = TableState::default();
    table_state.select(state.device_related_backup_selected_index());
    frame.render_stateful_widget(table, sections[1], &mut table_state);
    render_table_scrollbars(
        frame,
        sections[1],
        &viewport,
        related.len(),
        0,
        related
            .len()
            .min(sections[1].height.saturating_sub(3) as usize),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", accent()),
            Span::raw(" 选择 · "),
            Span::styled("h/l", accent()),
            Span::raw(" 列 · "),
            Span::styled("H/L", accent()),
            Span::raw(" 横移 · "),
            Span::styled("R", accent()),
            Span::raw(" 恢复当前备份"),
        ])),
        sections[2],
    );
}
