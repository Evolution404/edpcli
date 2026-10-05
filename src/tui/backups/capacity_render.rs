use super::*;
use crate::tui::disk_layout::{DiskCapacityMap, DiskCapacityMapProfile, TailExpansion};
use crate::tui::disk_region_list::{render_disk_region_list_body, DiskRegionListMode};

pub(super) fn draw_backup_capacity(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let focused = state.backups_focused_pane() == crate::tui::pane::PaneId::BackupCoverage;
    let title = state
        .selected_backup()
        .map(|backup| format!("容量布局 · 备份 #{}", backup.index))
        .unwrap_or_else(|| "容量布局".into());
    let block = crate::tui::ui::card(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(backup) = state.selected_backup() else {
        frame.render_widget(Paragraph::new("选择一条备份查看源盘容量布局。"), inner);
        return;
    };
    let Some(model) = backup
        .restore_preview
        .as_ref()
        .and_then(|preview| preview.layout.as_ref())
    else {
        let reason = backup
            .restore_preview
            .as_ref()
            .and_then(|preview| preview.layout_error.as_deref())
            .unwrap_or("备份未提供可验证的容量布局");
        frame.render_widget(
            Paragraph::new(Span::styled(safe(reason), warning())).wrap(Wrap { trim: false }),
            inner,
        );
        return;
    };
    let regions = state.backup_capacity_region_state();
    let selection = regions.selection();
    let profile = if inner.height >= 16 {
        DiskCapacityMapProfile::Full
    } else {
        DiskCapacityMapProfile::Mini
    };
    let map = DiskCapacityMap::new(model, profile)
        .with_tail(TailExpansion::Collapsed)
        .with_selection(selection.clone())
        .with_marker(true);
    let map_lines = map.lines(usize::from(inner.width));
    let chunks = crate::tui::backup_layout::capacity_sections(inner);
    frame.render_widget(Paragraph::new(map_lines), chunks[0]);
    render_disk_region_list_body(
        frame,
        chunks[1],
        model,
        &regions,
        DiskRegionListMode::Interactive { focused },
    );
    let selected = model
        .collapsed_tail_model()
        .segments
        .into_iter()
        .find(|segment| {
            selection.as_ref().is_some_and(|selection| {
                segment.start_lba == selection.start_lba
                    && segment.end_exclusive().ok() == Some(selection.end_exclusive)
                    && segment.kind == selection.kind
            })
        });
    if let Some(segment) = selected {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("当前区域  {}", safe(&segment.label)),
                    crate::tui::theme::current()
                        .disk_region(segment.kind)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(format!(
                    "LBA {} · {} sector",
                    segment.closed_range(),
                    segment.sector_count
                )),
                Line::from(Span::styled(
                    "j/k 选择区域 · Ctrl-w w 切换窗口 · Esc 返回列表",
                    muted(),
                )),
            ])
            .wrap(Wrap { trim: false }),
            chunks[2],
        );
    }
}
