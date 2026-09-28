use super::*;
use crate::tui::state::DeviceInfoNodeKey;

pub(super) fn device_detail_lines(
    row: &crate::disk_scan::Row,
    key: DeviceInfoNodeKey,
    width: usize,
) -> Vec<Line<'static>> {
    match key {
        DeviceInfoNodeKey::Identity => identity_detail_lines(row),
        DeviceInfoNodeKey::Capacity => capacity_detail_lines(row, key, width),
        DeviceInfoNodeKey::TailGroup => tail_detail_lines(row, key, width),
        DeviceInfoNodeKey::LayoutSegment { start_lba, kind } => {
            segment_detail_lines(row, key, start_lba, kind, width)
        }
        DeviceInfoNodeKey::Status => status_detail_lines(row),
        DeviceInfoNodeKey::Backups => backup_detail_lines(row),
        DeviceInfoNodeKey::Protocol => protocol_detail_lines(row),
    }
}

fn identity_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
    let cells = identity.display_cells();
    vec![
        section_line("硬件身份"),
        field_line("设备", format!("disk{}", row.disk)),
        field_line("容量", format_bytes(row.size)),
        field_line("接口", safe(&row.proto)),
        field_line("VID:PID", safe(&cells[1])),
        field_line("序列号", safe(row.serial.as_deref().unwrap_or("—"))),
        Line::from(""),
        section_line("协议身份"),
        field_line("onlyid", safe(identity.onlyid.as_deref().unwrap_or("—"))),
        field_line(
            "device_id",
            safe(identity.device_id.as_deref().unwrap_or("—")),
        ),
        field_line("部门", safe(row.dept.as_deref().unwrap_or("—"))),
        field_line("姓名", safe(row.user.as_deref().unwrap_or("—"))),
        field_line("名称", safe(row.label.as_deref().unwrap_or("—"))),
        field_line("盘型", safe(&cells[6])),
        Line::from(""),
        section_line("身份可靠性"),
        field_line("身份依据", device_identity_basis(row)),
    ]
}

fn capacity_detail_lines(
    row: &crate::disk_scan::Row,
    key: DeviceInfoNodeKey,
    width: usize,
) -> Vec<Line<'static>> {
    let Ok(model) = row.canonical_layout() else {
        let reason = row
            .canonical_layout()
            .err()
            .unwrap_or_else(|| "证据不足".into());
        return vec![Line::from(Span::styled(
            format!("无法建立可靠容量布局：{}", safe(&reason)),
            warning(),
        ))];
    };
    let collapsed = model.collapsed_tail_model();
    let mut lines = capacity_map_lines(&model, key, width);
    lines.push(Line::from(""));
    lines.push(section_line("区域列表"));
    lines.push(Line::from(Span::styled(
        format!(
            "{}  {}  {}  {}",
            crate::ui::pad_to("区域", 18),
            crate::ui::pad_to("LBA 范围", 24),
            crate::ui::pad_to("容量", 14),
            "占比"
        ),
        secondary(),
    )));
    for segment in &collapsed.segments {
        lines.push(Line::from(Span::styled(
            format!(
                "{}  {}  {}  {}",
                crate::ui::pad_to(&segment.label, 18),
                crate::ui::pad_to(&segment.closed_range(), 24),
                crate::ui::pad_to(
                    &format_bytes(
                        segment
                            .sector_count
                            .saturating_mul(crate::common::SECTOR as u64),
                    ),
                    14,
                ),
                percentage(segment.sector_count, model.total_sectors)
            ),
            crate::tui::theme::current().disk_region(segment.kind),
        )));
    }
    lines
}

fn tail_detail_lines(
    row: &crate::disk_scan::Row,
    key: DeviceInfoNodeKey,
    width: usize,
) -> Vec<Line<'static>> {
    let Ok(model) = row.canonical_layout() else {
        return vec![Line::from(Span::styled("尾部布局证据不足", warning()))];
    };
    let Some(tail) = model.tail_group() else {
        return vec![Line::from("当前介质没有独立尾部区域。")];
    };
    let mut lines = capacity_map_lines(&model, key, width);
    lines.push(Line::from(""));
    lines.extend([
        field_line(
            "LBA 范围",
            format!("[{}..{}]", tail.start_lba, tail.end_exclusive - 1),
        ),
        field_line(
            "容量",
            format_bytes(
                (tail.end_exclusive - tail.start_lba).saturating_mul(crate::common::SECTOR as u64),
            ),
        ),
        Line::from(""),
        section_line("尾部结构"),
    ]);
    for child in tail.children {
        lines.push(Line::from(format!(
            "{}  {}  {}",
            crate::ui::pad_to(&child.label, 20),
            crate::ui::pad_to(&child.closed_range(), 24),
            format_bytes(
                child
                    .sector_count
                    .saturating_mul(crate::common::SECTOR as u64),
            )
        )));
    }
    lines
}

