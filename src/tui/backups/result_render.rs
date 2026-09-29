use ratatui::Frame;

use crate::tui::state::{BackupBatchDeleteState, BackupDeleteState, BackupPruneState};

fn success_message(message: &str, success_prefixes: &[&str]) -> bool {
    success_prefixes
        .iter()
        .any(|prefix| message.starts_with(prefix))
}

fn message_card(title: &str, message: &str, ok: bool) -> crate::tui::ui::ResultCard {
    crate::tui::ui::ResultCard {
        title: title.into(),
        lines: vec![if ok {
            crate::tui::ui::ResultValue::success(message).emphasized()
        } else {
            crate::tui::ui::ResultValue::danger(message).emphasized()
        }],
    }
}

pub(super) fn draw_backup_delete_result(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    delete: &BackupDeleteState,
) {
    let message = delete.message.as_deref().unwrap_or("删除流程结束");
    let ok = success_message(message, &["备份已删除"]);
    let spec = crate::tui::ui::OperationResultSpec {
        title: "备份删除结果".into(),
        status: if ok {
            "删除完成".into()
        } else {
            "删除失败".into()
        },
        tone: if ok {
            crate::tui::ui::ResultTone::Success
        } else {
            crate::tui::ui::ResultTone::Danger
        },
        fields: vec![crate::tui::ui::ResultField::new(
            "文件",
            crate::tui::ui::ResultValue::primary(
                delete
                    .path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("<无效文件名>"),
            )
            .emphasized(),
        )],
        table: None,
        cards: vec![message_card("结果说明", message, ok)],
        footer: "Enter / Esc 返回备份列表".into(),
    };
    crate::tui::ui::render_operation_result(frame, area, &spec);
}

pub(super) fn draw_backup_batch_delete_result(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    batch: &BackupBatchDeleteState,
) {
    let message = batch.message.as_deref().unwrap_or("批量删除流程结束");
    let ok = success_message(message, &["批量删除完成"]);
    let planned = batch
        .prepared
        .as_ref()
        .map(|plan| plan.targets.len())
        .unwrap_or_default();
    let spec = crate::tui::ui::OperationResultSpec {
        title: "批量删除结果".into(),
        status: if ok {
            "批量删除完成".into()
        } else {
            "批量删除失败".into()
        },
        tone: if ok {
            crate::tui::ui::ResultTone::Success
        } else {
            crate::tui::ui::ResultTone::Danger
        },
        fields: vec![crate::tui::ui::ResultField::new(
            "固定目标",
            crate::tui::ui::ResultValue::primary(format!("{planned} 份")),
        )],
        table: None,
        cards: vec![message_card("结果说明", message, ok)],
        footer: "Enter / Esc 返回备份列表".into(),
    };
    crate::tui::ui::render_operation_result(frame, area, &spec);
}

pub(super) fn draw_backup_prune_result(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    prune: &BackupPruneState,
) {
    let message = prune.message.as_deref().unwrap_or("清理流程结束");
    let ok = success_message(message, &["清理完成", "无需清理"]);
    let tone = if ok {
        crate::tui::ui::ResultTone::Success
    } else {
        crate::tui::ui::ResultTone::Danger
    };
    let spec = crate::tui::ui::OperationResultSpec {
        title: "备份清理结果".into(),
        status: if message.starts_with("无需清理") {
            "无需清理".into()
        } else if ok {
            "清理完成".into()
        } else {
            "清理失败".into()
        },
        tone,
        fields: vec![crate::tui::ui::ResultField::new(
            "保留策略",
            crate::tui::ui::ResultValue::primary(format!("keep-{}", prune.keep_input)),
        )],
        table: None,
        cards: vec![message_card("结果说明", message, ok)],
        footer: "Enter / Esc 返回备份列表".into(),
    };
    crate::tui::ui::render_operation_result(frame, area, &spec);
}
