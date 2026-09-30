//! Read-only semantic summaries for Inspect Overview.
//!
//! This adapter consumes already materialized evidence and never reads a device or backup.

use super::inspect::{
    AbsoluteByteRange, InspectDecoderKind, InspectDiagnostic, InspectDiagnosticCode, InspectField,
    InspectFieldKey, InspectFieldStatus, InspectParseState,
};
use super::inspect_tree::{
    format_lba_closed_range, DiskRegionSemantic, InspectNodeKind, InspectNodeRange,
};
use crate::edpb::SemanticStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryImportance {
    Primary,
    Secondary,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryItem {
    pub key: Option<InspectFieldKey>,
    pub label: String,
    pub value: String,
    pub importance: SummaryImportance,
    pub source_range: Option<AbsoluteByteRange>,
    pub status: InspectFieldStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummarySection {
    pub title: String,
    pub items: Vec<SummaryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryAlert {
    pub code: InspectDiagnosticCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectNodeSummary {
    pub title: String,
    pub subtitle: String,
    pub location: String,
    pub parse_state: InspectParseState,
    pub sections: Vec<SummarySection>,
    pub alerts: Vec<SummaryAlert>,
}

#[derive(Debug, Clone, Copy)]
pub struct InspectSummarySource<'a> {
    pub kind: InspectNodeKind,
    pub label: &'a str,
    pub range: InspectNodeRange,
    pub decoder: Option<InspectDecoderKind>,
    pub status: SemanticStatus,
    pub region_semantic: Option<DiskRegionSemantic>,
    pub fields: &'a [InspectField],
    pub parse_state: InspectParseState,
    pub diagnostics: &'a [InspectDiagnostic],
}

fn title(source: &InspectSummarySource<'_>) -> String {
    if source.kind == InspectNodeKind::Sector {
        let meaning = match source.region_semantic {
            Some(DiskRegionSemantic::PartitionTable) => "分区表结构",
            Some(DiskRegionSemantic::PlainPartition) => "普通分区数据",
            Some(DiskRegionSemantic::Unallocated) => "空闲区域",
            Some(DiskRegionSemantic::Lce) => "LCE 兼容区",
            Some(DiskRegionSemantic::Tail) => "尾部区域",
            Some(DiskRegionSemantic::TailMetadataMirror) => "盘尾历史镜像",
            Some(DiskRegionSemantic::TailRestoreNode) => "盘尾恢复节点",
            Some(
                DiskRegionSemantic::Partition { .. } | DiskRegionSemantic::MbrPartition { .. },
            ) => "EDP 分区数据",
            Some(DiskRegionSemantic::Unknown) => "未知扇区",
            Some(DiskRegionSemantic::Protocol) | None => match source.range.start_lba {
                0 => "主引导记录",
                1 => "GPT/兼容结构",
                2 => "分区兼容结构",
                3 => "制造商私有元数据",
                4 => "设备 onlyid",
                5 => "写保护探测区",
                6 => "设备标签与归属",
                7 => "分区表与密码策略",
                8 => "设备身份与电子标签",
                9 => "归属与扩展元数据",
                10 => "EESI 兼容结构",
                11 => "设备身份与容量",
                12 => "密钥记录与密码策略",
                _ => "扇区证据",
            },
            Some(DiskRegionSemantic::Conflict) => "重叠证据",
        };
        return format!("{} · {meaning}", source.label);
    }
    match source.region_semantic {
        Some(DiskRegionSemantic::Protocol) => "EDP 主协议区".into(),
        Some(DiskRegionSemantic::PartitionTable) => "分区表结构".into(),
        Some(DiskRegionSemantic::PlainPartition) => format!("{} · 普通分区", source.label),
        Some(DiskRegionSemantic::Unallocated) => "空闲区域".into(),
        Some(DiskRegionSemantic::Lce) => "LCE 兼容区".into(),
        Some(DiskRegionSemantic::Tail) => "尾部区域".into(),
        Some(DiskRegionSemantic::TailMetadataMirror) => "盘尾历史镜像".into(),
        Some(DiskRegionSemantic::TailRestoreNode) => "盘尾恢复节点".into(),
        Some(DiskRegionSemantic::Partition { .. } | DiskRegionSemantic::MbrPartition { .. }) => {
            format!("{} · 分区", source.label)
        }
        Some(DiskRegionSemantic::Unknown) => "未知区域".into(),
        _ => source.label.to_string(),
    }
}

fn field_item(field: &InspectField, label: &str, importance: SummaryImportance) -> SummaryItem {
    SummaryItem {
        key: Some(field.key),
        label: label.into(),
        value: field.value.clone(),
        importance,
        source_range: Some(field.range),
        status: field.status,
    }
}

fn lba8_sections(fields: &[InspectField]) -> Vec<SummarySection> {
    let find = |key| fields.iter().find(|field| field.key == key);
    let mut sections = Vec::new();
    if let Some(field) =
        find(InspectFieldKey::Lba8Elabel).filter(|field| field.status == InspectFieldStatus::Known)
    {
        let mut ownership = Vec::new();
        for (key, label) in [("Dept", "部门"), ("User", "用户")] {
            if let Some(child) = field.children.iter().find(|child| child.label == key) {
                ownership.push(SummaryItem {
                    key: Some(field.key),
                    label: label.into(),
                    value: child.value.clone(),
                    importance: SummaryImportance::Primary,
                    source_range: Some(field.range),
                    status: field.status,
                });
            }
        }
        ownership.push(SummaryItem {
            key: Some(field.key),
            label: "E_LABEL".into(),
            value: format!("已解析 {} 项", field.children.len()),
            importance: SummaryImportance::Primary,
            source_range: Some(field.range),
            status: field.status,
        });
        sections.push(SummarySection {
            title: "归属与电子标签".into(),
            items: ownership,
        });
    }
    let mut identity = Vec::new();
    if let Some(field) = find(InspectFieldKey::Lba8UsbOnlyInfo) {
        identity.push(field_item(field, "UsbOnlyInfo", SummaryImportance::Primary));
    }
    if let Some(field) = find(InspectFieldKey::Lba8HostHardinfo) {
        identity.push(field_item(
            field,
            "HostHardinfo",
            SummaryImportance::Primary,
        ));
    }
    if let Some(field) = find(InspectFieldKey::Lba8MacInfo) {
        identity.push(field_item(field, "MacInfo", SummaryImportance::Secondary));
    }
    if !identity.is_empty() {
        sections.push(SummarySection {
            title: "Profile / 宿主信息".into(),
            items: identity,
        });
    }
    let mut versions = Vec::new();
    for (key, label) in [
        (InspectFieldKey::Lba8ToolVersion, "Tool version"),
        (InspectFieldKey::Lba8LabVersion, "Lab version"),
        (InspectFieldKey::Lba8WriteTime, "writeTime"),
        (InspectFieldKey::Lba8LogicalLength, "logical length"),
    ] {
        if let Some(field) = find(key) {
            versions.push(field_item(field, label, SummaryImportance::Secondary));
        }
    }
    if !versions.is_empty() {
        sections.push(SummarySection {
            title: "版本 / 写入".into(),
            items: versions,
        });
    }
    sections
}

pub fn summarize_node(source: InspectSummarySource<'_>) -> InspectNodeSummary {
    let location = if source.range.sector_count == 1 {
        format!("LBA {} · 512 B", source.range.start_lba)
    } else {
        format!(
            "{} · {} sectors",
            format_lba_closed_range(source.range.start_lba, source.range.end_lba_exclusive())
                .unwrap_or_else(|| "[空区间]".into()),
            source.range.sector_count
        )
    };
    let subtitle = match source.parse_state {
        InspectParseState::Parsed => "已识别",
        InspectParseState::Ambiguous => "profile 未唯一确定",
        InspectParseState::MissingContext => "缺少解析上下文",
        InspectParseState::Unsupported => "无已注册 decoder",
        InspectParseState::Invalid => "解析失败",
    };
    let subtitle = match source.decoder {
        Some(decoder) => format!("{subtitle} · {decoder:?}"),
        None => subtitle.into(),
    };
    let mut sections = if source.kind == InspectNodeKind::Sector && source.range.start_lba == 8 {
        lba8_sections(source.fields)
    } else {
        let items = source
            .fields
            .iter()
            .filter(|field| field.status == InspectFieldStatus::Known)
            .take(4)
            .map(|field| field_item(field, &field.label, SummaryImportance::Primary))
            .collect::<Vec<_>>();
        if items.is_empty() {
            Vec::new()
        } else {
            vec![SummarySection {
                title: "关键字段".into(),
                items,
            }]
        }
    };
    if sections.is_empty() {
        let role = match source.region_semantic {
            Some(DiskRegionSemantic::Partition { partition_type }) => {
                format!("EDP type{partition_type}")
            }
            Some(DiskRegionSemantic::MbrPartition { partition_type }) => {
                format!("MBR type=0x{partition_type:02X}")
            }
            Some(DiskRegionSemantic::Tail) => "LCE 起始至盘尾的复合区域".into(),
            Some(DiskRegionSemantic::Unknown) => "尚无可验证的协议分类".into(),
            _ => match source.status {
                SemanticStatus::Identified => "已识别物理范围".into(),
                SemanticStatus::Unknown => "尚无可验证语义".into(),
            },
        };
        sections.push(SummarySection {
            title: "语义".into(),
            items: vec![SummaryItem {
                key: None,
                label: "角色".into(),
                value: role,
                importance: SummaryImportance::Primary,
                source_range: source.range.byte_range,
                status: InspectFieldStatus::Known,
            }],
        });
    }
    let alerts = source
        .diagnostics
        .iter()
        .map(|diagnostic| SummaryAlert {
            code: diagnostic.code,
            message: diagnostic.message.clone(),
        })
        .collect();
    InspectNodeSummary {
        title: title(&source),
        subtitle,
        location,
        parse_state: source.parse_state,
        sections,
        alerts,
    }
}
