use super::*;
use crate::tui::state::DeviceInfoNodeKey;

pub(super) fn device_detail_lines(
    state: &AppState,
    row: &crate::disk_scan::Row,
    key: DeviceInfoNodeKey,
    width: usize,
) -> Vec<Line<'static>> {
    match key {
        DeviceInfoNodeKey::Identity => identity_detail_lines(row),
        DeviceInfoNodeKey::Capacity => capacity_detail_lines(state, row, width),
        DeviceInfoNodeKey::TailGroup => tail_detail_lines(row),
        DeviceInfoNodeKey::LayoutSegment { start_lba, kind } => {
            segment_detail_lines(row, start_lba, kind)
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
    state: &AppState,
    row: &crate::disk_scan::Row,
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
    let presentation = crate::tui::disk_layout::DiskLayoutPresentation::new(
        &collapsed,
        crate::tui::disk_layout::DiskLayoutProfile::CompactHuman,
        crate::tui::disk_layout::TailExpansion::Collapsed,
    );
    let mut lines = vec![
        section_line("全盘布局"),
        presentation.bar_line(width.max(8)),
    ];
    lines.extend(presentation.compact_grid_lines(width.max(8)));
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
        lines.push(Line::from(format!(
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
        )));
    }
    if state
        .device_info_tree_rows()
        .iter()
        .any(|node| node.key == DeviceInfoNodeKey::TailGroup)
    {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "尾部区域可直接在左侧按 o 展开，不需要进入深度检查。",
            muted(),
        )));
    }
    lines
}

fn tail_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let Ok(model) = row.canonical_layout() else {
        return vec![Line::from(Span::styled("尾部布局证据不足", warning()))];
    };
    let Some(tail) = model.tail_group() else {
        return vec![Line::from("当前介质没有独立尾部区域。")];
    };
    let mut lines = vec![
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
    ];
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
    start_lba: u64,
    kind: crate::disk_layout::DiskRegionKind,
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
    vec![
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
    ]
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