fn segment_detail_lines(
    row: &crate::disk_scan::Row,
    key: DeviceInfoNodeKey,
    start_lba: u64,
    kind: crate::disk_layout::DiskRegionKind,
    width: usize,
) -> Vec<Line<'static>> {
    let Ok(model) = row.canonical_layout() else {
        return vec![Line::from(Span::styled("区域布局证据不足", warning()))];
    };
    let Some(segment) = model
        .segments
        .iter()
        .find(|segment| segment.start_lba == start_lba && segment.kind == kind)
    else {
        return vec![Line::from(Span::styled(
            "所选区域已不在当前设备布局中。",
            warning(),
        ))];
    };
    let mut lines = capacity_map_lines(&model, key, width);
    lines.push(Line::from(""));
    lines.extend([
        field_line("区域类型", segment.kind.label()),
        field_line("LBA 范围", segment.closed_range()),
        field_line("起始 LBA", segment.start_lba.to_string()),
        field_line(
            "结束 LBA",
            segment
                .start_lba
                .saturating_add(segment.sector_count)
                .saturating_sub(1)
                .to_string(),
        ),
        field_line("扇区数量", segment.sector_count.to_string()),
        field_line(
            "容量",
            format_bytes(
                segment
                    .sector_count
                    .saturating_mul(crate::common::SECTOR as u64),
            ),
        ),
        field_line(
            "占比",
            percentage(segment.sector_count, model.total_sectors),
        ),
        Line::from(""),
        Line::from(Span::styled(
            "需要逐扇区、十六进制或字段证据时按 i 进入深度检查。",
            muted(),
        )),
    ]);
    lines
}

fn capacity_map_lines(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    key: DeviceInfoNodeKey,
    width: usize,
) -> Vec<Line<'static>> {
    let active = active_capacity_extent(model, key);
    let visual = model.collapsed_tail_model();
    let segment_count = visual.segments.len().max(1);
    let separators = segment_count.saturating_add(1);
    let band_width = width.max(separators.saturating_add(segment_count));
    let cell_budget = band_width.saturating_sub(separators).max(segment_count);
    let allocations = disk_map_allocations(&visual, cell_budget);
    let map_width = allocations.iter().sum::<usize>() + separators;

    let last_lba = model.total_sectors.saturating_sub(1);
    let left = "LBA 0".to_string();
    let right = format!(
        "LBA {last_lba} · {}",
        format_bytes(
            model
                .total_sectors
                .saturating_mul(crate::common::SECTOR as u64)
        )
    );
    let coordinate_gap = map_width
        .saturating_sub(crate::tui::table_layout::display_width(&left))
        .saturating_sub(crate::tui::table_layout::display_width(&right));

    let mut lines = vec![
        section_line("全盘容量地图"),
        Line::from(vec![
            Span::styled(left, muted()),
            Span::raw(" ".repeat(coordinate_gap)),
            Span::styled(right, muted()),
        ]),
    ];
    lines.extend(disk_map_axis_lines(map_width));
    lines.extend([
        disk_map_border_line(&visual, &allocations, active.as_ref(), true),
        disk_map_label_line(&visual, &allocations, active.as_ref()),
        disk_map_value_line(&visual, &allocations, active.as_ref()),
        disk_map_border_line(&visual, &allocations, active.as_ref(), false),
    ]);

    if let Some(active) = active.as_ref() {
        let marker = disk_map_marker_column(&visual, &allocations, active);
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(marker)),
            Span::styled("▲", accent()),
        ]));
    } else {
        lines.push(Line::from(""));
    }

    lines.extend(disk_map_selection_card_lines(
        model,
        active.as_ref(),
        map_width,
    ));
    lines.push(Line::from(Span::styled(
        "极小区域使用最小可视宽度；LBA、容量与占比保持真实。",
        muted(),
    )));
    lines
}

