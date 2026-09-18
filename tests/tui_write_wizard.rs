use std::path::PathBuf;

use edpcli::tui::state::{AppState, WriteIntent, WriteKind, WizardStage};

#[test]
fn write_wizard_requires_exact_yes_before_entering_critical_stage() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::Apply, 6, None);
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
            kind: WriteKind::Apply,
            disk: 6,
            backup: None,
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
        })
    );
}

#[test]
fn finishing_write_clears_critical_state_only_after_result_is_recorded() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::Apply, 6, None);
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let _ = state.submit_wizard_confirmation();
    assert!(state.is_critical_operation());

    state.finish_write(Ok(()));
    assert!(!state.is_critical_operation());
    assert_eq!(state.wizard().expect("wizard").stage, WizardStage::Result);
}
