use std::path::PathBuf;

use edpcli::tui::state::{AppState, NavCommand, StateEffect, WizardStage, WriteIntent, WriteKind};

#[test]
fn write_wizard_requires_exact_yes_before_entering_critical_stage() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::BackupCreate, 6, None);
    assert_eq!(state.wizard().expect("wizard").stage, WizardStage::Confirm);

    for ch in ['Y', 'E', 'S', 'x'] {
        state.push_wizard_confirmation(ch);
    }
    assert!(state.submit_wizard_confirmation().is_none());
    assert!(!state.is_critical_operation());

    state.clear_wizard_confirmation();
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    assert_eq!(
        state.submit_wizard_confirmation(),
        Some(WriteIntent {
            kind: WriteKind::BackupCreate,
            disk: 6,
            backup: None,
            expected_identity: None,
        })
    );
    assert!(state.is_critical_operation());
}

#[test]
fn restore_intent_pins_both_disk_and_backup_path() {
    let mut state = AppState::new();
    let path = PathBuf::from("backup/example.bin");
    state.begin_write_wizard(WriteKind::Restore, 9, Some(path.clone()));
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }

    assert_eq!(
        state.submit_wizard_confirmation(),
        Some(WriteIntent {
            kind: WriteKind::Restore,
            disk: 9,
            backup: Some(path),
            expected_identity: None,
        })
    );
}

#[test]
fn finishing_write_clears_critical_state_only_after_result_is_recorded() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::BackupCreate, 6, None);
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let _ = state.submit_wizard_confirmation();
    assert!(state.is_critical_operation());

    state.finish_write(Ok(()));
    assert!(!state.is_critical_operation());
    assert_eq!(state.wizard().expect("wizard").stage, WizardStage::Result);
}

#[test]
fn running_operation_rejects_new_wizards_and_all_navigation() {
    let mut state = AppState::new();
    assert!(state.begin_write_wizard(WriteKind::BackupCreate, 6, None));
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let _ = state.submit_wizard_confirmation();

    assert!(!state.begin_write_wizard(WriteKind::Restore, 7, Some("other.bin".into())));
    assert!(!state.begin_backup_delete(
        "old.bin".into(),
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    ));
    assert_eq!(
        state.navigate(NavCommand::WorkspaceBackups, 20),
        StateEffect::None
    );
    assert_eq!(state.wizard().expect("original running wizard").disk, 6);

    assert_eq!(
        state.navigate(NavCommand::Quit, 20),
        StateEffect::ExitDeferred
    );
    state.finish_write(Ok(()));
    assert_eq!(state.take_deferred_exit(), StateEffect::ExitRequested);
}
