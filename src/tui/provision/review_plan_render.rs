use super::review_render_style::{action_style, data_style, filesystem_style};
use super::*;
use crate::tui::state::ProvisionConfirmationViewModel;
use crate::tui::table_layout::{content_driven_layout, content_widths};

#[derive(Clone, Copy)]
enum PlanColumn {
    Region,
    LbaRange,
    Capacity,
    Handling,
    Data,
    Filesystem,
}

impl PlanColumn {
    const fn heading(self) -> &'static str {
        match self {
            Self::Region => "区域",
            Self::LbaRange => "LBA 范围",
            Self::Capacity => "容量",
            Self::Handling => "处理",
            Self::Data => "数据",
            Self::Filesystem => "文件系统",
        }
    }

    fn value(
        self,
        region: &crate::tui::state::ProvisionConfirmationRegion,
        model: &crate::tui::disk_layout::DiskLayoutModel,
    ) -> String {
        match self {
            Self::Region => safe(&region.label),
            Self::LbaRange => lba_range(region),
            Self::Capacity => model
                .sector_byte_len(region.sector_count)
                .map(crate::common::fmt_capacity)
                .unwrap_or_else(|| "容量溢出".into()),
            Self::Handling => region.action.label().to_string(),
            Self::Data => region.data_effect.label().to_string(),
            Self::Filesystem => region.filesystem_effect.label(),
        }
    }

    fn style(self, region: &crate::tui::state::ProvisionConfirmationRegion) -> Style {
        match self {
            Self::LbaRange => muted(),
            Self::Handling => action_style(region.action),
            Self::Data => data_style(region.data_effect),
            Self::Filesystem => filesystem_style(region.filesystem_effect),
            _ => Style::default(),
        }
    }
}

pub(super) fn draw_partition_plan(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    view: &ProvisionConfirmationViewModel,
    selected: usize,
    focused: bool,
) {
    let visible_capacity = area.height.saturating_sub(3).max(1) as usize;
    let window_len = visible_capacity.min(view.regions.len().max(1));
    let max_start = view.regions.len().saturating_sub(window_len);
    let window_start = selected.saturating_sub(window_len / 2).min(max_start);
    let window_end = window_start
        .saturating_add(window_len)
        .min(view.regions.len());

    let available = usize::from(area.width.saturating_sub(4));
    let full = [
        PlanColumn::Region,
        PlanColumn::LbaRange,
        PlanColumn::Capacity,
        PlanColumn::Handling,
        PlanColumn::Data,
        PlanColumn::Filesystem,
    ];
    let normal = [
        PlanColumn::Region,
        PlanColumn::LbaRange,
        PlanColumn::Handling,
        PlanColumn::Data,
    ];
    let compact = [PlanColumn::Region, PlanColumn::Handling, PlanColumn::Data];

    let columns: &[PlanColumn] =
        if table_total_width(view, window_start, window_end, &full) <= available {
            &full
        } else if table_total_width(view, window_start, window_end, &normal) <= available {
            &normal
        } else {
            &compact
        };

    let headings = columns
        .iter()
        .map(|column| column.heading())
        .collect::<Vec<_>>();
    let values = project_rows(view, window_start, window_end, columns);
    let layout = content_driven_layout(&headings).with_column_spacing(2);
    let measured = content_widths(&headings, &values);
    let widths = layout
        .natural_widths(&measured, None)
        .into_iter()
        .map(|width| Constraint::Length(width.min(u16::MAX as usize) as u16))
        .collect::<Vec<_>>();

    let header = TableRow::new(headings.iter().copied());
    let rows = view.regions[window_start..window_end]
        .iter()
        .map(|region| {
            TableRow::new(
                columns
                    .iter()
                    .map(|column| {
                        Cell::from(column.value(region, &view.layout)).style(column.style(region))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();

    let title = format!(
        "区域执行计划 · {}/{}",
        if view.regions.is_empty() {
            0
        } else {
            selected + 1
        },
        view.regions.len()
    );
    let table = crate::tui::ui::data_table(&title, header, rows, widths, focused)
        .column_spacing(layout.column_spacing().min(u16::MAX as usize) as u16);
    let mut table_state = TableState::default();
    if !view.regions.is_empty() {
        table_state.select(Some(selected.saturating_sub(window_start)));
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn project_rows(
    view: &ProvisionConfirmationViewModel,
    start: usize,
    end: usize,
    columns: &[PlanColumn],
) -> Vec<Vec<String>> {
    view.regions[start..end]
        .iter()
        .map(|region| {
            columns
                .iter()
                .map(|column| column.value(region, &view.layout))
                .collect()
        })
        .collect()
}

fn table_total_width(
    view: &ProvisionConfirmationViewModel,
    start: usize,
    end: usize,
    columns: &[PlanColumn],
) -> usize {
    let headings = columns
        .iter()
        .map(|column| column.heading())
        .collect::<Vec<_>>();
    let values = project_rows(view, start, end, columns);
    let measured = content_widths(&headings, &values);
    content_driven_layout(&headings)
        .with_column_spacing(2)
        .total_width(&measured, None)
}

fn lba_range(region: &crate::tui::state::ProvisionConfirmationRegion) -> String {
    format!(
        "LBA {}–{}",
        region.selection.start_lba,
        region.selection.end_exclusive.saturating_sub(1)
    )
}
