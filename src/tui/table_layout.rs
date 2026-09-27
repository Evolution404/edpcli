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
            let mut columns = vec![
                table_column(Selected, "选", column(3, 3, 4, 99, 1, true)),
                table_column(Index, "#", column(3, 4, 6, 90, 1, true)),
                table_column(Name, "名称", column(10, 23, 48, 96, 2, true)),
                table_column(Time, "时间", column(12, 17, 20, 25, 1, false)),
            ];
            columns.extend(identity);
            columns.push(table_column(Health, "健康", column(8, 11, 15, 97, 1, true)));
            Some(columns)
        }
        _ => None,
    }
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
    viewport_offset: usize,
    active_column: usize,
    sort: Option<TableSort>,
}

impl TableInteractionState {
    pub const fn viewport_offset(self) -> usize {
        self.viewport_offset
    }

    // Compatibility shims while callers migrate to the unified table interaction API.
    pub const fn offset(self) -> usize {
        self.viewport_offset()
    }

    pub fn set_offset(&mut self, offset: usize, layout: &AdaptiveTableLayout) {
        self.set_viewport_offset(offset, layout);
    }

    pub fn left(&mut self) -> bool {
        let next = self.viewport_offset.saturating_sub(1);
        let changed = next != self.viewport_offset;
        self.viewport_offset = next;
        changed
    }

    pub fn right(&mut self, layout: &AdaptiveTableLayout) -> bool {
        self.scroll_viewport(layout, false)
    }

    pub const fn active_column(self) -> usize {
        self.active_column
    }

    pub const fn sort(self) -> Option<TableSort> {
        self.sort
    }

    pub fn normalize(&mut self, layout: &AdaptiveTableLayout) {
        let count = layout.specs().len();
        self.active_column = self.active_column.min(count.saturating_sub(1));
        self.viewport_offset = self
            .viewport_offset
            .min(layout.scrollable_count().saturating_sub(1));
        if self.sort.is_some_and(|sort| sort.column >= count) {
            self.sort = None;
        }
    }

    pub fn set_viewport_offset(&mut self, offset: usize, layout: &AdaptiveTableLayout) {
        self.viewport_offset = offset.min(layout.scrollable_count().saturating_sub(1));
    }