fn disk_map_axis_lines(width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut labels = vec![' '; width];
    for (percent, label) in [
        (0usize, "0%"),
        (25, "25%"),
        (50, "50%"),
        (75, "75%"),
        (100, "100%"),
    ] {
        let target = width.saturating_sub(1).saturating_mul(percent) / 100;
        let label_width = label.len().min(width);
        let start = if percent == 0 {
            0
        } else if percent == 100 {
            width.saturating_sub(label_width)
        } else {
            target
                .saturating_sub(label_width / 2)
                .min(width.saturating_sub(label_width))
        };
        for (index, ch) in label.chars().take(label_width).enumerate() {
            labels[start + index] = ch;
        }
    }

    let mut ticks = vec!['┈'; width];
    for percent in [0usize, 25, 50, 75, 100] {
        let index = width.saturating_sub(1).saturating_mul(percent) / 100;
        ticks[index] = '┊';
    }
    vec![
        Line::from(Span::styled(
            labels.into_iter().collect::<String>(),
            muted(),
        )),
        Line::from(Span::styled(ticks.into_iter().collect::<String>(), muted())),
    ]
}

fn disk_map_selection_card_lines(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    active: Option<&ActiveCapacityExtent>,
    width: usize,
) -> Vec<Line<'static>> {
    let (title, body, style) = if let Some(active) = active {
        (
            "当前选中",
            format!(
                "● 当前区域：{}  │  LBA {}..{}  │  {}  │  {}",
                active.label,
                active.start_lba,
                active.end_exclusive.saturating_sub(1),
                format_bytes(
                    active
                        .end_exclusive
                        .saturating_sub(active.start_lba)
                        .saturating_mul(crate::common::SECTOR as u64)
                ),
                percentage(
                    active.end_exclusive.saturating_sub(active.start_lba),
                    model.total_sectors
                )
            ),
            accent(),
        )
    } else {
        (
            "全盘布局",
            format!(
                "{}  │  {} sectors  │  LBA 0..{}",
                format_bytes(
                    model
                        .total_sectors
                        .saturating_mul(crate::common::SECTOR as u64)
                ),
                model.total_sectors,
                model.total_sectors.saturating_sub(1)
            ),
            secondary(),
        )
    };

    let width = width.max(8);
    let top_prefix = format!("╭─ {title} ");
    let top_used = crate::tui::table_layout::display_width(&top_prefix).saturating_add(1);
    let top = format!(
        "{}{}╮",
        top_prefix,
        "─".repeat(width.saturating_sub(top_used))
    );
    let inner_width = width.saturating_sub(4);
    let clipped = crate::tui::table_layout::truncate_cell(
        &body,
        inner_width,
        crate::tui::table_layout::TruncatePolicy::Ellipsis,
    );
    let padding = inner_width.saturating_sub(crate::tui::table_layout::display_width(&clipped));
    let body_line = format!("│ {}{} │", clipped, " ".repeat(padding));
    let bottom = format!("╰{}╯", "─".repeat(width.saturating_sub(2)));

    vec![
        Line::from(Span::styled(top, muted())),
        Line::from(Span::styled(body_line, style)),
        Line::from(Span::styled(bottom, muted())),
    ]
}

#[derive(Debug, Clone)]
struct ActiveCapacityExtent {
    start_lba: u64,
    end_exclusive: u64,
    label: String,
}

fn active_capacity_extent(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    key: DeviceInfoNodeKey,
) -> Option<ActiveCapacityExtent> {
    match key {
        DeviceInfoNodeKey::TailGroup => model.tail_group().map(|tail| ActiveCapacityExtent {
            start_lba: tail.start_lba,
            end_exclusive: tail.end_exclusive,
            label: "尾部区域".into(),
        }),
        DeviceInfoNodeKey::LayoutSegment { start_lba, kind } => model
            .segments
            .iter()
            .find(|segment| segment.start_lba == start_lba && segment.kind == kind)
            .and_then(|segment| {
                segment
                    .end_exclusive()
                    .ok()
                    .map(|end_exclusive| ActiveCapacityExtent {
                        start_lba: segment.start_lba,
                        end_exclusive,
                        label: segment.label.clone(),
                    })
            }),
        _ => None,
    }
}

