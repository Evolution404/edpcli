//! CLI presentation for read-only device scan rows.

use crate::common::{fmt_capacity, group_digits};
use crate::disk_scan::Row;
use crate::provision::DiskProvisionKind;

pub fn print_disk_table(rows: &[Row]) -> String {
    use crate::ui::{dim, render_table, truncate_mid, TableCell, Tone};
    let mut out = String::new();
    if rows.is_empty() {
        out.push_str("未检测到外接盘。\n");
        return out;
    }
    out.push_str(&format!("外接盘 {} 个:\n", rows.len()));
    let table_rows = rows
        .iter()
        .map(|row| {
            let (status, tone) = if row.proto != "USB" {
                ("非 USB / 不支持".to_string(), Tone::Dim)
            } else if row.denied {
                ("需管理员权限才能识别".to_string(), Tone::Dim)
            } else if let Some(error) = &row.probe_error {
                (format!("读取异常: {}", error), Tone::Yellow)
            } else if let Some(kind) = row.confirmed_provision_kind() {
                (
                    kind.short_name().to_string(),
                    if kind == DiskProvisionKind::Plain {
                        Tone::Dim
                    } else {
                        Tone::Green
                    },
                )
            } else {
                ("未知 / 未确认".to_string(), Tone::Yellow)
            };
            vec![
                TableCell::left(format!("disk{}", row.disk), Tone::Bold),
                TableCell::right(fmt_capacity(row.size), Tone::Magenta),
                TableCell::left(
                    row.proto.clone(),
                    if row.proto == "USB" {
                        Tone::Green
                    } else {
                        Tone::Dim
                    },
                ),
                TableCell::left(format!("{}:{}", row.vid, row.pid), Tone::Yellow),
                TableCell::left(
                    row.user
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .map(|value| truncate_mid(value, 14))
                        .unwrap_or_else(|| "—".to_string()),
                    if row.user.is_some() {
                        Tone::Cyan
                    } else {
                        Tone::Dim
                    },
                ),
                TableCell::left(
                    row.dept
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .map(|value| truncate_mid(value, 28))
                        .unwrap_or_else(|| "—".to_string()),
                    if row.dept.is_some() {
                        Tone::Cyan
                    } else {
                        Tone::Dim
                    },
                ),
                TableCell::left(status, tone),
            ]
        })
        .collect::<Vec<_>>();
    out.push_str(&render_table(
        &["设备", "容量", "总线", "VID:PID", "姓名", "部门", "盘型"],
        &table_rows,
    ));

    for row in rows {
        if row.proto == "USB" && !row.denied && row.probe_error.is_none() && row.device_id.is_some()
        {
            let mut details = Vec::new();
            if let Some(parts) = &row.partitions {
                let items: Vec<String> = parts
                    .iter()
                    .map(|part| {
                        format!(
                            "{} {} (LBA {}~{})",
                            part.type_name(),
                            fmt_capacity(part.size_bytes),
                            group_digits(part.start_lba),
                            group_digits(part.end_lba())
                        )
                    })
                    .collect();
                details.push(format!("└─ EDPF: {}", items.join(" · ")));
            }
            let mut meta = Vec::new();
            if let Some(onlyid) = &row.onlyid {
                meta.push(format!("onlyid={}", onlyid));
            }
            meta.push(match (row.n_baks, row.n_possible_baks) {
                (0, 0) => "无备份".to_string(),
                (confirmed, 0) => format!("备份 {confirmed} 份"),
                (0, possible) => format!("可能相关 {possible} 份"),
                (confirmed, possible) => {
                    format!("备份 {confirmed} 份 · 可能相关 {possible} 份")
                }
            });
            details.push(format!("   {}", meta.join(" · ")));
            out.push_str(&format!(
                "  {}\n",
                crate::ui::bold(&format!("disk{} 详情", row.disk))
            ));
            for detail in details {
                out.push_str(&format!("    {}\n", dim(&detail)));
            }
        }
    }
    out
}
