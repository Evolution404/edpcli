use super::*;
use crate::tui::{
    keymap::TuiAction,
    state::{InputMode, StateEffect},
};

fn screen(state: &AppState, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, state)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace(' ', "")
}

#[test]
fn complete_error_remains_reachable_after_resize_and_new_notice() {
    let mut state = AppState::new();
    let error = format!(
        "恢复读回失败 code=17\n路径 /备份/{}.edpb\n末尾原因：目标身份改变",
        "中文长文件名".repeat(80)
    );
    state.set_error_notice(error.clone());
    state.open_notice_details();
    state.set_success_notice("后来收到的通知");
    assert_eq!(state.notice_details().unwrap().text(), error);
    for (width, height) in [(160, 50), (40, 24), (80, 24)] {
        state.set_viewport_size(ratatui::layout::Size::new(width, height));
        state.scroll_notice_details(TuiAction::Bottom);
        let text = screen(&state, width, height);
        assert!(text.contains("末尾原因：目标身份改变"));
        assert!(text.contains("Esc/F2关闭"));
        let (count, rows) = metrics(&state);
        assert_eq!(state.notice_scroll(), count.saturating_sub(rows));
    }
}

#[test]
fn message_keys_do_not_submit_confirmation_and_quit_stays_deferred() {
    use crate::tui::{
        keymap::KeyMapper,
        runtime_input::{handle_key, KeyOutcome},
        task::TaskHub,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut state = AppState::new();
    state.begin_write_wizard_for_identity(
        crate::tui::state::WriteKind::Restore,
        6,
        Some("unused.edpb".into()),
        None,
    );
    state.set_error_notice("长错误原因");
    let mut tasks = TaskHub::new();
    let mut keys = KeyMapper::new();
    let mut input = |state: &mut AppState, code| {
        handle_key(
            state,
            &mut tasks,
            &mut keys,
            KeyEvent::new(code, KeyModifiers::NONE),
            std::path::Path::new("unused"),
            ratatui::layout::Size::new(40, 24),
        )
    };
    input(&mut state, KeyCode::F(2));
    assert!(state.notice_details().is_some());
    let mode = state.input_mode();
    input(&mut state, KeyCode::Enter);
    input(&mut state, KeyCode::Char('Y'));
    assert_eq!(state.input_mode(), mode);
    assert_eq!(mode, InputMode::Confirm);
    assert_eq!(state.wizard().unwrap().confirmation, "");
    input(&mut state, KeyCode::Esc);
    assert!(state.notice_details().is_none());
    state.set_critical_operation(true);
    input(&mut state, KeyCode::F(2));
    assert!(matches!(
        input(&mut state, KeyCode::Char('q')),
        KeyOutcome::NextIteration
    ));
    assert_eq!(
        state.navigate(crate::tui::state::NavCommand::Quit, 24),
        StateEffect::ExitDeferred
    );
}

#[test]
fn footer_reserves_complete_details_key_at_narrow_width() {
    let mut state = AppState::new();
    state.set_error_notice("恢复失败，错误码及完整路径".repeat(40));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 1)).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::shell::message_bar(
                frame,
                frame.area(),
                state.notice_message(),
                Some("Ctrl-w w 切换窗口"),
            )
        })
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.replace(' ', "").contains("F2详情"));
    assert!(text.contains('…'));
}
