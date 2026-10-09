use super::*;
use crate::application::inspect::{AdvancedInspectItem, AdvancedInspectWorkspace};
use crate::application::inspect_tree::InspectNodeKind;
use crate::tui::state::AdvancedInspectTreeRow;

#[derive(Debug, Clone)]
struct EvidenceRow {
    category: String,
    item: String,
    value: String,
}

fn kind_label(kind: InspectNodeKind) -> &'static str {
    match kind {
        InspectNodeKind::Device => "整盘",
        InspectNodeKind::Region => "区域",
        InspectNodeKind::Extent => "范围",
        InspectNodeKind::Sector => "扇区",
        InspectNodeKind::Structure => "结构",
        InspectNodeKind::Group => "结构组",
        InspectNodeKind::Field => "字段",
        InspectNodeKind::Partition => "分区",
        InspectNodeKind::UnknownRange => "未知范围",
    }
}

fn semantic_status_label(status: crate::edpb::SemanticStatus) -> &'static str {
    match status {
        crate::edpb::SemanticStatus::Identified => "已识别",
        crate::edpb::SemanticStatus::Unknown => "未知",
    }
}

fn decoder_label(kind: crate::application::inspect::InspectDecoderKind) -> &'static str {
    match kind {
        crate::application::inspect::InspectDecoderKind::Protocol => "协议",
        crate::application::inspect::InspectDecoderKind::Lce => "LCE",
        crate::application::inspect::InspectDecoderKind::Partition => "分区",
    }
}

fn parse_state_label(state: crate::application::inspect::InspectParseState) -> &'static str {
    match state {
        crate::application::inspect::InspectParseState::Parsed => "已解析",
        crate::application::inspect::InspectParseState::Ambiguous => "未唯一确定",
        crate::application::inspect::InspectParseState::MissingContext => "缺少上下文",
        crate::application::inspect::InspectParseState::Unsupported => "暂不支持",
        crate::application::inspect::InspectParseState::Invalid => "解析失败",
    }
}

fn push(rows: &mut Vec<EvidenceRow>, category: &str, item: &str, value: impl Into<String>) {
    rows.push(EvidenceRow {
        category: category.into(),
        item: item.into(),
        value: value.into(),
    });
}

fn append_meta_text(rows: &mut Vec<EvidenceRow>, text: &str) {
    let mut section = "分析".to_string();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Enter ") {
            continue;
        }
        if line.ends_with(':') || line.ends_with('：') {
            section = line.trim_end_matches([':', '：']).trim().to_string();
            continue;
        }
        if let Some((key, value)) = line.split_once('：').or_else(|| line.split_once(": ")) {
            push(rows, &section, key.trim(), value.trim());
        } else {
            push(rows, &section, "说明", line);
        }
    }
}

