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
    #[doc(hidden)]
    pub numeric_sort_keys: Vec<Vec<Option<u64>>>,
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
            numeric_sort_keys: Vec::new(),
        }
    }

    fn with_numeric_sort_keys(mut self, keys: Vec<Vec<Option<u64>>>) -> Self {
        assert_eq!(keys.len(), self.rows.len());
        assert!(keys
            .iter()
            .zip(&self.rows)
            .all(|(key, row)| key.len() == row.len()));
        self.numeric_sort_keys = keys;
        self
    }

    pub fn sorted_indices(
        &self,
        mut indices: Vec<usize>,
        interaction: TableInteractionState,
    ) -> Vec<usize> {
        let Some(sort) = interaction.sort() else {
            return indices;
        };
        // Decorate each visible row once instead of repeatedly decoding text
        // inside O(n log n) comparisons. Keep exact legacy rank/number/text order.
        let mut decorated = indices
            .drain(..)
            .map(|index| {
                let text = self
                    .rows
                    .get(index)
                    .and_then(|row| row.get(sort.column))
                    .map(String::as_str)
                    .unwrap_or("");
                let number = self
                    .numeric_sort_keys
                    .get(index)
                    .and_then(|row| row.get(sort.column))
                    .copied()
                    .flatten();
                let key = if let Some(number) = number {
                    SortCell::SourceNumber(number)
                } else {
                    SortCell::Display {
                        semantic: semantic_rank(text),
                        numeric: numeric_cell(text),
                        lowercase: text.to_lowercase(),
                    }
                };
                (index, key)
            })
            .collect::<Vec<_>>();
        decorated.sort_by(|(left_index, left), (right_index, right)| {
            let ordering = left.cmp(right).then_with(|| left_index.cmp(right_index));
            match sort.direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            }
        });
        indices.extend(decorated.into_iter().map(|(index, _)| index));
        indices
    }
}

enum SortCell {
    SourceNumber(u64),
    Display {
        semantic: Option<i64>,
        numeric: Option<f64>,
        lowercase: String,
    },
}

impl SortCell {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Self::SourceNumber(a), Self::SourceNumber(b)) => a.cmp(b),
            (Self::SourceNumber(_), _) => Ordering::Less,
            (_, Self::SourceNumber(_)) => Ordering::Greater,
            (
                Self::Display {
                    semantic: sa,
                    numeric: na,
                    lowercase: la,
                },
                Self::Display {
                    semantic: sb,
                    numeric: nb,
                    lowercase: lb,
                },
            ) => {
                match (sa, sb) {
                    (Some(a), Some(b)) => return a.cmp(b),
                    (Some(_), None) => return Ordering::Less,
                    (None, Some(_)) => return Ordering::Greater,
                    _ => {}
                }
                match (na, nb) {
                    (Some(a), Some(b)) => a.total_cmp(b),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    _ => la.cmp(lb),
                }
            }
        }
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
    use std::cmp::Ordering;

    // Classification must be consistent for every pair: switching to lexical
    // comparison when only one cell has a rank/number creates ordering cycles.
    match (semantic_rank(a), semantic_rank(b)) {
        (Some(a), Some(b)) => return a.cmp(&b),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        (None, None) => {}
    }
    match (numeric_cell(a), numeric_cell(b)) {
        (Some(a), Some(b)) => return a.total_cmp(&b),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        (None, None) => {}
    }
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn safe(value: &str) -> String {
    crate::ui::sanitize_terminal_text(value)
}

pub fn backup_health_text(backup: &crate::application::BackupWorkspaceItem) -> &'static str {
    if backup.display_cached {
        use crate::application::BackupHealth;
        match backup.health() {
            BackupHealth::Verified => "EDPB ✓ · 缓存",
            BackupHealth::VerificationFailed => "校验失败 · 缓存",
            BackupHealth::CoreDataInvalid => "大小异常 · 缓存",
            BackupHealth::Invalid => "EDPB ✗ · 缓存",
        }
    } else {
        backup.health().label()
    }
}

pub fn device_table_view(rows: &[crate::disk_scan::Row], generation: u64) -> TableViewData {
    device_table_view_with_search(rows, generation).0
}

