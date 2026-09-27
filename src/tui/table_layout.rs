//! One cell-width aware column allocator and horizontal viewport for TUI tables.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncatePolicy {
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TableKind {
    Devices,
    Backups,
    ProvisionDevices,
    ProvisionMenu,
    InspectFields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnId {
    Device,
    Selected,
    Index,
    Name,
    Time,
    Capacity,
    VidPid,
    Model,
    Onlyid,
    User,
    Dept,
    ProvisionKind,
    Bus,
    State,
    Backups,
    Health,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableColumnSpec {
    pub id: ColumnId,
    pub heading: &'static str,
    pub layout: AdaptiveColumnSpec,
    pub copyable: bool,
}

fn table_column(
    id: ColumnId,
    heading: &'static str,
    layout: AdaptiveColumnSpec,
) -> TableColumnSpec {
    TableColumnSpec {
        id,
        heading,
        layout,
        copyable: true,
    }
}

fn table_control_column(
    id: ColumnId,
    heading: &'static str,
    layout: AdaptiveColumnSpec,
) -> TableColumnSpec {
    TableColumnSpec {
        id,
        heading,
        layout,
        copyable: false,
    }
}

pub fn identity_column_specs() -> [TableColumnSpec; 7] {
    use ColumnId::*;
    [
        table_column(Capacity, "容量", column(8, 9, 12, 80, 1, false)),
        table_column(VidPid, "VID:PID", column(9, 9, 12, 75, 1, false)),
        table_column(Model, "型号", column(10, 17, 28, 50, 2, false)),
        table_column(Onlyid, "onlyid", column(8, 12, 20, 70, 1, false)),
        table_column(User, "姓名", column(6, 10, 20, 45, 1, false)),
        table_column(Dept, "部门", column(8, 16, 40, 20, 3, false)),
        table_column(ProvisionKind, "盘型", column(12, 20, 26, 95, 1, true)),
    ]
}

pub fn table_column_schema(kind: TableKind) -> Option<Vec<TableColumnSpec>> {
    use ColumnId::*;
    let identity = identity_column_specs();
    match kind {
        TableKind::Devices => Some(vec![
            table_column(Device, "设备", column(7, 9, 12, 100, 1, true)),
            table_column(Capacity, "容量", column(8, 9, 12, 96, 1, true)),
            table_column(Dept, "部门", column(8, 16, 32, 94, 2, true)),
            table_column(User, "姓名", column(6, 10, 18, 93, 1, true)),
            table_column(ProvisionKind, "盘型", column(12, 22, 30, 98, 2, true)),
            table_column(State, "状态", column(8, 12, 18, 97, 1, true)),
            table_column(Backups, "备份", column(4, 6, 8, 70, 1, false)),
            table_column(Model, "型号", column(10, 18, 32, 45, 2, false)),
        ]),
        TableKind::Backups => {
            let identity_column = |id| {
                *identity
                    .iter()
                    .find(|column| column.id == id)
                    .expect("backup identity column")
            };
            Some(vec![
                table_control_column(Selected, "选", column(3, 3, 4, 99, 1, true)),
                table_column(Index, "序号", column(4, 6, 8, 90, 1, true)),
                table_column(Time, "时间", column(12, 17, 20, 25, 1, false)),
                identity_column(Capacity),
                identity_column(Dept),
                identity_column(User),
                identity_column(Model),
                identity_column(ProvisionKind),
                table_column(Health, "健康", column(8, 11, 15, 97, 1, true)),
                identity_column(VidPid),
                identity_column(Onlyid),
                table_column(Name, "名称", column(10, 23, 48, 96, 2, true)),
            ])
        }
        _ => None,
    }
}

pub fn table_column_copyable(kind: TableKind, logical_column: usize) -> bool {
    table_column_schema(kind)
        .and_then(|columns| columns.get(logical_column).copied())
        .is_none_or(|column| column.copyable)
}

fn normalize_copied_cell(value: &str) -> String {
    value.replace(['\t', '\r', '\n'], " ")
}

pub fn copy_cell_value(
    kind: TableKind,
    logical_column: usize,
    values: &[String],
) -> Option<String> {
    table_column_copyable(kind, logical_column)
        .then(|| values.get(logical_column))
        .flatten()
        .map(|value| normalize_copied_cell(value))
}

pub fn copy_row_values(kind: TableKind, order: &[usize], values: &[String]) -> String {
    order
        .iter()
        .copied()
        .filter(|logical| table_column_copyable(kind, *logical))
        .filter_map(|logical| values.get(logical))
        .map(|value| normalize_copied_cell(value))
        .collect::<Vec<_>>()
        .join("\t")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Ascending => "↑",
            Self::Descending => "↓",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSort {
    pub column: usize,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TableInteractionState {
    scroll_x: usize,
    active_column: usize,
    sort: Option<TableSort>,
}

impl TableInteractionState {
    pub const fn viewport_offset(self) -> usize {
        self.scroll_x
    }

    pub const fn offset(self) -> usize {
        self.scroll_x
    }

    pub fn set_offset(&mut self, offset: usize, _layout: &AdaptiveTableLayout) {
        self.scroll_x = offset;
    }

    pub const fn active_column(self) -> usize {
        self.active_column
    }

    pub const fn sort(self) -> Option<TableSort> {
        self.sort
    }

    fn normalize_columns(&mut self, layout: &AdaptiveTableLayout) {
        let count = layout.specs().len();
        self.active_column = self.active_column.min(count.saturating_sub(1));
        if self.sort.is_some_and(|sort| sort.column >= count) {
            self.sort = None;
        }
    }

    pub fn move_active(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let count = layout.specs().len();
        if count == 0 {
            return false;
        }
        let next = if reverse {
            self.active_column.saturating_sub(1)
        } else {
            self.active_column.saturating_add(1).min(count - 1)
        };
        let changed = next != self.active_column;
        if !changed {
            return false;
        }
        self.active_column = next;
        self.ensure_active_visible(layout, content_widths, viewport_width, reverse);
        true
    }

    pub fn move_active_edge(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        last: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let count = layout.specs().len();
        if count == 0 {
            return false;
        }
        let next = if last { count - 1 } else { 0 };
        let changed = next != self.active_column;
        self.active_column = next;
        self.ensure_active_visible(layout, content_widths, viewport_width, !last);
        changed
    }

    fn ensure_active_visible(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) {
        let viewport_width = usize::from(viewport_width.max(1));
        let (start, end) =
            layout.column_span(content_widths, Some(self.active_column), self.active_column);
        let current_end = self.scroll_x.saturating_add(viewport_width);
        if start < self.scroll_x {
            self.scroll_x = start;
        } else if end > current_end {
            self.scroll_x = if end.saturating_sub(start) > viewport_width && reverse {
                start
            } else {
                end.saturating_sub(viewport_width)
            };
        }
        self.scroll_x = self.scroll_x.min(layout.max_scroll(
            content_widths,
            Some(self.active_column),
            viewport_width as u16,
        ));
    }

    pub fn scroll_viewport(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let max_scroll =
            layout.max_scroll(content_widths, Some(self.active_column), viewport_width);
        let next = if reverse {
            self.scroll_x.saturating_sub(2)
        } else {
            self.scroll_x.saturating_add(2).min(max_scroll)
        };
        let changed = next != self.scroll_x;
        self.scroll_x = next;
        changed
    }

    pub fn toggle_sort_for(&mut self, logical_column: usize) {
        self.sort = Some(match self.sort {
            Some(TableSort {
                column,
                direction: SortDirection::Ascending,
            }) if column == logical_column => TableSort {
                column,
                direction: SortDirection::Descending,
            },
            _ => TableSort {
                column: logical_column,
                direction: SortDirection::Ascending,
            },
        });
    }

    pub fn toggle_sort(&mut self) {
        self.toggle_sort_for(self.active_column);
    }

    pub fn set_active_column(&mut self, column: usize) {
        self.active_column = column;
    }

    pub fn ensure_active_visible_for_layout(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) {
        self.ensure_active_visible(layout, content_widths, viewport_width, reverse);
    }

    pub fn clear_sort(&mut self) -> bool {
        let changed = self.sort.is_some();
        self.sort = None;
        changed
    }
}

#[derive(Debug, Clone, Default)]
pub struct TableViewData {
    pub generation: u64,
    pub rows: Vec<Vec<String>>,
    pub content_widths: Vec<usize>,
}

impl TableViewData {
    fn from_rows(generation: u64, columns: &[TableColumnSpec], rows: Vec<Vec<String>>) -> Self {
        let mut content_widths = columns
            .iter()
            .map(|column| display_width(column.heading))
            .collect::<Vec<_>>();
        for row in &rows {
            assert_eq!(
                row.len(),
                columns.len(),
                "table projection/schema arity mismatch"
            );
            for (index, value) in row.iter().enumerate() {
                content_widths[index] = content_widths[index].max(display_width(value));
            }
        }
        Self {
            generation,
            rows,
            content_widths,
        }
    }

    pub fn sorted_indices(
        &self,
        mut indices: Vec<usize>,
        interaction: TableInteractionState,
    ) -> Vec<usize> {
        let Some(sort) = interaction.sort() else {
            return indices;
        };
        indices.sort_by(|left, right| {
            let a = self
                .rows
                .get(*left)
                .and_then(|row| row.get(sort.column))
                .map(String::as_str)
                .unwrap_or("");
            let b = self
                .rows
                .get(*right)
                .and_then(|row| row.get(sort.column))
                .map(String::as_str)
                .unwrap_or("");
            let ordering = smart_cell_cmp(a, b).then_with(|| left.cmp(right));
            match sort.direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            }
        });
        indices
    }
}

fn semantic_rank(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase();
    if lower.contains("普通盘") || lower == "plain" {
        Some(0)
    } else if lower.contains("mode0") {
        Some(10)
    } else if lower.contains("mode1") {
        Some(11)
    } else if lower.contains("mode2") {
        Some(12)
    } else if lower.contains("mode3") {
        Some(13)
    } else if value.contains("可用") {
        Some(20)
    } else if value.contains("需权限") || value.contains("管理员权限") {
        Some(21)
    } else if value.contains("读取异常") {
        Some(22)
    } else if value.contains("非 USB") {
        Some(23)
    } else {
        None
    }
}

fn numeric_cell(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if let Some(rest) = trimmed.strip_prefix("disk") {
        return rest.parse::<f64>().ok();
    }
    let number = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == '-')
        .collect::<String>();
    if number.is_empty() || number == "-" {
        return None;
    }
    let mut value = number.parse::<f64>().ok()?;
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("tib") || lower.contains("tb") {
        value *= 1_000_000_000_000.0;
    } else if lower.contains("gib") || lower.contains("gb") {
        value *= 1_000_000_000.0;
    } else if lower.contains("mib") || lower.contains("mb") {
        value *= 1_000_000.0;
    } else if lower.contains("kib") || lower.contains("kb") {
        value *= 1_000.0;
    }
    Some(value)
}

pub(crate) fn smart_cell_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    if let (Some(a), Some(b)) = (semantic_rank(a), semantic_rank(b)) {
        return a.cmp(&b);
    }
    if let (Some(a), Some(b)) = (numeric_cell(a), numeric_cell(b)) {
        return a.total_cmp(&b);
    }
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn safe(value: &str) -> String {
    crate::ui::sanitize_terminal_text(value)
}

pub fn backup_health_text(backup: &crate::application::BackupWorkspaceItem) -> &'static str {
    if !backup.size_ok {
        "大小异常"
    } else if backup.integrity_status == crate::application::BackupIntegrityStatus::Verified {
        "EDPB ✓"
    } else {
        "EDPB ✗"
    }
}

pub fn device_table_view(rows: &[crate::disk_scan::Row], generation: u64) -> TableViewData {
    let columns = table_column_schema(TableKind::Devices).expect("device column schema");
    let projected = rows
        .iter()
        .map(|row| {
            let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
            let cells = identity.display_cells();
            columns
                .iter()
                .map(|column| {
                    safe(&match column.id {
                        ColumnId::Device => format!("disk{}", row.disk),
                        ColumnId::Capacity => cells[0].clone(),
                        ColumnId::VidPid => cells[1].clone(),
                        ColumnId::Model => cells[2].clone(),
                        ColumnId::Onlyid => cells[3].clone(),
                        ColumnId::User => cells[4].clone(),
                        ColumnId::Dept => cells[5].clone(),
                        ColumnId::ProvisionKind => cells[6].clone(),
                        ColumnId::Bus => row.proto.clone(),
                        ColumnId::State => {
                            if row.proto != "USB" {
                                "非 USB".into()
                            } else if row.denied {
                                "需权限".into()
                            } else if row.probe_error.is_some() {
                                "读取异常".into()
                            } else {
                                "可用".into()
                            }
                        }
                        ColumnId::Backups => row.n_baks.to_string(),
                        _ => unreachable!("device schema"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    TableViewData::from_rows(generation, &columns, projected)
}

pub fn backup_table_view(
    rows: &[crate::application::BackupWorkspaceItem],
    generation: u64,
) -> TableViewData {
    let columns = table_column_schema(TableKind::Backups).expect("backup column schema");
    let projected = rows
        .iter()
        .map(|row| {
            let identity = crate::application::identity::WorkspaceIdentity::from_backup(row);
            let cells = identity.display_cells();
            columns
                .iter()
                .map(|column| {
                    safe(&match column.id {
                        ColumnId::Selected => String::new(),
                        ColumnId::Index => row.index.to_string(),
                        ColumnId::Name => row.file_name.clone(),
                        ColumnId::Time => row.display_time.clone(),
                        ColumnId::Capacity => cells[0].clone(),
                        ColumnId::VidPid => cells[1].clone(),
                        ColumnId::Model => cells[2].clone(),
                        ColumnId::Onlyid => cells[3].clone(),
                        ColumnId::User => cells[4].clone(),
                        ColumnId::Dept => cells[5].clone(),
                        ColumnId::ProvisionKind => cells[6].clone(),
                        ColumnId::Health => backup_health_text(row).into(),
                        _ => unreachable!("backup schema"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    TableViewData::from_rows(generation, &columns, projected)
}

fn column(
    min: u16,
    preferred: u16,
    max: u16,
    priority: u8,
    weight: u16,
    pinned: bool,
) -> AdaptiveColumnSpec {
    AdaptiveColumnSpec {
        min_width: min,
        preferred_width: preferred,
        max_width: max,
        priority,
        weight,
        truncate_policy: TruncatePolicy::Ellipsis,
        pinned,
    }
}

pub fn layout_for(kind: TableKind) -> AdaptiveTableLayout {
    use TableKind::*;
    let specs = match kind {
        Devices | Backups => table_column_schema(kind)
            .expect("workspace tables have a column schema")
            .into_iter()
            .map(|column| column.layout)
            .collect(),
        ProvisionDevices => vec![
            column(7, 9, 12, 100, 1, true),
            column(8, 12, 14, 80, 1, false),
            column(9, 13, 20, 50, 1, false),
            column(12, 18, 25, 95, 1, true),
            column(8, 15, 24, 35, 1, false),
        ],
        ProvisionMenu => vec![
            column(3, 4, 5, 100, 1, true),
            column(12, 24, 36, 90, 1, false),
            column(16, 35, 80, 30, 3, false),
        ],
        InspectFields => vec![
            column(7, 7, 12, 100, 1, true),
            column(3, 4, 8, 95, 1, true),
            column(8, 14, 28, 35, 1, false),
            column(10, 24, 48, 90, 2, true),
            column(10, 26, 64, 85, 3, false),
            column(9, 24, 72, 40, 2, false),
            column(9, 24, 72, 30, 2, false),
            column(9, 24, 72, 25, 2, false),
            column(8, 12, 18, 25, 1, false),
            column(8, 12, 18, 70, 1, false),
            column(10, 24, 40, 20, 1, false),
        ],
    };
    AdaptiveTableLayout::new(specs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveColumnSpec {
    pub min_width: u16,
    pub preferred_width: u16,
    pub max_width: u16,
    pub priority: u8,
    pub weight: u16,
    pub truncate_policy: TruncatePolicy,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleColumn {
    pub index: usize,
    pub width: u16,
    pub full_width: usize,
    pub clip_left: usize,
    pub truncate_policy: TruncatePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableViewport {
    pub columns: Vec<VisibleColumn>,
    pub scroll_x: usize,
    pub total_width: usize,
    pub viewport_width: usize,
}

impl TableViewport {
    pub fn widths(&self) -> Vec<ratatui::layout::Constraint> {
        self.columns
            .iter()
            .map(|column| ratatui::layout::Constraint::Length(column.width))
            .collect()
    }

    pub fn position_label(&self) -> String {
        let max_scroll = self.total_width.saturating_sub(self.viewport_width);
        format!(
            "横向 {}/{}",
            self.scroll_x.min(max_scroll) + usize::from(max_scroll > 0),
            max_scroll.max(1)
        )
    }

    pub fn project<T: Clone>(&self, values: &[T]) -> Vec<T> {
        self.columns
            .iter()
            .filter_map(|column| values.get(column.index).cloned())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AdaptiveTableLayout {
    specs: Vec<AdaptiveColumnSpec>,
}

impl AdaptiveTableLayout {
    pub fn new(specs: Vec<AdaptiveColumnSpec>) -> Self {
        Self { specs }
    }

    pub fn specs(&self) -> &[AdaptiveColumnSpec] {
        &self.specs
    }

    pub fn scrollable_count(&self) -> usize {
        self.specs.len()
    }

    fn natural_widths(&self, content_widths: &[usize], active_column: Option<usize>) -> Vec<usize> {
        self.specs
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let content = content_widths.get(index).copied().unwrap_or(0);
                let base = content
                    .max(usize::from(spec.preferred_width))
                    .max(usize::from(spec.min_width.max(1)));
                if Some(index) == active_column {
                    base
                } else {
                    base.min(usize::from(spec.max_width.max(spec.min_width)))
                }
            })
            .collect()
    }

    pub fn total_width(&self, content_widths: &[usize], active_column: Option<usize>) -> usize {
        let widths = self.natural_widths(content_widths, active_column);
        widths.iter().sum::<usize>() + widths.len().saturating_sub(1)
    }

    pub fn max_scroll(
        &self,
        content_widths: &[usize],
        active_column: Option<usize>,
        viewport_width: u16,
    ) -> usize {
        self.total_width(content_widths, active_column)
            .saturating_sub(usize::from(viewport_width))
    }

    pub fn column_span(
        &self,
        content_widths: &[usize],
        active_column: Option<usize>,
        index: usize,
    ) -> (usize, usize) {
        let widths = self.natural_widths(content_widths, active_column);
        let index = index.min(widths.len().saturating_sub(1));
        let start = widths
            .iter()
            .take(index)
            .sum::<usize>()
            .saturating_add(index);
        (
            start,
            start.saturating_add(widths.get(index).copied().unwrap_or(0)),
        )
    }

    pub fn layout(&self, width: u16, content_widths: &[usize], scroll: usize) -> TableViewport {
        self.layout_with_active(width, content_widths, scroll, None)
    }

    pub fn layout_with_active(
        &self,
        width: u16,
        content_widths: &[usize],
        scroll_x: usize,
        active_column: Option<usize>,
    ) -> TableViewport {
        let viewport_width = usize::from(width);
        if self.specs.is_empty() || viewport_width == 0 {
            return TableViewport {
                columns: Vec::new(),
                scroll_x: 0,
                total_width: 0,
                viewport_width,
            };
        }

        let widths = self.natural_widths(content_widths, active_column);
        let total_width = widths.iter().sum::<usize>() + widths.len().saturating_sub(1);
        let scroll_x = scroll_x.min(total_width.saturating_sub(viewport_width));
        let viewport_end = scroll_x.saturating_add(viewport_width);

        let mut columns = Vec::new();
        let mut start = 0usize;
        for (index, full_width) in widths.iter().copied().enumerate() {
            let end = start.saturating_add(full_width);
            let visible_start = start.max(scroll_x);
            let visible_end = end.min(viewport_end);
            if visible_start < visible_end {
                columns.push(VisibleColumn {
                    index,
                    width: (visible_end - visible_start).min(u16::MAX as usize) as u16,
                    full_width,
                    clip_left: visible_start.saturating_sub(start),
                    truncate_policy: if Some(index) == active_column {
                        TruncatePolicy::Clip
                    } else {
                        self.specs[index].truncate_policy
                    },
                });
            }
            start = end.saturating_add(1);
            if start >= viewport_end {
                break;
            }
        }

        TableViewport {
            columns,
            scroll_x,
            total_width,
            viewport_width,
        }
    }
}

pub type HorizontalScrollState = TableInteractionState;

pub fn table_heading(heading: &str, index: usize, interaction: TableInteractionState) -> String {
    let marker = interaction
        .sort()
        .filter(|sort| sort.column == index)
        .map(|sort| sort.direction.marker())
        .unwrap_or("");
    if marker.is_empty() {
        heading.to_string()
    } else {
        format!("{heading} {marker}")
    }
}

pub fn table_position_label(
    layout: &AdaptiveTableLayout,
    interaction: TableInteractionState,
    _viewport: &TableViewport,
) -> String {
    let total = layout.specs().len().max(1);
    format!(
        "当前列 {}/{}",
        interaction.active_column().min(total - 1) + 1,
        total
    )
}

pub fn table_scrollbar_visibility(
    viewport: &TableViewport,
    row_total: usize,
    row_visible: usize,
) -> (bool, bool) {
    (
        viewport.total_width > viewport.viewport_width,
        row_visible > 0 && row_total > row_visible,
    )
}

pub fn render_table_scrollbars(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    viewport: &TableViewport,
    row_total: usize,
    row_start: usize,
    row_visible: usize,
) {
    use ratatui::{
        layout::Margin,
        widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
    };

    let (horizontal, vertical) = table_scrollbar_visibility(viewport, row_total, row_visible);
    if horizontal && area.width > 2 && area.height > 1 {
        let horizontal_positions = viewport
            .total_width
            .saturating_sub(viewport.viewport_width)
            .saturating_add(1);
        let mut state = ScrollbarState::new(horizontal_positions)
            .position(viewport.scroll_x)
            .viewport_content_length(viewport.viewport_width);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .thumb_symbol("━")
            .track_symbol(Some("─"))
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(crate::tui::theme::current().accent())
            .track_style(crate::tui::theme::current().muted());
        frame.render_stateful_widget(scrollbar, area.inner(Margin::new(1, 0)), &mut state);
    }

    if vertical && area.height > 2 && area.width > 1 {
        let vertical_positions = row_total.saturating_sub(row_visible).saturating_add(1);
        let mut state = ScrollbarState::new(vertical_positions)
            .position(row_start)
            .viewport_content_length(row_visible);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("┃")
            .track_symbol(Some("│"))
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(crate::tui::theme::current().accent())
            .track_style(crate::tui::theme::current().muted());
        frame.render_stateful_widget(scrollbar, area.inner(Margin::new(0, 1)), &mut state);
    }
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn visible_cell(text: &str, column: &VisibleColumn) -> String {
    let base = truncate_cell(text, column.full_width, column.truncate_policy);
    slice_display_cells(&base, column.clip_left, usize::from(column.width))
}

fn slice_display_cells(text: &str, start: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let end = start.saturating_add(width);
    let mut out = String::new();
    let mut position = 0usize;
    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        let cells = display_width(grapheme);
        let grapheme_start = position;
        let grapheme_end = position.saturating_add(cells);
        position = grapheme_end;
        if grapheme_end <= start {
            continue;
        }
        if grapheme_start >= end {
            break;
        }
        if grapheme_start >= start && grapheme_end <= end {
            out.push_str(grapheme);
        } else {
            let overlap_start = grapheme_start.max(start);
            let overlap_end = grapheme_end.min(end);
            out.push_str(&" ".repeat(overlap_end.saturating_sub(overlap_start)));
        }
    }
    out
}

pub fn truncate_cell(text: &str, width: usize, policy: TruncatePolicy) -> String {
    if display_width(text) <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    let ellipsis = if policy == TruncatePolicy::Ellipsis {
        "…"
    } else {
        ""
    };
    let target = width.saturating_sub(display_width(ellipsis));
    let mut out = String::new();
    let mut used = 0;
    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        let cells = display_width(grapheme);
        if used + cells > target {
            break;
        }
        out.push_str(grapheme);
        used += cells;
    }
    out.push_str(ellipsis);
    out
}
