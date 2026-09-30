use super::*;

pub(super) fn draw_restore_write_confirmation(frame: &mut Frame, state: &AppState) {
    let Some(wizard) = state.wizard() else {
        return;
    };
    let target_row = state.devices().iter().find(|row| row.disk == wizard.disk);
    let target_identity =
        target_row.map(crate::application::identity::WorkspaceIdentity::from_device);
    let target_cells = target_identity
        .as_ref()
        .map(|identity| identity.display_cells());
    let target_serial = target_row
        .map(crate::application::identity::device_hardware_serial)
        .unwrap_or_else(|| "—".into());
    let backup = wizard
        .backup
        .as_ref()
        .and_then(|path| state.backups().iter().find(|item| &item.path == path));
    let backup_identity = backup.map(|item| {
        crate::application::identity::WorkspaceIdentity::from_backup_against(item, target_row)
    });

    let mut details = Vec::new();
    if let (Some(item), Some(identity)) = (backup, backup_identity.as_ref()) {
        let cells = identity.display_cells();
        let match_level = identity
            .canonical
            .as_ref()
            .map(|canonical| canonical.match_level())
            .unwrap_or(crate::application::identity::IdentityMatchLevel::Unknown);
        let match_style = crate::tui::theme::current().identity_match_level(match_level);
        let relationship = identity.canonical_status();
        details.push(Line::from(vec![
            Span::styled("● 匹配度  ", muted()),
            Span::styled(match_level.label(), match_style),
            Span::styled(format!("  ·  {relationship}"), match_style),
        ]));
        details.push(Line::from(""));

        let theme = crate::tui::theme::current();
        let left_width: usize = 38;
        let right_width: usize = 38;
        details.push(Line::from(vec![
            Span::styled(
                crate::ui::pad_to("当前设备", left_width),
                theme.heading_text(),
            ),
            Span::styled("备份", theme.heading_text()),
        ]));
        let summary_rows = if let Some(target_cells) = target_cells.as_ref() {
            vec![
                (
                    format!("设备  disk{}", wizard.disk),
                    format!("时间  {}", safe(&item.display_time)),
                ),
                (
                    format!("型号  {}", target_cells[2]),
                    format!(
                        "文件  {}",
                        crate::tui::table_layout::truncate_cell(
                            &safe(&item.file_name),
                            right_width.saturating_sub(6),
                            crate::tui::table_layout::TruncatePolicy::Ellipsis,
                        )
                    ),
                ),
                (
                    format!("盘型  {}", target_cells[6]),
                    format!("盘型  {}", cells[6]),
                ),
                (
                    format!("容量  {}", target_cells[0]),
                    format!("容量  {}", cells[0]),
                ),
            ]
        } else {
            vec![
                (
                    format!("设备  disk{}", wizard.disk),
                    format!("时间  {}", safe(&item.display_time)),
                ),
                ("型号  —".into(), format!("文件  {}", safe(&item.file_name))),
                ("盘型  —".into(), format!("盘型  {}", cells[6])),
                ("容量  —".into(), format!("容量  {}", cells[0])),
            ]
        };
        for (current, saved) in summary_rows {
            details.push(Line::from(vec![
                Span::styled(
                    crate::ui::pad_to(
                        &crate::tui::table_layout::truncate_cell(
                            &current,
                            left_width.saturating_sub(2),
                            crate::tui::table_layout::TruncatePolicy::Ellipsis,
                        ),
                        left_width,
                    ),
                    theme.body_text(),
                ),
                Span::styled(
                    crate::ui::pad_to(
                        &crate::tui::table_layout::truncate_cell(
                            &saved,
                            right_width,
                            crate::tui::table_layout::TruncatePolicy::Ellipsis,
                        ),
                        right_width,
                    ),
                    theme.body_text(),
                ),
            ]));
        }
        details.push(Line::from(""));

        details.push(Line::from(Span::styled("身份对照", theme.heading_text())));
        details.push(Line::from(Span::styled(
            format!(
                "  {}  {}  {}  {}",
                crate::ui::pad_to("字段", 10),
                crate::ui::pad_to("当前设备", 24),
                crate::ui::pad_to("备份", 24),
                "结果"
            ),
            muted(),
        )));

        let backup_serial = item
            .identity
            .as_ref()
            .and_then(|snapshot| snapshot.hardware.serial.as_deref())
            .unwrap_or("—");
        let comparison_rows = [
            ("序列号", target_serial.as_str(), backup_serial),
            (
                "VID:PID",
                target_cells
                    .as_ref()
                    .map(|value| value[1].as_str())
                    .unwrap_or("—"),
                cells[1].as_str(),
            ),
            (
                "onlyid",
                target_cells
                    .as_ref()
                    .map(|value| value[3].as_str())
                    .unwrap_or("—"),
                cells[3].as_str(),
            ),
            (
                "部门",
                target_cells
                    .as_ref()
                    .map(|value| value[5].as_str())
                    .unwrap_or("—"),
                cells[5].as_str(),
            ),
            (
                "姓名",
                target_cells
                    .as_ref()
                    .map(|value| value[4].as_str())
                    .unwrap_or("—"),
                cells[4].as_str(),
            ),
        ];
        for (field, current, saved) in comparison_rows {
            let (result, result_style) = if current == "—" && saved == "—" {
                ("—", muted())
            } else if current == saved {
                ("✓ 一致", success())
            } else if current == "—" || saved == "—" {
                ("△ 缺失", warning())
            } else {
                ("✗ 冲突", danger())
            };
            details.push(Line::from(vec![
                Span::styled(format!("  {}  ", crate::ui::pad_to(field, 10)), muted()),
                Span::styled(
                    format!(
                        "{}  {}  ",
                        crate::ui::pad_to(current, 24),
                        crate::ui::pad_to(saved, 24)
                    ),
                    theme.body_text(),
                ),
                Span::styled(result, result_style),
            ]));
        }
    } else if let Some(path) = &wizard.backup {
        details.push(Line::from(vec![
            Span::styled("● 匹配度  ", muted()),
            Span::styled("未知", warning().add_modifier(Modifier::BOLD)),
            Span::styled("  ·  备份身份详情未加载", warning()),
        ]));
        details.push(Line::from(""));
        details.push(Line::from(format!("当前设备  disk{}", wizard.disk)));
        details.push(Line::from(format!(
            "备份      {}",
            safe(&path.display().to_string())
        )));
    }

    crate::tui::ui::render_write_confirmation_modal(
        frame,
        crate::tui::ui::WriteConfirmationSpec {
            kind: crate::tui::ui::MediaWriteConfirmationKind::Restore,
            title: "恢复写入确认",
            warning: format!("确认后将直接开始向 disk{} 写入", wizard.disk),
            details,
            confirmation: &wizard.confirmation,
            message: wizard.message.as_deref(),
        },
    );
}
