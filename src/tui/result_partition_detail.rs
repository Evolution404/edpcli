use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::application::post_restore::PostRestorePartition;
use crate::tui::state::ProvisionResultPartition;

fn row_value(row: Option<&[String]>, index: usize) -> String {
    row.and_then(|row| row.get(index))
        .cloned()
        .unwrap_or_else(|| "—".into())
}

fn detail_block(title: String) -> Block<'static> {
    Block::default().borders(Borders::TOP).title(title)
}

pub(crate) fn render_restore_partition_detail(
    frame: &mut Frame,
    area: Rect,
    partition: &PostRestorePartition,
    row: Option<&[String]>,
    status_style: Style,
) {
    let theme = crate::tui::theme::current();
    let end_lba = partition
        .start_lba
        .saturating_add(partition.sector_count.saturating_sub(1));
    let detail = vec![
        Line::from(vec![
            Span::styled("状态  ", theme.table_text_muted()),
            Span::styled(row_value(row, 1), status_style),
            Span::raw("    "),
            Span::styled("文件系统  ", theme.table_text_muted()),
            Span::raw(row_value(row, 2)),
        ]),
        Line::from(vec![
            Span::styled("LBA  ", theme.table_text_muted()),
            Span::raw(format!("{}–{}", partition.start_lba, end_lba)),
            Span::raw("    "),
            Span::styled("容量  ", theme.table_text_muted()),
            Span::raw(row_value(row, 3)),
        ]),
        Line::from(vec![
            Span::styled("密钥  ", theme.table_text_muted()),
            Span::raw(row_value(row, 4)),
        ]),
        Line::from(vec![
            Span::styled("说明  ", theme.table_text_muted()),
            Span::raw(row_value(row, 5)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(detail)
            .block(detail_block(format!("当前分区  P{}", partition.index)))
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(crate) fn render_provision_partition_detail(
    frame: &mut Frame,
    area: Rect,
    partition_index: usize,
    partition: &ProvisionResultPartition,
    row: Option<&[String]>,
    final_status_style: Style,
) {
    let theme = crate::tui::theme::current();
    let sectors = partition.size_bytes / crate::common::SECTOR as u64;
    let end_lba = partition
        .start_lba
        .saturating_add(sectors.saturating_sub(1));
    let detail = vec![
        Line::from(vec![
            Span::styled("角色  ", theme.table_text_muted()),
            Span::raw(row_value(row, 1)),
            Span::raw("    "),
            Span::styled("文件系统  ", theme.table_text_muted()),
            Span::raw(row_value(row, 2)),
        ]),
        Line::from(vec![
            Span::styled("LBA  ", theme.table_text_muted()),
            Span::raw(format!("{}–{}", partition.start_lba, end_lba)),
            Span::raw("    "),
            Span::styled("容量  ", theme.table_text_muted()),
            Span::raw(row_value(row, 3)),
        ]),
        Line::from(vec![
            Span::styled("处理  ", theme.table_text_muted()),
            Span::raw(row_value(row, 4)),
        ]),
        Line::from(vec![
            Span::styled("最终状态  ", theme.table_text_muted()),
            Span::styled(row_value(row, 5), final_status_style),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(detail)
            .block(detail_block(format!("当前分区  P{}", partition_index + 1)))
            .wrap(Wrap { trim: true }),
        area,
    );
}
