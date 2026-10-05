//! Canonical cell projections and semantic sorting; no geometry or rendering.
use super::{
    display_width, table_column_schema, ColumnId, SortDirection, TableColumnSpec,
    TableInteractionState, TableKind,
};

#[derive(Debug, Clone, Default)]
pub struct TableViewData {
    pub generation: u64,
    pub rows: Vec<Vec<String>>,
    pub content_widths: Vec<usize>,
}

impl TableViewData {
    pub(crate) fn from_rows(
        generation: u64,
        columns: &[TableColumnSpec],
        rows: Vec<Vec<String>>,
    ) -> Self {
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
    if lower.contains("tib") {
        value *= 1_099_511_627_776.0;
    } else if lower.contains("tb") {
        value *= 1_000_000_000_000.0;
    } else if lower.contains("gib") {
        value *= 1_073_741_824.0;
    } else if lower.contains("gb") {
        value *= 1_000_000_000.0;
    } else if lower.contains("mib") {
        value *= 1_048_576.0;
    } else if lower.contains("mb") {
        value *= 1_000_000.0;
    } else if lower.contains("kib") {
        value *= 1_024.0;
    } else if lower.contains("kb") {
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
    backup.health().label()
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
                        ColumnId::Reliability => {
                            crate::application::identity::device_identity_reliability(row)
                                .0
                                .label()
                                .into()
                        }
                        ColumnId::VidPid => cells[1].clone(),
                        ColumnId::Model => cells[2].clone(),
                        ColumnId::Serial => {
                            crate::application::identity::device_hardware_serial(row)
                        }
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

pub fn related_backup_table_view(
    backups: &[crate::application::BackupWorkspaceItem],
    related: &[(usize, crate::application::media_identity::BackupAffinity)],
) -> TableViewData {
    use crate::application::media_identity::BackupAffinity;
    let columns = table_column_schema(TableKind::RelatedBackups).expect("related backup schema");
    let projected = related
        .iter()
        .filter_map(|(source, affinity)| backups.get(*source).map(|backup| (backup, affinity)))
        .map(|(backup, affinity)| {
            let identity = crate::application::identity::WorkspaceIdentity::from_backup(backup);
            let cells = identity.display_cells();
            columns
                .iter()
                .map(|column| {
                    safe(&match column.id {
                        ColumnId::Relation => match affinity {
                            BackupAffinity::Confirmed => "● 确认".into(),
                            BackupAffinity::Possible => "▲ 疑似".into(),
                            BackupAffinity::Unrelated => "—".into(),
                        },
                        ColumnId::Time => backup.display_time.clone(),
                        ColumnId::Capacity => cells[0].clone(),
                        ColumnId::VidPid => cells[1].clone(),
                        ColumnId::Model => cells[2].clone(),
                        ColumnId::Onlyid => cells[3].clone(),
                        ColumnId::User => cells[4].clone(),
                        ColumnId::Dept => cells[5].clone(),
                        ColumnId::ProvisionKind => cells[6].clone(),
                        ColumnId::Name => backup.file_name.clone(),
                        _ => unreachable!("related backup schema"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    TableViewData::from_rows(0, &columns, projected)
}
