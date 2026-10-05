//! Read-only presentation of the selected backup's metadata and restore contract.

use crate::application::backup_restore_preview::BackupRestoreRegionKind;
use crate::application::media_identity::MediaRelationship;
use crate::tui::{state::AppState, theme};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

fn field(label: &str, value: impl AsRef<str>, style: Style) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{label}  "), theme::current().muted()),
        Span::styled(crate::ui::sanitize_terminal_text(value.as_ref()), style),
    ]
}

fn pair(left: Vec<Span<'static>>, right: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = left;
    spans.push(Span::raw("    "));
    spans.extend(right);
    Line::from(spans)
}

fn section(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_string(),
        theme::current()
            .secondary_accent()
            .add_modifier(Modifier::BOLD),
    ))
}

pub(crate) fn backup_metadata_lines(state: &AppState, width: u16) -> Vec<Line<'static>> {
    let Some(backup) = state.selected_backup() else {
        return vec![Line::from("选择一条备份查看关键元数据。")];
    };
    let theme = theme::current();
    let identity = crate::application::identity::WorkspaceIdentity::from_backup_against(
        backup,
        state.selected_device(),
    );
    let cells = identity.display_cells();
    let health = backup.health().label();
    let health_style = theme.backup_health(backup.health());
    let (relation, relation_style) =
        match identity.canonical.as_ref().map(|value| value.relationship) {
            Some(MediaRelationship::SamePhysicalMedia) => ("已确认 · 物理介质", theme.success()),
            Some(MediaRelationship::DifferentMedia) => ("硬件冲突", theme.danger()),
            Some(MediaRelationship::SameEdpInstance | MediaRelationship::SameControlledLineage) => {
                ("协议相关 · 物理未确认", theme.warning())
            }
            Some(
                MediaRelationship::ProbableSameMedia
                | MediaRelationship::ModelOnlyMatch
                | MediaRelationship::Ambiguous,
            ) => ("可能相关 · 不可唯一确认", theme.warning()),
            None => ("身份未验证", theme.muted()),
        };
    let kind_style = backup
        .provision_kind
        .map(|kind| theme.provision_kind_emphasis(kind))
        .unwrap_or_else(|| theme.warning());
    let partition_count = backup
        .restore_preview
        .as_ref()
        .and_then(|preview| preview.layout.as_ref())
        .map(|layout| {
            layout
                .segments
                .iter()
                .filter(|segment| {
                    matches!(
                        segment.kind,
                        crate::tui::disk_layout::DiskRegionKind::Boot
                            | crate::tui::disk_layout::DiskRegionKind::Share
                            | crate::tui::disk_layout::DiskRegionKind::Encrypt
                            | crate::tui::disk_layout::DiskRegionKind::Combined
                            | crate::tui::disk_layout::DiskRegionKind::Compatibility
                            | crate::tui::disk_layout::DiskRegionKind::Plain
                    )
                })
                .count()
                .to_string()
        })
        .unwrap_or_else(|| "—".into());
    let mut lines = vec![
        pair(
            field("健康", health, health_style.add_modifier(Modifier::BOLD)),
            field("身份", relation, relation_style),
        ),
        section("基本信息"),
        pair(
            field("编号", format!("#{}", backup.index), theme.accent()),
            field("时间", &backup.display_time, theme.table_text()),
        ),
        pair(
            field("盘型", &cells[6], kind_style),
            field("源盘容量", &cells[0], theme.accent()),
        ),
        Line::from(field("分区数", partition_count, theme.accent())),
        section("来源设备"),
        pair(
            field("型号", &cells[2], theme.table_text()),
            field("VID:PID", &cells[1], theme.accent()),
        ),
        Line::from(field("onlyid", &cells[3], theme.accent())),
        section("归属信息"),
        pair(
            field("部门", &cells[5], theme.table_text()),
            field("姓名", &cells[4], theme.table_text()),
        ),
        section("完整性与备份范围"),
    ];
    if let Some(error) = &backup.verification_error {
        lines.push(Line::from(field("校验失败", error, theme.danger())));
    }
    if let Some(preview) = &backup.restore_preview {
        for region in &preview.region_statuses {
            if region.label == "用户文件" {
                continue;
            }
            let (status, style) = match region.kind {
                BackupRestoreRegionKind::CompleteBytes => ("✓ 完整备份", theme.success()),
                BackupRestoreRegionKind::StructureOnly => ("✓ 仅结构", theme.accent()),
                BackupRestoreRegionKind::OutOfScope => ("— 未备份", theme.muted()),
                BackupRestoreRegionKind::PartialOrInvalid => ("⚠ 不完整", theme.danger()),
            };
            lines.push(Line::from(field(&region.label, status, style)));
        }
        if let Some(contract) = &preview.restore_contract {
            lines.push(pair(
                field(
                    "分区结构",
                    if contract.restores_partition_structure {
                        "✓ 可恢复"
                    } else {
                        "— 不可恢复"
                    },
                    if contract.restores_partition_structure {
                        theme.success()
                    } else {
                        theme.muted()
                    },
                ),
                field(
                    "原文件系统",
                    if contract.restores_filesystem {
                        "✓ 可恢复"
                    } else {
                        "— 不包含"
                    },
                    if contract.restores_filesystem {
                        theme.success()
                    } else {
                        theme.muted()
                    },
                ),
            ));
            lines.push(Line::from(field(
                "用户文件",
                if contract.restores_user_data {
                    "✓ 可恢复"
                } else {
                    "— 目录和用户文件未备份"
                },
                if contract.restores_user_data {
                    theme.success()
                } else {
                    theme.warning()
                },
            )));
            if contract.post_restore_assessment_required {
                lines.push(Line::from(Span::styled(
                    "恢复后需重新评估分区可用性",
                    theme.warning(),
                )));
            }
        } else {
            lines.push(Line::from(Span::styled(
                "恢复合同不可用；不能声明可恢复范围。",
                theme.warning(),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "未提供可验证的恢复合同。",
            theme.warning(),
        )));
    }
    lines.extend([
        section("完整标识与校验摘要"),
        Line::from(field("文件", &backup.file_name, theme.table_text())),
        Line::from(field(
            "device_id",
            identity.device_id.as_deref().unwrap_or("—"),
            theme.accent(),
        )),
        Line::from(field(
            "USB serial",
            backup
                .identity
                .as_ref()
                .and_then(|snapshot| snapshot.hardware.serial.as_deref())
                .unwrap_or("—"),
            theme.accent(),
        )),
        Line::from(field(
            "SHA-256",
            backup.content_sha256.as_deref().unwrap_or("—"),
            theme.accent(),
        )),
    ]);
    if let Some(canonical) = &identity.canonical {
        lines.extend(canonical.evidence_lines().into_iter().map(|value| {
            Line::from(Span::styled(
                crate::ui::sanitize_terminal_text(&value),
                theme.muted(),
            ))
        }));
    }
    wrap_lines(lines, width)
}