    pub fn move_active(&mut self, layout: &AdaptiveTableLayout, reverse: bool) -> bool {
        self.normalize(layout);
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
        self.active_column = next;

        let scrollable = layout
            .specs()
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| (!spec.pinned).then_some(index))
            .collect::<Vec<_>>();
        if let Some(position) = scrollable
            .iter()
            .position(|index| *index == self.active_column)
        {
            self.viewport_offset = position;
        }
        changed
    }

    pub fn scroll_viewport(&mut self, layout: &AdaptiveTableLayout, reverse: bool) -> bool {
        self.normalize(layout);
        let next = if reverse {
            self.viewport_offset.saturating_sub(1)
        } else {
            self.viewport_offset
                .saturating_add(1)
                .min(layout.scrollable_count().saturating_sub(1))
        };
        let changed = next != self.viewport_offset;
        self.viewport_offset = next;
        changed
    }

    pub fn toggle_sort(&mut self) {
        self.sort = Some(match self.sort {
            Some(TableSort {
                column,
                direction: SortDirection::Ascending,
            }) if column == self.active_column => TableSort {
                column,
                direction: SortDirection::Descending,
            },
            _ => TableSort {
                column: self.active_column,
                direction: SortDirection::Ascending,
            },
        });
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
    pub truncate_policy: TruncatePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableViewport {
    pub columns: Vec<VisibleColumn>,
    pub first_scrollable_column: usize,
    pub scrollable_columns: usize,
}

impl TableViewport {
    pub fn widths(&self) -> Vec<ratatui::layout::Constraint> {
        self.columns
            .iter()
            .map(|column| ratatui::layout::Constraint::Length(column.width))
            .collect()
    }

    pub fn position_label(&self) -> String {
        if self.scrollable_columns == 0 {
            "1/1 列".into()
        } else {
            format!(
                "{}/{} 列",
                self.first_scrollable_column + 1,
                self.scrollable_columns
            )
        }
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
        self.specs.iter().filter(|spec| !spec.pinned).count()
    }

    pub fn layout(&self, width: u16, content_widths: &[usize], scroll: usize) -> TableViewport {
        self.layout_with_active(width, content_widths, scroll, None)
    }

    pub fn layout_with_active(
        &self,
        width: u16,
        content_widths: &[usize],
        scroll: usize,
        active_column: Option<usize>,
    ) -> TableViewport {
        if self.specs.is_empty() || width == 0 {
            return TableViewport {
                columns: Vec::new(),
                first_scrollable_column: 0,
                scrollable_columns: self.scrollable_count(),
            };
        }
        let scrollable = self
            .specs
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| (!spec.pinned).then_some(index))
            .collect::<Vec<_>>();
        let offset = scroll.min(scrollable.len().saturating_sub(1));
        let mut selected = self
            .specs
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| spec.pinned.then_some(index))
            .collect::<Vec<_>>();
        let min_sum = |indices: &[usize]| {
            indices
                .iter()
                .map(|&index| usize::from(self.specs[index].min_width.max(1)))
                .sum::<usize>()
                + indices.len().saturating_sub(1)
        };
        while min_sum(&selected) > usize::from(width) && selected.len() > 1 {
            let removable = selected
                .iter()
                .enumerate()
                .filter(|(_, index)| **index != 0)
                .min_by_key(|(_, index)| self.specs[**index].priority)
                .map(|(position, _)| position)
                .unwrap_or(selected.len() - 1);
            selected.remove(removable);
        }
        for &index in scrollable.iter().skip(offset) {
            let mut candidate = selected.clone();
            candidate.push(index);
            if min_sum(&candidate) <= usize::from(width) {
                selected.push(index);
            } else if selected.is_empty() {
                selected.push(index);
                break;
            } else {
                break;
            }
        }
        let active = active_column.filter(|index| *index < self.specs.len());
        if let Some(active) = active {
            if !selected.contains(&active) {
                selected.push(active);
            }
        }
        if selected.is_empty() {
            selected.push(active.unwrap_or(0));
        }
        selected.sort_unstable();
        selected.dedup();

        let active_required = |index: usize| {
            if Some(index) == active {
                content_widths
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .max(usize::from(self.specs[index].min_width.max(1)))
                    .max(1)
                    .min(usize::from(width))
            } else {
                usize::from(self.specs[index].min_width.max(1))
            }
        };
        while selected.len() > 1 {
            let required = selected
                .iter()
                .map(|index| active_required(*index))
                .sum::<usize>()
                + selected.len().saturating_sub(1);
            if required <= usize::from(width) {
                break;
            }
            let removable = selected
                .iter()
                .enumerate()
                .filter(|(_, index)| Some(**index) != active)
                .min_by_key(|(_, index)| self.specs[**index].priority)
                .map(|(position, _)| position);
            let Some(removable) = removable else {
                break;
            };
            selected.remove(removable);
        }

        let spacing = selected.len().saturating_sub(1);
        let usable = usize::from(width).saturating_sub(spacing);
        let mut widths = selected
            .iter()
            .map(|&index| active_required(index))
            .collect::<Vec<_>>();
        if widths.iter().sum::<usize>() > usable {
            let mut excess = widths.iter().sum::<usize>() - usable;
            let mut order = (0..selected.len()).collect::<Vec<_>>();
            order.sort_by_key(|&position| {
                (
                    Some(selected[position]) == active,
                    self.specs[selected[position]].priority,
                )
            });
            for position in order {
                let shrink = excess.min(widths[position].saturating_sub(1));
                widths[position] -= shrink;
                excess -= shrink;
                if excess == 0 {
                    break;
                }
            }
        }
        let mut remaining = usable.saturating_sub(widths.iter().sum::<usize>());
        let mut priority_order = (0..selected.len()).collect::<Vec<_>>();
        priority_order.sort_by_key(|&position| {
            (
                Some(selected[position]) != active,
                std::cmp::Reverse(self.specs[selected[position]].priority),
            )
        });
        for &position in &priority_order {
            let index = selected[position];
            let spec = self.specs[index];
            let preferred = usize::from(spec.preferred_width)
                .max(content_widths.get(index).copied().unwrap_or(0))
                .min(usize::from(spec.max_width.max(spec.min_width)));
            let add = remaining.min(preferred.saturating_sub(widths[position]));
            widths[position] += add;
            remaining -= add;
        }
        while remaining > 0 {
            let mut progressed = false;
            for &position in &priority_order {
                let spec = self.specs[selected[position]];
                for _ in 0..spec.weight.max(1) {
                    if remaining == 0 || widths[position] >= usize::from(spec.max_width) {
                        break;
                    }
                    widths[position] += 1;
                    remaining -= 1;
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        TableViewport {
            columns: selected
                .into_iter()
                .zip(widths)
                .map(|(index, width)| VisibleColumn {
                    index,
                    width: width.min(u16::MAX as usize) as u16,
                    truncate_policy: if Some(index) == active {
                        TruncatePolicy::Clip
                    } else {
                        self.specs[index].truncate_policy
                    },
                })
                .collect(),
            first_scrollable_column: offset,
            scrollable_columns: scrollable.len(),
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
) -> String {
    let total = layout.specs().len().max(1);
    format!(
        "当前列 {}/{}",
        interaction.active_column().min(total - 1) + 1,
        total
    )
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
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