fn disk_map_allocations(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    cell_budget: usize,
) -> Vec<usize> {
    let count = model.segments.len();
    if count == 0 {
        return Vec::new();
    }
    let min_width: usize = if cell_budget >= count.saturating_mul(3) {
        3
    } else if cell_budget >= count.saturating_mul(2) {
        2
    } else {
        1
    };
    let mut allocations = vec![min_width; count];
    let remaining = cell_budget.saturating_sub(min_width.saturating_mul(count));
    if remaining == 0 {
        return allocations;
    }

    let total = model
        .segments
        .iter()
        .map(|segment| u128::from(segment.sector_count))
        .sum::<u128>()
        .max(1);
    let mut assigned = 0usize;
    let mut remainders = Vec::with_capacity(count);
    for (index, segment) in model.segments.iter().enumerate() {
        let scaled = u128::from(segment.sector_count) * remaining as u128;
        let extra = (scaled / total) as usize;
        allocations[index] += extra;
        assigned += extra;
        remainders.push((scaled % total, index));
    }
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_, index) in remainders
        .into_iter()
        .take(remaining.saturating_sub(assigned))
    {
        allocations[index] += 1;
    }
    allocations
}

fn disk_map_border_line(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    allocations: &[usize],
    active: Option<&ActiveCapacityExtent>,
    top: bool,
) -> Line<'static> {
    let border = ratatui::symbols::border::QUADRANT_INSIDE;
    let first_active = model
        .segments
        .first()
        .is_some_and(|segment| capacity_segment_active(segment, active));
    let mut spans = vec![Span::styled(
        if top {
            border.top_left
        } else {
            border.bottom_left
        },
        disk_map_outer_border_style(model.segments.first(), first_active),
    )];

    for (index, (segment, width)) in model
        .segments
        .iter()
        .zip(allocations.iter().copied())
        .enumerate()
    {
        let is_active = capacity_segment_active(segment, active);
        let horizontal_symbol = if top {
            border.horizontal_top
        } else {
            border.horizontal_bottom
        };
        spans.push(Span::styled(
            horizontal_symbol.repeat(width),
            disk_map_outer_border_style(Some(segment), is_active),
        ));

        if index + 1 == model.segments.len() {
            spans.push(Span::styled(
                if top {
                    border.top_right
                } else {
                    border.bottom_right
                },
                disk_map_outer_border_style(Some(segment), is_active),
            ));
        } else {
            let next = &model.segments[index + 1];
            spans.push(Span::styled(
                horizontal_symbol,
                disk_map_outer_boundary_style(segment, next, active),
            ));
        }
    }
    Line::from(spans)
}

fn disk_map_outer_border_style(
    segment: Option<&crate::tui::disk_layout::DiskLayoutSegment>,
    active: bool,
) -> ratatui::style::Style {
    let theme = crate::tui::theme::current();
    let Some(segment) = segment else {
        return muted().bg(theme.palette().background);
    };
    theme
        .disk_region_outline(segment.kind, active)
        .bg(theme.palette().background)
}

fn disk_map_outer_boundary_style(
    left: &crate::tui::disk_layout::DiskLayoutSegment,
    right: &crate::tui::disk_layout::DiskLayoutSegment,
    active: Option<&ActiveCapacityExtent>,
) -> ratatui::style::Style {
    let left_active = capacity_segment_active(left, active);
    let right_active = capacity_segment_active(right, active);
    let owner = if right_active && !left_active {
        right
    } else {
        left
    };
    let owner_active = capacity_segment_active(owner, active);
    disk_map_outer_border_style(Some(owner), owner_active)
}

fn disk_map_internal_boundary_span(
    left: &crate::tui::disk_layout::DiskLayoutSegment,
    right: &crate::tui::disk_layout::DiskLayoutSegment,
    active: Option<&ActiveCapacityExtent>,
) -> Span<'static> {
    let border = ratatui::symbols::border::QUADRANT_INSIDE;
    let theme = crate::tui::theme::current();
    let left_active = capacity_segment_active(left, active);
    let right_active = capacity_segment_active(right, active);

    if right_active && !left_active {
        let background = theme
            .disk_region_fill(left.kind, left_active)
            .bg
            .unwrap_or(theme.palette().background);
        Span::styled(
            border.vertical_left,
            theme.disk_region_outline(right.kind, true).bg(background),
        )
    } else {
        let background = theme
            .disk_region_fill(right.kind, right_active)
            .bg
            .unwrap_or(theme.palette().background);
        Span::styled(
            border.vertical_right,
            theme
                .disk_region_outline(left.kind, left_active)
                .bg(background),
        )
    }
}