fn wrap_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let width = usize::from(width).max(1);
    let mut wrapped = Vec::new();
    for line in lines {
        let mut current = Line::default();
        let mut used = 0;
        for span in line.spans {
            for grapheme in span.content.graphemes(true) {
                let cells = grapheme.width();
                if used > 0 && used + cells > width {
                    wrapped.push(current);
                    current = Line::default();
                    used = 0;
                }
                if let Some(last) = current
                    .spans
                    .last_mut()
                    .filter(|last| last.style == span.style)
                {
                    last.content.to_mut().push_str(grapheme);
                } else {
                    current
                        .spans
                        .push(Span::styled(grapheme.to_string(), span.style));
                }
                used += cells;
            }
        }
        wrapped.push(current);
    }
    wrapped
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn metadata_wrap_preserves_full_values_graphemes_and_semantic_colors() {
        let label = theme::current().muted();
        let value = theme::current().accent();
        let hash = "a".repeat(64);
        let original = format!("摘要 江苏省e\u{301}🙂{hash}");
        let wrapped = wrap_lines(
            vec![Line::from(vec![
                Span::styled("摘要 ", label),
                Span::styled(format!("江苏省e\u{301}🙂{hash}"), value),
            ])],
            9,
        );
        let joined = wrapped
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<String>();
        assert_eq!(joined, original);
        assert!(wrapped.iter().all(|line| line.to_string().width() <= 9));
        assert!(wrapped
            .iter()
            .flat_map(|line| &line.spans)
            .any(|span| { span.content.contains("e\u{301}") && span.style == value }));
        assert!(wrapped
            .last()
            .unwrap()
            .spans
            .iter()
            .all(|span| span.style == value));
    }
}
