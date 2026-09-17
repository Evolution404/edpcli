//! 外接盘发现、cems 只读探测与列表渲染。
//!
//! 这里集中 `diskutil/ioreg + LBA4/7/12` 的只读探测逻辑，顶层 CLI 只负责路由。

use std::io;
use std::path::Path;

use crate::common::{fmt_gb, group_digits, NopwdError, SECTOR, EXIT_IO};
use crate::diskio::{self, find_backups, DiskFacts};
use crate::identify::identify;
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
    pub n_baks: usize,
    pub denied: bool,
    pub probe_error: Option<String>,
    pub is_nopwd: bool,
    pub partitions: Option<Vec<EdpfPartition>>,
}

/// 外接盘一览数据: 编号/容量/接口; USB 盘再尽力识别 cems 身份、免密状态、
/// EDPF 分区与备份份数。权限不足和读取异常分开记录，避免错误提示用户去 sudo。
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
                    let read = |lba: u32| {
                        read_exact(lba)
                            .map_err(|e| NopwdError::new(EXIT_IO, format!("错误: {}", e)))
                    };
                    row.is_nopwd = looks_nopwd(&read, did)
                        .map_err(|e| io::Error::other(e.msg))?;
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
    use crate::ui::{bold, dim, green, pad_left, pad_to};
    let mut out = String::new();
    if rows.is_empty() {
        out.push_str("未检测到外接盘。\n");
        return out;
    }
    out.push_str(&format!("外接盘 {} 个:\n", rows.len()));
    let width = rows
        .iter()
        .map(|r| r.disk.to_string().len())
        .max()
        .unwrap_or(1);
    for row in rows {
        let name = pad_to(&format!("disk{}", row.disk), width + 4);
        let head = format!(
            "  {}  {}  {}  {}",
            bold(&name),
            pad_left(&fmt_gb(row.size), 8),
            pad_to(&row.proto, 12),
            pad_to(&format!("{}:{}", row.vid, row.pid), 13),
        );
        let detail_pad = " ".repeat(2 + (width + 4) + 2 + 8 + 2 + 1);
        if row.proto != "USB" {
            out.push_str(&format!("{}  {}\n", head, dim("(非USB, 本工具不支持)")));
        } else if row.denied {
            out.push_str(&format!(
                "{}  {}\n",
                head,
                dim("(加 sudo 可识别 cems 盘/备份)")
            ));
        } else if let Some(error) = &row.probe_error {
            out.push_str(&format!(
                "{}  {}\n",
                head,
                crate::ui::yellow(&format!("读取异常: {}", error))
            ));
        } else if row.device_id.is_none() {
            out.push_str(&format!("{}  {}\n", head, dim("非cems盘")));
        } else {
            let nopwd_tag = if row.is_nopwd {
                format!(" {}", green("[免密]"))
            } else {
                String::new()
            };
            out.push_str(&format!("{}  cems盘{}\n", head, nopwd_tag));
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
            for detail in details {
                out.push_str(&format!("{}{}\n", detail_pad, detail));
            }
        }
    }
    out
}