pub fn device_table_view_with_search(
    rows: &[crate::disk_scan::Row],
    generation: u64,
) -> (TableViewData, Vec<String>) {
    let columns = table_column_schema(TableKind::Devices).expect("device column schema");
    let (projected, searches): (Vec<_>, Vec<_>) = rows
        .iter()
        .map(|row| {
            let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
            let search = format!(
                "disk{} {} {} {}",
                row.disk,
                identity.search_text(),
                row.proto,
                identity
                    .provision_kind
                    .map(|kind| kind.full_name())
                    .unwrap_or_default()
            )
            .to_ascii_lowercase();
            let cells = identity.display_cells();
            let projected_row = columns
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
                            if !matches!(row.proto.as_str(), "USB" | "Disk Image") {
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
                .collect::<Vec<_>>();
            (projected_row, search)
        })
        .unzip();
    let keys = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|column| match column.id {
                    ColumnId::Device => Some(u64::from(row.disk)),
                    ColumnId::Capacity => Some(row.size),
                    ColumnId::Backups => Some(row.n_baks as u64),
                    _ => None,
                })
                .collect()
        })
        .collect();
    (
        TableViewData::from_rows(generation, &columns, projected).with_numeric_sort_keys(keys),
        searches,
    )
}

pub fn backup_table_view(
    rows: &[crate::application::BackupWorkspaceItem],
    generation: u64,
) -> TableViewData {
    backup_table_view_with_search(rows, generation).0
}

pub fn backup_table_view_with_search(
    rows: &[crate::application::BackupWorkspaceItem],
    generation: u64,
) -> (TableViewData, Vec<String>) {
    let columns = table_column_schema(TableKind::Backups).expect("backup column schema");
    let (projected, searches): (Vec<_>, Vec<_>) = rows
        .iter()
        .map(|row| {
            let identity = crate::application::identity::WorkspaceIdentity::from_backup(row);
            let search = format!(
                "{} {} {} {}",
                row.file_name,
                row.display_time,
                identity.search_text(),
                identity
                    .provision_kind
                    .map(|kind| kind.full_name())
                    .unwrap_or_default()
            )
            .to_ascii_lowercase();
            let cells = identity.display_cells();
            let projected_row = columns
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
                .collect::<Vec<_>>();
            (projected_row, search)
        })
        .unzip();
    let keys = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|column| match column.id {
                    ColumnId::Index => Some(row.index as u64),
                    ColumnId::Capacity => row.size_bytes,
                    _ => None,
                })
                .collect()
        })
        .collect();
    (
        TableViewData::from_rows(generation, &columns, projected).with_numeric_sort_keys(keys),
        searches,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_sort_keys_preserve_exact_capacity_above_f64_precision() {
        let columns = table_column_schema(TableKind::Devices).unwrap();
        let row = |label: &str| {
            (0..columns.len())
                .map(|index| if index == 1 { label.into() } else { "x".into() })
                .collect::<Vec<String>>()
        };
        let view = TableViewData::from_rows(
            1,
            &columns,
            vec![
                row("9007199254740993 B"),
                row("9007199254740992 B"),
                row("unknown"),
            ],
        )
        .with_numeric_sort_keys(vec![
            (0..columns.len())
                .map(|i| (i == 1).then_some(9_007_199_254_740_993))
                .collect(),
            (0..columns.len())
                .map(|i| (i == 1).then_some(9_007_199_254_740_992))
                .collect(),
            vec![None; columns.len()],
        ]);
        let mut state = TableInteractionState::default();
        state.toggle_sort_for(1);
        assert_eq!(view.sorted_indices(vec![0, 1, 2], state), vec![1, 0, 2]);
        state.toggle_sort_for(1);
        assert_eq!(view.sorted_indices(vec![0, 1, 2], state), vec![2, 0, 1]);
    }

    #[test]
    fn mixed_semantic_numeric_and_unknown_cells_obey_total_order() {
        let cells = [
            "普通盘",
            "plain",
            "mode0 · 缺省三分区",
            "mode1 · 二合一区",
            "mode2 · 整盘加密",
            "mode3 · 内外网通用双分区",
            "—",
            "未知",
            "",
            "可用",
            "需权限",
            "读取异常",
            "非 USB",
            "-1",
            "100 MB",
            "2 GB",
            "10 GB",
            "disk2",
            "disk10",
            "3tree",
            "abc",
            "ABC",
        ];
        for a in cells {
            assert_eq!(smart_cell_cmp(a, a), std::cmp::Ordering::Equal);
            for b in cells {
                assert_eq!(
                    smart_cell_cmp(a, b),
                    smart_cell_cmp(b, a).reverse(),
                    "{a:?} / {b:?}"
                );
                for c in cells {
                    if smart_cell_cmp(a, b).is_le() && smart_cell_cmp(b, c).is_le() {
                        assert!(
                            smart_cell_cmp(a, c).is_le(),
                            "non-transitive {a:?} <= {b:?} <= {c:?}"
                        );
                    }
                }
            }
        }
        assert!(smart_cell_cmp("普通盘", "mode0").is_lt());
        assert!(smart_cell_cmp("mode0", "mode1").is_lt());
        assert!(smart_cell_cmp("mode1", "—").is_lt());
        assert!(smart_cell_cmp("100 MB", "2 GB").is_lt());
        assert!(smart_cell_cmp("disk2", "disk10").is_lt());
    }
}