fn disk_map_outer_vertical_span(
    segment: &crate::tui::disk_layout::DiskLayoutSegment,
    active: bool,
    left: bool,
) -> Span<'static> {
    let border = ratatui::symbols::border::QUADRANT_INSIDE;
    Span::styled(
        if left {
            border.vertical_left
        } else {
            border.vertical_right
        },
        disk_map_outer_border_style(Some(segment), active),
    )
}

fn disk_map_label_line(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    allocations: &[usize],
    active: Option<&ActiveCapacityExtent>,
) -> Line<'static> {
    disk_map_content_line(model, allocations, active, true)
}

fn disk_map_value_line(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    allocations: &[usize],
    active: Option<&ActiveCapacityExtent>,
) -> Line<'static> {
    disk_map_content_line(model, allocations, active, false)
}

fn disk_map_content_line(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    allocations: &[usize],
    active: Option<&ActiveCapacityExtent>,
    label_row: bool,
) -> Line<'static> {
    let Some(first) = model.segments.first() else {
        return Line::default();
    };
    let first_active = capacity_segment_active(first, active);
    let mut spans = vec![disk_map_outer_vertical_span(first, first_active, true)];

    for (index, (segment, width)) in model
        .segments
        .iter()
        .zip(allocations.iter().copied())
        .enumerate()
    {
        let is_active = capacity_segment_active(segment, active);
        let text = if label_row {
            disk_map_segment_label(segment, width)
        } else {
            disk_map_segment_value(segment, width, model.total_sectors)
        };
        spans.push(Span::styled(
            center_disk_map_label(&text, width),
            disk_map_segment_style(segment, active),
        ));

        if let Some(next) = model.segments.get(index + 1) {
            spans.push(disk_map_internal_boundary_span(segment, next, active));
        } else {
            spans.push(disk_map_outer_vertical_span(segment, is_active, false));
        }
    }
    Line::from(spans)
}

fn disk_map_segment_label(
    segment: &crate::tui::disk_layout::DiskLayoutSegment,
    width: usize,
) -> String {
    if width == 0 {
        return String::new();
    }
    if crate::tui::table_layout::display_width(&segment.label) <= width {
        return segment.label.clone();
    }
    String::new()
}

fn disk_map_segment_value(
    segment: &crate::tui::disk_layout::DiskLayoutSegment,
    width: usize,
    total_sectors: u64,
) -> String {
    if width < 8 {
        return String::new();
    }
    let capacity = format_bytes(
        segment
            .sector_count
            .saturating_mul(crate::common::SECTOR as u64),
    );
    let percent = percentage(segment.sector_count, total_sectors);
    let full = format!("{capacity} · {percent}");
    if crate::tui::table_layout::display_width(&full) <= width {
        full
    } else if crate::tui::table_layout::display_width(&capacity) <= width {
        capacity
    } else {
        String::new()
    }
}

fn disk_map_segment_style(
    segment: &crate::tui::disk_layout::DiskLayoutSegment,
    active: Option<&ActiveCapacityExtent>,
) -> ratatui::style::Style {
    let is_active = capacity_segment_active(segment, active);
    crate::tui::theme::current().disk_region_fill(segment.kind, is_active)
}

fn capacity_segment_active(
    segment: &crate::tui::disk_layout::DiskLayoutSegment,
    active: Option<&ActiveCapacityExtent>,
) -> bool {
    let Some(active) = active else {
        return false;
    };
    segment
        .end_exclusive()
        .ok()
        .is_some_and(|end| segment.start_lba < active.end_exclusive && end > active.start_lba)
}

fn center_disk_map_label(label: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let clipped = crate::tui::table_layout::truncate_cell(
        label,
        width,
        crate::tui::table_layout::TruncatePolicy::Clip,
    );
    let used = crate::tui::table_layout::display_width(&clipped);
    let left = width.saturating_sub(used) / 2;
    let right = width.saturating_sub(used).saturating_sub(left);
    format!("{}{}{}", " ".repeat(left), clipped, " ".repeat(right))
}

