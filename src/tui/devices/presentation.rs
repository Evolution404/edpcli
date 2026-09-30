use super::*;
use crate::tui::state::DeviceInfoNodeKey;

pub(super) fn device_detail_lines(
    state: &AppState,
    row: &crate::disk_scan::Row,
    key: DeviceInfoNodeKey,
    width: usize,
) -> Vec<Line<'static>> {
    match key {
        DeviceInfoNodeKey::Identity | DeviceInfoNodeKey::Protocol => {
            identity_protocol_detail_lines(row)
        }
        DeviceInfoNodeKey::Capacity => capacity_detail_lines(row, key, width),
        DeviceInfoNodeKey::TailGroup => tail_detail_lines(row, key, width),
        DeviceInfoNodeKey::LayoutSegment { start_lba, kind } => {
            segment_detail_lines(row, key, start_lba, kind, width)
        }
        DeviceInfoNodeKey::Status | DeviceInfoNodeKey::Backups => {
            status_backup_detail_lines(state, row, width)
        }
    }
}

fn identity_protocol_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
    let cells = identity.display_cells();
    let (reliability, basis) = crate::application::identity::device_identity_reliability(row);
    let reliability_style = crate::tui::theme::current().identity_reliability(reliability);
    vec![
        section_line("硬件"),
        field_line("设备", format!("disk{}", row.disk)),
        field_line("型号", safe(&cells[2])),
        field_line("容量", format_bytes(row.size)),
        field_line("接口", safe(&row.proto)),
        field_line("VID:PID", safe(&cells[1])),
        field_line(
            "序列号",
            safe(&crate::application::identity::device_hardware_serial(row)),
        ),
        Line::from(""),
        section_line("EDP 协议"),
        styled_field_line(
            "盘型",
            safe(&cells[6]),
            row.confirmed_provision_kind()
                .map(|kind| crate::tui::theme::current().provision_kind(kind))
                .unwrap_or_else(warning),
        ),
        field_line("onlyid", safe(identity.onlyid.as_deref().unwrap_or("—"))),
        field_line(
            "device_id",
            safe(identity.device_id.as_deref().unwrap_or("—")),
        ),
        field_line("部门", safe(row.dept.as_deref().unwrap_or("—"))),
        field_line("姓名", safe(row.user.as_deref().unwrap_or("—"))),
        field_line("标签", safe(row.label.as_deref().unwrap_or("—"))),
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
        Line::from(""),
        section_line("身份可靠性"),
        styled_field_line(
            "可靠性",
            format!("● {}", reliability.label()),
            reliability_style,
        ),
        field_line("依据", basis),
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
    let mut lines = capacity_map_lines(&model, key, width);
    lines.push(Line::from(""));
    lines.extend(crate::tui::disk_region_list::disk_region_list_lines(&model));
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
    let selection = active
        .as_ref()
        .map(|active| crate::tui::disk_layout::DiskCapacitySelection {
            start_lba: active.start_lba,
            end_exclusive: active.end_exclusive,
            kind: active.kind,
        });
    let map = crate::tui::disk_layout::DiskCapacityMap::new(
        model,
        crate::tui::disk_layout::DiskCapacityMapProfile::Full,
    )
    .with_tail(crate::tui::disk_layout::TailExpansion::Collapsed)
    .with_selection(selection)
    .with_marker(true);
    let map_width = width.max(model.collapsed_tail_model().segments.len().max(1));

    let mut lines = vec![section_line("全盘容量地图")];
    lines.extend(map.lines(width));
    lines.extend(disk_map_selection_card_lines(
        model,
        active.as_ref(),
        map_width,
    ));
    lines
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
    kind: crate::tui::disk_layout::DiskRegionKind,
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
            kind: crate::tui::disk_layout::DiskRegionKind::Tail,
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
                        kind: segment.kind,
                        label: segment.label.clone(),
                    })
            }),
        _ => None,
    }
}

fn status_backup_detail_lines(
    _state: &AppState,
    row: &crate::disk_scan::Row,
    _width: usize,
) -> Vec<Line<'static>> {
    let status_style = if row.probe_error.is_some() {
        danger()
    } else if row.denied {
        warning()
    } else {
        success()
    };
    let mut lines = vec![
        section_line("设备状态"),
        styled_field_line("总体状态", device_status(row), status_style),
        field_line(
            "容量布局",
            if row.canonical_layout().is_ok() {
                "完整"
            } else {
                "证据不足"
            },
        ),
        field_line(
            "协议读取",
            if row.identity_pin.is_some() {
                "完整"
            } else {
                "未完整读取"
            },
        ),
        field_line(
            "LCE",
            if row.lce.is_some() {
                "已确认"
            } else {
                "未确认"
            },
        ),
    ];
    if let Some(error) = &row.probe_error {
        lines.push(Line::from(Span::styled(
            format!("读取异常：{}", safe(error)),
            danger(),
        )));
    } else if row.denied {
        lines.push(Line::from(Span::styled(
            "需要管理员权限读取原始设备。",
            warning(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(section_line("关联备份"));
    let mut counts = Vec::new();
    if row.n_baks > 0 {
        counts.push(Span::styled(format!("● {} 份确认", row.n_baks), success()));
    }
    if row.n_possible_baks > 0 {
        if !counts.is_empty() {
            counts.push(Span::styled("  ·  ", muted()));
        }
        counts.push(Span::styled(
            format!("▲ {} 份疑似", row.n_possible_baks),
            warning(),
        ));
    }
    if counts.is_empty() {
        counts.push(Span::styled("暂无备份", muted()));
    }
    lines.push(Line::from(counts));
    lines
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

fn styled_field_line(
    label: &'static str,
    value: impl Into<String>,
    value_style: Style,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(crate::ui::pad_to(label, 14), muted()),
        Span::styled(value.into(), value_style.add_modifier(Modifier::BOLD)),
    ])
}

fn format_bytes(bytes: u64) -> String {
    crate::common::fmt_capacity(bytes)
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
