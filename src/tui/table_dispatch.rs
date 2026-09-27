//! Shared table interaction dispatch, including injectable clipboard writes.

use super::*;

pub(super) fn dispatch_table_action(
    state: &mut AppState,
    action: keymap::TuiAction,
    viewport_height: usize,
    viewport_width: u16,
) -> bool {
    dispatch_table_action_with_clipboard(
        state,
        action,
        viewport_height,
        viewport_width,
        &mut clipboard::ClipboardService,
    )
}

pub(super) fn dispatch_table_action_with_clipboard(
    state: &mut AppState,
    action: keymap::TuiAction,
    viewport_height: usize,
    viewport_width: u16,
    clipboard: &mut dyn clipboard::ClipboardBackend,
) -> bool {
    use keymap::TuiAction;

    let Some(kind) = state.active_table_kind() else {
        return false;
    };
    match action {
        TuiAction::TableColumnLeft => {
            state.move_table_column_for_viewport(kind, true, viewport_width, viewport_height);
        }
        TuiAction::TableColumnRight => {
            state.move_table_column_for_viewport(kind, false, viewport_width, viewport_height);
        }
        TuiAction::TableMoveColumnLeft => {
            state.reorder_table_column_for_viewport(kind, true, viewport_width, viewport_height);
        }
        TuiAction::TableMoveColumnRight => {
            state.reorder_table_column_for_viewport(kind, false, viewport_width, viewport_height);
        }
        TuiAction::TableColumnFirst => {
            state.move_table_column_edge_for_viewport(kind, false, viewport_width, viewport_height);
        }
        TuiAction::TableColumnLast => {
            state.move_table_column_edge_for_viewport(kind, true, viewport_width, viewport_height);
        }
        TuiAction::TableScrollLeft => {
            state.scroll_table_for_viewport(kind, true, viewport_width, viewport_height);
        }
        TuiAction::TableScrollRight => {
            state.scroll_table_for_viewport(kind, false, viewport_width, viewport_height);
        }
        TuiAction::TableSortToggle => state.toggle_table_sort(kind),
        TuiAction::TableSortClear => {
            state.clear_table_sort(kind);
        }
        TuiAction::TableCopyCell | TuiAction::TableCopyRow => {
            let whole_row = action == TuiAction::TableCopyRow;
            if !whole_row {
                let logical = state.table_logical_column(kind, state.table_active_column(kind));
                if !crate::tui::table_layout::table_column_copyable(kind, logical) {
                    state.set_notice("当前列是界面控制列，不复制。");
                    return true;
                }
            }
            let Some(payload) = state.table_copy_payload(kind, whole_row) else {
                state.set_notice("当前没有可复制的数据。");
                return true;
            };
            let outcome = clipboard.copy(&payload);
            state.set_notice(clipboard::outcome_message(&outcome, whole_row));
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeClipboard {
        payloads: Vec<String>,
    }

    impl clipboard::ClipboardBackend for FakeClipboard {
        fn copy(&mut self, content: &str) -> clipboard::ClipboardOutcome {
            self.payloads.push(content.into());
            clipboard::ClipboardOutcome::Confirmed
        }
    }

    fn device() -> crate::disk_scan::Row {
        crate::disk_scan::Row {
            disk: 6,
            size: 64_000_000_000,
            vid: "1234".into(),
            pid: "5678".into(),
            proto: "USB".into(),
            serial: None,
            device_id: None,
            identity_pin: None,
            onlyid: None,
            dept: Some("中文部门🙂".into()),
            user: Some("测试用户".into()),
            label: None,
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
            n_baks: 0,
            n_possible_baks: 0,
            denied: false,
            probe_error: None,
            provision_kind: crate::provision::DiskProvisionKind::Plain,
            partitions: None,
            partition_table: None,
            partition_table_error: None,
            lce: None,
        }
    }

    #[test]
    fn fake_backend_receives_cell_and_runtime_order_row() {
        let mut state = AppState::new();
        state.replace_devices(vec![device()]);
        let mut clipboard = FakeClipboard::default();
        assert!(dispatch_table_action_with_clipboard(
            &mut state,
            keymap::TuiAction::TableCopyCell,
            30,
            160,
            &mut clipboard,
        ));
        assert_eq!(clipboard.payloads.len(), 1);
        assert_eq!(state.notice(), Some("已复制当前单元格。"));

        assert!(dispatch_table_action_with_clipboard(
            &mut state,
            keymap::TuiAction::TableCopyRow,
            30,
            160,
            &mut clipboard,
        ));
        assert_eq!(clipboard.payloads.len(), 2);
        assert!(clipboard.payloads[1].contains("中文部门🙂"));
        assert_eq!(state.notice(), Some("已复制当前整行。"));
    }

    #[test]
    fn control_column_never_calls_clipboard_backend() {
        let mut state = AppState::new();
        state.navigate(NavCommand::WorkspaceBackups, 20);
        let mut clipboard = FakeClipboard::default();
        assert!(dispatch_table_action_with_clipboard(
            &mut state,
            keymap::TuiAction::TableCopyCell,
            30,
            160,
            &mut clipboard,
        ));
        assert!(clipboard.payloads.is_empty());
        assert_eq!(state.notice(), Some("当前列是界面控制列，不复制。"));
    }
}