fn disk_map_marker_column(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    allocations: &[usize],
    active: &ActiveCapacityExtent,
) -> usize {
    let active_mid =
        u128::from(active.start_lba).saturating_add(u128::from(active.end_exclusive)) / 2;
    let mut cursor = 1usize;
    for (segment, width) in model.segments.iter().zip(allocations.iter().copied()) {
        let Ok(segment_end) = segment.end_exclusive() else {
            cursor = cursor.saturating_add(width).saturating_add(1);
            continue;
        };
        if segment.start_lba < active.end_exclusive && segment_end > active.start_lba {
            let segment_start = u128::from(segment.start_lba);
            let segment_len = u128::from(segment.sector_count).max(1);
            let relative = active_mid
                .clamp(segment_start, u128::from(segment_end).saturating_sub(1))
                .saturating_sub(segment_start);
            let offset = ((relative.saturating_mul(width as u128)) / segment_len)
                .min(width.saturating_sub(1) as u128) as usize;
            return cursor.saturating_add(offset);
        }
        cursor = cursor.saturating_add(width).saturating_add(1);
    }
    0
}

fn status_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let mut lines = vec![
        field_line("总体状态", device_status(row)),
        field_line("接口", safe(&row.proto)),
        field_line(
            "介质类型",
            row.confirmed_provision_kind()
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "未确认".into()),
        ),
        field_line(
            "容量布局",
            if row.canonical_layout().is_ok() {
                "完整"
            } else {
                "证据不足"
            },
        ),
        field_line("身份依据", device_identity_basis(row)),
        field_line("确认备份", format!("{} 份", row.n_baks)),
        field_line("可能相关", format!("{} 份", row.n_possible_baks)),
    ];
    if let Some(error) = &row.probe_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("读取异常：{}", safe(error)),
            warning(),
        )));
    } else if row.denied {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "需要管理员权限读取原始设备。",
            warning(),
        )));
    }
    lines
}

fn backup_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    vec![
        section_line("当前设备相关备份"),
        field_line("已确认", format!("{} 份", row.n_baks)),
        field_line("可能相关", format!("{} 份", row.n_possible_baks)),
        Line::from(""),
        Line::from(Span::styled(
            "本页只展示当前已加载的设备关联证据；全局备份管理请切换到“备份”Tab。",
            muted(),
        )),
    ]
}

fn protocol_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
    let cells = identity.display_cells();
    vec![
        field_line("介质类型", safe(&cells[6])),
        field_line(
            "分区记录",
            row.partitions
                .as_ref()
                .map(|parts| format!("{} 条", parts.len()))
                .unwrap_or_else(|| "未读取".into()),
        ),
        field_line(
            "LCE",
            if row.lce.is_some() {
                "已确认"
            } else {
                "未确认"
            },
        ),
        field_line("部门", safe(row.dept.as_deref().unwrap_or("—"))),
        field_line("姓名", safe(row.user.as_deref().unwrap_or("—"))),
        field_line("名称", safe(row.label.as_deref().unwrap_or("—"))),
        field_line("onlyid", safe(row.onlyid.as_deref().unwrap_or("—"))),
        Line::from(""),
        Line::from(Span::styled(
            "完整 LBA0～12 / LCE / 原始数据 / 字段解析请按 i 进入深度检查。",
            muted(),
        )),
    ]
}

fn section_line(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        label,
        secondary().add_modifier(Modifier::BOLD),
    ))
}

fn field_line(label: &'static str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(crate::ui::pad_to(label, 14), muted()),
        Span::raw(value.into()),
    ])
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.2} kB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn percentage(sectors: u64, total: u64) -> String {
    if total == 0 {
        return "0.00%".into();
    }
    let ratio = sectors as f64 * 100.0 / total as f64;
    if ratio > 0.0 && ratio < 0.01 {
        "<0.01%".into()
    } else {
        format!("{ratio:.2}%")
    }
}

fn device_identity_basis(row: &crate::disk_scan::Row) -> &'static str {
    use crate::application::media_identity::SerialQuality;

    match row
        .identity_pin
        .as_ref()
        .map(|pin| pin.snapshot.hardware.serial_quality)
    {
        Some(SerialQuality::Usable) => "强 · 硬件序列号 + VID:PID + 容量",
        Some(SerialQuality::Suspicious) => "中 · 序列号可疑，结合 VID:PID + 容量",
        Some(SerialQuality::Missing) if row.device_id.is_some() && row.onlyid.is_some() => {
            "中 · EDP device_id + onlyid + 硬件特征"
        }
        Some(SerialQuality::Missing) => "弱 · 仅硬件型号/容量等非唯一特征",
        None if row.serial.is_some() => "待确认 · 已读取序列号，身份快照未建立",
        None if row.device_id.is_some() || row.onlyid.is_some() => "待确认 · 仅协议身份可用",
        None => "未建立可靠身份依据",
    }
}
