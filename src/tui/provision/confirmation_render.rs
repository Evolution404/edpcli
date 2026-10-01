use super::*;

fn confirmation_status_line(label: &str, value: String, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(crate::ui::pad_to(label, 10), muted()),
        Span::styled(value, style),
    ])
}

fn provision_confirmation_target(
    view: &crate::tui::state::ProvisionConfirmationViewModel,
) -> String {
    let mut parts = vec![
        format!("disk{}", view.target.disk),
        crate::common::fmt_capacity(
            view.target
                .total_sectors
                .saturating_mul(crate::common::SECTOR as u64),
        ),
    ];
    if let (Some(vid), Some(pid)) = (view.target.vid, view.target.pid) {
        parts.push(format!("VID:PID {vid:04X}:{pid:04X}"));
    }
    if let Some(onlyid) = view.target.onlyid.as_deref() {
        parts.push(format!("onlyid {onlyid}"));
    }
    parts.join(" · ")
}

pub(super) fn provision_confirmation_details(
    view: &crate::tui::state::ProvisionConfirmationViewModel,
) -> Vec<Line<'static>> {
    use crate::tui::state::{
        ProvisionConfirmationDataEffect, ProvisionConfirmationFilesystemEffect,
        ProvisionConfirmationPasswordEffect,
    };

    let cleared = view
        .regions
        .iter()
        .filter(|region| region.data_effect == ProvisionConfirmationDataEffect::Clear)
        .map(|region| region.label.as_str())
        .collect::<Vec<_>>();
    let initialized = view
        .regions
        .iter()
        .filter(|region| {
            region.password_effect == ProvisionConfirmationPasswordEffect::InitializeNew
        })
        .map(|region| region.label.as_str())
        .collect::<Vec<_>>();
    let rebuilt = view
        .regions
        .iter()
        .filter(|region| region.password_effect == ProvisionConfirmationPasswordEffect::Rebuild)
        .map(|region| region.label.as_str())
        .collect::<Vec<_>>();
    let rewrapped = view
        .regions
        .iter()
        .filter(|region| region.password_effect == ProvisionConfirmationPasswordEffect::Rewrap)
        .map(|region| region.label.as_str())
        .collect::<Vec<_>>();
    let filesystem_changes = view
        .regions
        .iter()
        .filter_map(|region| match region.filesystem_effect {
            ProvisionConfirmationFilesystemEffect::Format(filesystem) => Some((
                "格式化",
                format!("{} {}", region.label, filesystem.windows_format_name()),
            )),
            ProvisionConfirmationFilesystemEffect::Create(filesystem) => Some((
                "新建",
                format!("{} {}", region.label, filesystem.windows_format_name()),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("目标设备  ", muted()),
            Span::styled(provision_confirmation_target(view), secondary()),
        ]),
        Line::from(vec![
            Span::styled("目标布局  ", muted()),
            Span::styled(view.target.target.full_name(), secondary()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "写入影响",
            secondary().add_modifier(Modifier::BOLD),
        )),
    ];

    lines.push(if cleared.is_empty() {
        confirmation_status_line("数据", "✓ 无区域清空".into(), success())
    } else {
        confirmation_status_line(
            "数据",
            format!("⚠ 清空 {} 个区域：{}", cleared.len(), cleared.join("、")),
            warning(),
        )
    });

    lines.push(if initialized.is_empty() && rebuilt.is_empty() && rewrapped.is_empty() {
        confirmation_status_line("密码", "✓ 无密码变化".into(), success())
    } else {
        let mut effects = Vec::new();
        if !initialized.is_empty() {
            effects.push(format!("+ 新建 {} 个：{}", initialized.len(), initialized.join("、")));
        }
        if !rebuilt.is_empty() {
            effects.push(format!("⚠ 重建 {} 个：{}", rebuilt.len(), rebuilt.join("、")));
        }
        if !rewrapped.is_empty() {
            effects.push(format!("↻ 改密 {} 个：{}", rewrapped.len(), rewrapped.join("、")));
        }
        let style = if rebuilt.is_empty() { accent() } else { warning() };
        confirmation_status_line("密码", effects.join("；"), style)
    });

    lines.push(if filesystem_changes.is_empty() {
        confirmation_status_line("文件系统", "✓ 不新建/格式化".into(), success())
    } else {
        let all_format = filesystem_changes
            .iter()
            .all(|(operation, _)| *operation == "格式化");
        let all_create = filesystem_changes
            .iter()
            .all(|(operation, _)| *operation == "新建");
        let operation = if all_format {
            "格式化"
        } else if all_create {
            "新建"
        } else {
            "新建/格式化"
        };
        confirmation_status_line(
            "文件系统",
            format!(
                "⚠ {operation} {} 个区域：{}",
                filesystem_changes.len(),
                filesystem_changes
                    .iter()
                    .map(|(_, item)| item.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
            warning(),
        )
    });
    lines
}