fn evidence_rows(
    workspace: &AdvancedInspectWorkspace,
    row: Option<&AdvancedInspectTreeRow>,
    item: Option<&AdvancedInspectItem>,
) -> Vec<EvidenceRow> {
    let mut rows = Vec::new();
    let Some(row) = row else {
        return rows;
    };

    push(&mut rows, "对象", "类型", kind_label(row.kind));
    if row.range.sector_count == 1 {
        push(&mut rows, "位置", "LBA", row.range.start_lba.to_string());
        push(
            &mut rows,
            "位置",
            "物理字节偏移",
            format!(
                "0x{:X}",
                row.range.start_lba.saturating_mul(
                    item.map_or(crate::common::SECTOR as u64, |item| item.raw.len() as u64)
                )
            ),
        );
    } else {
        let end = row
            .range
            .start_lba
            .saturating_add(row.range.sector_count.saturating_sub(1));
        push(
            &mut rows,
            "位置",
            "LBA 范围",
            format!("{}..{}", row.range.start_lba, end),
        );
        push(
            &mut rows,
            "位置",
            "扇区数",
            row.range.sector_count.to_string(),
        );
    }
    if let Some(range) = row.range.byte_range {
        push(
            &mut rows,
            "位置",
            "字节范围",
            format!("0x{:X}..0x{:X}", range.start, range.end_exclusive),
        );
    }
    push(
        &mut rows,
        "状态",
        "语义状态",
        semantic_status_label(row.status),
    );
    if let Some(decoder) = row.decoder {
        push(&mut rows, "解码", "解码器", decoder_label(decoder));
    }
    if let Some(region) = row.region_semantic {
        push(&mut rows, "归属", "区域语义", format!("{region:?}"));
    }
    push(&mut rows, "来源", "介质", workspace.source.clone());

    if let Some(item) = item {
        if row.kind == InspectNodeKind::Field {
            if let Some(range) = row.range.byte_range {
                if let Some(field) = item.fields.iter().find(|field| field.range == range) {
                    let hex = |bytes: &[u8]| {
                        bytes
                            .iter()
                            .map(|byte| format!("{byte:02X}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    push(&mut rows, "字段", "名称", field.label.clone());
                    push(&mut rows, "字段", "值", field.value.clone());
                    push(
                        &mut rows,
                        "字段",
                        "来源 LBA",
                        (field.range.start / item.raw.len() as u64).to_string(),
                    );
                    push(
                        &mut rows,
                        "字段",
                        "分组",
                        field.group.clone().unwrap_or_else(|| "—".into()),
                    );
                    push(
                        &mut rows,
                        "字段",
                        "偏移 / 长度",
                        format!("0x{:X} / {} B", field.range.start, field.range.len()),
                    );
                    push(&mut rows, "字段", "原始", hex(&field.raw));
                    push(&mut rows, "字段", "解码", hex(&field.decoded));
                    push(
                        &mut rows,
                        "字段",
                        "逻辑值",
                        field
                            .field_logical
                            .as_deref()
                            .map(hex)
                            .unwrap_or_else(|| "—".into()),
                    );
                    push(
                        &mut rows,
                        "字段",
                        "变换",
                        field
                            .transform
                            .map(|value| format!("{value:?}"))
                            .unwrap_or_else(|| "—".into()),
                    );
                    push(
                        &mut rows,
                        "字段",
                        "类型 / 状态",
                        format!(
                            "{:?} / {}",
                            field.field_type,
                            crate::tui::state::inspect_field_status_label(field.status)
                        ),
                    );
                }
            }
        }
        if !item.regions.is_empty() {
            push(&mut rows, "归属", "区域", item.regions.join(" · "));
        }
        push(
            &mut rows,
            "数据",
            "RAW 非零字节",
            format!("{} / {}", item.raw_nonzero, item.raw.len()),
        );
        push(&mut rows, "数据", "RAW SHA-256", item.raw_sha256.clone());
        if let Some(decoded) = item.decoded_sha256.as_deref() {
            push(&mut rows, "解码", "Decoded SHA-256", decoded);
        }
        if let Some(method) = item.method.as_deref() {
            push(&mut rows, "解码", "方法", method);
        }
        if let Some(error) = item.decode_error.as_deref() {
            push(&mut rows, "解码", "错误", error);
        }
        push(
            &mut rows,
            "解析",
            "状态",
            parse_state_label(item.parse_state),
        );
        for diagnostic in &item.diagnostics {
            push(
                &mut rows,
                "诊断",
                &format!("{:?}", diagnostic.code),
                diagnostic.message.clone(),
            );
        }
        if let Some(text) = item.meta_text.as_deref() {
            append_meta_text(&mut rows, text);
        }
        for note in &item.notes {
            push(&mut rows, "备注", "说明", note);
        }
    }

    if let Some(manifest) = workspace.backup_manifest.as_ref() {
        push(&mut rows, "备份", "Manifest", manifest.schema.clone());
        push(
            &mut rows,
            "备份",
            "Region / Extent / Artifact",
            format!(
                "{} / {} / {}",
                manifest.regions.len(),
                manifest.extents.len(),
                manifest.artifacts.len()
            ),
        );
        for region in &manifest.regions {
            push(
                &mut rows,
                "Manifest.Region",
                &region.id,
                format!(
                    "role={} · start={} · sectors={} · status={:?}",
                    region.role,
                    region
                        .start_lba
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "—".into()),
                    region
                        .sector_count
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "—".into()),
                    region.semantic_status
                ),
            );
        }
        for extent in &manifest.extents {
            push(
                &mut rows,
                "Manifest.Extent",
                &extent.id,
                format!(
                    "region={} · LBA{} +{} · {}",
                    extent.region_id, extent.start_lba, extent.sector_count, extent.purpose
                ),
            );
        }
        for artifact in &manifest.artifacts {
            push(
                &mut rows,
                "Manifest.Artifact",
                &artifact.id,
                format!(
                    "kind={} · policy={:?} · completeness={:?}",
                    artifact.kind, artifact.restore_policy, artifact.completeness
                ),
            );
            push(
                &mut rows,
                "Manifest.Artifact",
                "source_extent_ids",
                artifact.source_extent_ids.join(", "),
            );
            push(
                &mut rows,
                "Manifest.Artifact",
                "SHA-256",
                artifact.storage.sha256.clone(),
            );
        }
    }

    rows
}

pub(super) fn draw_technical_evidence_table(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    workspace: &AdvancedInspectWorkspace,
    selected_row: Option<&AdvancedInspectTreeRow>,
    item: Option<&AdvancedInspectItem>,
    focused: bool,
    offset: usize,
) {
    let rows = evidence_rows(workspace, selected_row, item);
    let visible = usize::from(area.height.saturating_sub(3)).max(1);
    let start = offset.min(rows.len().saturating_sub(visible));
    let end = start.saturating_add(visible).min(rows.len());
    let theme = crate::tui::theme::current();

    let header = TableRow::new([
        Cell::from("类别").style(theme.table_header(false, focused)),
        Cell::from("项目").style(theme.table_header(false, focused)),
        Cell::from("值").style(theme.table_header(false, focused)),
    ]);
    let body = rows[start..end].iter().map(|row| {
        TableRow::new([
            Cell::from(safe(&row.category)).style(theme.table_text_muted()),
            Cell::from(safe(&row.item)).style(theme.table_text()),
            Cell::from(safe(&row.value)).style(theme.table_text()),
        ])
    });
    let title = format!(
        "技术证据 · 行 {}–{} / {}",
        if rows.is_empty() { 0 } else { start + 1 },
        end,
        rows.len()
    );
    let table = crate::tui::ui::data_table(
        &title,
        header,
        body,
        [
            Constraint::Length(10),
            Constraint::Length(22),
            Constraint::Min(20),
        ],
        focused,
    );
    frame.render_widget(table, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_text_becomes_structured_evidence_rows() {
        let mut rows = Vec::new();
        append_meta_text(
            &mut rows,
            "区域：\n  主区域: 保密区\n解码：\n  文件系统: exFAT\nEnter 打开扇区检查",
        );
        assert!(rows.iter().any(|row| {
            row.category == "区域" && row.item == "主区域" && row.value == "保密区"
        }));
        assert!(rows.iter().any(|row| {
            row.category == "解码" && row.item == "文件系统" && row.value == "exFAT"
        }));
        assert!(!rows.iter().any(|row| row.value.contains("Enter")));
    }
}
