use ratatui::Frame;

use crate::tui::state::{WizardState, WriteKind};

pub(super) fn draw_wizard_result(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    wizard: &WizardState,
) {
    let message = wizard.message.as_ref();
    let message_text = message.map(|message| message.text()).unwrap_or("操作结束");
    let ok = !message.is_some_and(|message| message.is_error());
    let (title, status, operation) = match wizard.kind {
        WriteKind::BackupCreate => (
            "备份结果",
            if ok {
                "备份创建完成"
            } else {
                "备份创建失败"
            },
            "元数据备份",
        ),
        WriteKind::Restore => (
            "恢复结果",
            if ok { "恢复完成" } else { "恢复失败" },
            "元数据恢复",
        ),
    };
    let tone = message
        .map(|message| message.result_tone())
        .unwrap_or(crate::tui::ui::ResultTone::Success);

    let mut cards = vec![crate::tui::ui::ResultCard {
        title: "结果说明".into(),
        lines: vec![crate::tui::ui::ResultValue {
            text: message_text.into(),
            tone,
            bold: true,
        }],
    }];

    if let Some(run) = wizard.run.as_ref() {
        let elapsed = run
            .last_activity_at
            .duration_since(run.started_at)
            .as_secs();
        cards.push(crate::tui::ui::ResultCard {
            title: "执行信息".into(),
            lines: vec![crate::tui::ui::ResultValue::primary(format!(
                "总耗时  {elapsed} 秒"
            ))],
        });
    }

    let spec = crate::tui::ui::OperationResultSpec {
        title: title.into(),
        status: status.into(),
        tone,
        fields: vec![
            crate::tui::ui::ResultField::new(
                "目标设备",
                crate::tui::ui::ResultValue::primary(format!("disk{}", wizard.disk)).emphasized(),
            ),
            crate::tui::ui::ResultField::new(
                "操作",
                crate::tui::ui::ResultValue {
                    text: operation.into(),
                    tone: crate::tui::ui::ResultTone::Accent,
                    bold: true,
                },
            ),
        ],
        table: None,
        cards,
        footer: "Enter / Esc 返回".into(),
    };
    crate::tui::ui::render_operation_result(frame, area, &spec);
}
