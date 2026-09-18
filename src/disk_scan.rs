//! 外接盘发现、cems 只读探测与列表渲染。
//!
//! 这里集中平台设备信息 + LBA4/7/12 的只读探测逻辑，顶层 CLI 只负责路由。

use std::io;
use std::path::Path;

use crate::common::{fmt_gb, group_digits, EdpCliError, EXIT_IO, SECTOR};
use crate::diskio::{self, find_backups, DiskFacts};
use crate::identify::identify;
use crate::inspect::InspectMeta;
use crate::metainfo;
use crate::sectors::{looks_nopwd, parse_lba12, EdpfPartition};
use crate::sysinfo::{self, CmdRunner};

pub struct Row {
    pub disk: u32,
    pub size: u64,
    pub vid: String,
    pub pid: String,
    pub proto: String,
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub dept: Option<String>,
    pub user: Option<String>,
    pub n_baks: usize,
    pub denied: bool,
    pub probe_error: Option<String>,
    pub is_nopwd: bool,
    pub partitions: Option<Vec<EdpfPartition>>,
}

/// 外接盘一览数据: 编号/容量/接口; USB 盘再尽力识别 cems 身份、免密状态、
/// EDPF 分区与备份份数。权限不足和读取异常分开记录。
pub fn scan_disks(
    runner: &dyn CmdRunner,
    backup_dir: &Path,
    read_disk: &dyn Fn(u32, u32) -> io::Result<Vec<u8>>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for d in sysinfo::list_external_disks(runner) {
        let mut row = Row {
            disk: d.n,
            size: d.size,
            vid: d.vid.clone(),
            pid: d.pid.clone(),
            proto: d.proto.clone(),
            device_id: None,
            onlyid: None,
            dept: None,
            user: None,
            n_baks: 0,
            denied: false,
            probe_error: None,
            is_nopwd: false,
            partitions: None,
        };
        if d.proto == "USB" {
            let probe = (|| -> io::Result<()> {
                let read_exact = |lba: u32| -> io::Result<Vec<u8>> {
                    let data = read_disk(d.n, lba)?;
                    if data.len() != SECTOR {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            format!(
                                "disk{} LBA{} 读取 {}B，预期 {}B",
                                d.n,
                                lba,
                                data.len(),
                                SECTOR
                            ),
                        ));
                    }
                    Ok(data)
                };
                let lba7 = read_exact(7)?;
                let id = identify(runner, d.n, &lba7);
                row.device_id = id.device_id.clone();
                let lba4 = read_exact(4)?;
                row.onlyid = diskio::lba4_label_id_from(&lba4);
                if let Some(did) = &id.device_id {
                    if let Ok(lba8) = read_exact(8) {
                        let meta = InspectMeta {
                            device_id: Some(did.clone()),
                            vid: Some(d.vid.clone()),
                            pid: Some(d.pid.clone()),
                            size_bytes: Some(d.size),
                            onlyid: row.onlyid.clone(),
                        };
                        if let Some(ownership) = metainfo::ownership_from_lba8(&lba8, &meta) {
                            row.dept = ownership.dept;
                            row.user = ownership.user;
                        }
                    }
                    let read = |lba: u32| {
                        read_exact(lba)
                            .map_err(|e| EdpCliError::new(EXIT_IO, format!("错误: {}", e)))
                    };
                    row.is_nopwd = looks_nopwd(&read, did).map_err(|e| io::Error::other(e.msg))?;
                    let lba12 = read_exact(12)?;
                    row.partitions = parse_lba12(&lba12, did);
                    let tag = diskio::lba4_tag16_from(&lba4).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::UnexpectedEof, "LBA4 缺少 16B 身份标签")
                    })?;
                    let facts = DiskFacts {
                        disk: d.n,
                        total_sectors: sysinfo::disk_total_sectors(runner, d.n),
                        vid: d.vid.clone(),
                        pid: d.pid.clone(),
                        label_id: row.onlyid.clone(),
                    };
                    row.n_baks = find_backups(backup_dir, &facts, Some(did), Some(tag)).len();
                }
                Ok(())
            })();
            if let Err(e) = probe {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    row.denied = true;
                } else {
                    row.probe_error = Some(e.to_string());
                }
            }
        }
        rows.push(row);
    }
    rows
}

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
            } else if row.device_id.is_none() {
                ("非 cems 盘".to_string(), Tone::Dim)
            } else if row.is_nopwd {
                ("cems盘 [免密]".to_string(), Tone::Green)
            } else {
                ("cems盘".to_string(), Tone::Plain)
            };
            vec![
                TableCell::left(format!("disk{}", row.disk), Tone::Bold),
                TableCell::right(fmt_gb(row.size), Tone::Magenta),
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
        &["设备", "容量", "总线", "VID:PID", "姓名", "部门", "状态"],
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
                            fmt_gb(part.size_bytes),
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
            meta.push(if row.n_baks > 0 {
                format!("备份 {} 份", row.n_baks)
            } else {
                "无备份".to_string()
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
