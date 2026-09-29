use std::path::PathBuf;

use edpcli::application::post_restore::{
    EncryptedPartitionReinitializeResult, EncryptedPostRestoreError,
    EncryptedPostRestoreFormatResult, MetadataRestoreOutcome, MetadataRestoreReport,
    PostRestoreAssessment, PostRestoreFormatResult, PostRestorePartition,
    PostRestorePartitionState,
};
use edpcli::edpb::ManifestPartition;
use edpcli::filesystem::FilesystemKind;
use edpcli::provision::ExistingFileKeyError;
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
    assert_eq!(state.wizard().expect("wizard").stage, WizardStage::Review);
    state.advance_restore_review();
    assert_eq!(state.wizard().expect("wizard").stage, WizardStage::Confirm);
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
fn running_operation_rejects_new_wizards_but_allows_read_only_workspace_navigation() {
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
    assert_eq!(state.workspace(), edpcli::tui::state::Workspace::Backups);
    assert_eq!(state.wizard().expect("original running wizard").disk, 6);

    assert_eq!(
        state.navigate(NavCommand::Quit, 20),
        StateEffect::ExitDeferred
    );
    state.finish_write(Ok(()));
    assert_eq!(state.take_deferred_exit(), StateEffect::ExitRequested);
}

fn plain_needs_format_outcome() -> MetadataRestoreOutcome {
    let manifest = ManifestPartition {
        index: 1,
        role: Some("plain".into()),
        partition_type: Some("mbr:0x07".into()),
        start_lba: 2_048,
        sector_count: 245_757_952,
        filesystem_hint: Some("exfat".into()),
        volume_label_hint: Some("普通卷".into()),
    };
    MetadataRestoreOutcome {
        report: MetadataRestoreReport {
            metadata_restored: true,
            readback_verified: true,
            restored_artifact_ids: vec!["raw.partition_table.mbr".into()],
        },
        assessment: PostRestoreAssessment {
            partitions: vec![PostRestorePartition {
                index: 1,
                role: manifest.role.clone(),
                start_lba: manifest.start_lba,
                sector_count: manifest.sector_count,
                filesystem_hint: manifest.filesystem_hint.clone(),
                detected_filesystem: None,
                requires_original_key: false,
                state: PostRestorePartitionState::NeedsFormat,
                detail: "文件系统引导区无效".into(),
            }],
            issues: Vec::new(),
        },
        partitions: vec![manifest],
        device_state: "plain".into(),
        device_id: String::new(),
        total_sectors: 245_760_000,
        format_target_pin: None,
    }
}

#[test]
fn restore_post_processing_requires_a_second_yes_before_plain_format() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::Restore, 4, Some("plain.edpb".into()));
    state.advance_restore_review();
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let _ = state.submit_wizard_confirmation();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::Running);

    state.finish_restore(Ok(plain_needs_format_outcome()));
    assert_eq!(state.wizard().unwrap().stage, WizardStage::PostRestore);
    assert!(!state.is_critical_operation());

    state.begin_selected_post_restore_action();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::VolumeLabelInput);
    assert_eq!(state.wizard().unwrap().volume_label_input, "普通卷");
    state.backspace_wizard_volume_label();
    state.backspace_wizard_volume_label();
    state.backspace_wizard_volume_label();
    for ch in "MYUSB".chars() {
        state.push_wizard_volume_label_char(ch);
    }
    state.submit_wizard_volume_label();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::FormatConfirm);
    assert_eq!(
        state
            .wizard()
            .unwrap()
            .pending_format
            .as_ref()
            .unwrap()
            .filesystem,
        FilesystemKind::ExFat
    );
    assert!(
        state.submit_post_restore_format_confirmation().is_none(),
        "format must not reuse the restore confirmation"
    );

    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let intent = state
        .submit_post_restore_format_confirmation()
        .expect("second YES starts format");
    assert_eq!(intent.disk, 4);
    assert_eq!(intent.request.partition_index, 1);
    assert_eq!(intent.volume_label, "MYUSB");
    assert!(state.is_critical_operation());
    assert_eq!(state.wizard().unwrap().stage, WizardStage::Formatting);

    state.finish_post_restore_format(PostRestoreFormatResult {
        partition_index: 1,
        filesystem: FilesystemKind::ExFat,
        result: Ok(()),
    });
    let wizard = state.wizard().unwrap();
    assert_eq!(wizard.stage, WizardStage::PostRestore);
    assert_eq!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .assessment
            .partitions[0]
            .state,
        PostRestorePartitionState::Usable
    );
    assert!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .report
            .metadata_restored,
        "format result must not rewrite restore success"
    );
    assert!(!state.is_critical_operation());
}

fn encrypted_post_restore_outcome(state: PostRestorePartitionState) -> MetadataRestoreOutcome {
    let mut outcome = plain_needs_format_outcome();
    outcome.device_state = "edp".into();
    outcome.device_id = "disk&ven_test&prod_edp".into();
    outcome.partitions[0].role = Some("encrypt".into());
    outcome.partitions[0].partition_type = Some("4".into());
    outcome.assessment.partitions[0].role = Some("encrypt".into());
    outcome.assessment.partitions[0].requires_original_key = true;
    outcome.assessment.partitions[0].state = state;
    outcome
}

fn begin_post_restore(state: &mut AppState, outcome: MetadataRestoreOutcome) {
    state.begin_write_wizard(WriteKind::Restore, 4, Some("edp.edpb".into()));
    state.advance_restore_review();
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let _ = state.submit_wizard_confirmation();
    state.finish_restore(Ok(outcome));
}

#[test]
fn password_required_flow_uses_secret_input_and_wrong_password_returns_without_losing_restore() {
    let mut state = AppState::new();
    begin_post_restore(
        &mut state,
        encrypted_post_restore_outcome(PostRestorePartitionState::PasswordRequired),
    );

    state.begin_selected_post_restore_action();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::PasswordInput);
    for ch in "old-password".chars() {
        state.push_wizard_secret_char(ch);
    }
    assert_eq!(state.wizard_secret_len(), "old-password".chars().count());
    state.submit_wizard_secret();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::VolumeLabelInput);
    assert_eq!(state.wizard().unwrap().volume_label_input, "普通卷");
    state.submit_wizard_volume_label();
    assert_eq!(
        state.wizard().unwrap().stage,
        WizardStage::EncryptedFormatConfirm
    );
    assert!(
        state.submit_encrypted_format_confirmation().is_none(),
        "restore YES must never be reused for encrypted format"
    );

    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let intent = state
        .submit_encrypted_format_confirmation()
        .expect("independent YES starts encrypted format");
    assert_eq!(intent.request.partition_index, 1);
    assert!(intent.password.is_some());
    assert_eq!(intent.volume_label, "普通卷");
    assert_eq!(state.wizard().unwrap().stage, WizardStage::Formatting);
    assert!(state.is_critical_operation());

    state.finish_post_restore_encrypted_format(EncryptedPostRestoreFormatResult {
        partition_index: 1,
        filesystem: FilesystemKind::ExFat,
        result: Err(EncryptedPostRestoreError::FileKey(
            ExistingFileKeyError::PasswordMismatch,
        )),
    });
    let wizard = state.wizard().unwrap();
    assert_eq!(wizard.stage, WizardStage::PasswordInput);
    assert_eq!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .assessment
            .partitions[0]
            .state,
        PostRestorePartitionState::PasswordRequired
    );
    assert!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .report
            .metadata_restored
    );
    assert!(!state.is_critical_operation());
}

#[test]
fn crypto_invalid_reinitialize_requires_two_matching_passwords_and_independent_yes() {
    let mut state = AppState::new();
    begin_post_restore(
        &mut state,
        encrypted_post_restore_outcome(PostRestorePartitionState::CryptoMetadataInvalid),
    );

    state.begin_selected_post_restore_action();
    assert_eq!(
        state.wizard().unwrap().stage,
        WizardStage::ReinitializePassword
    );
    for ch in "new-pass".chars() {
        state.push_wizard_secret_char(ch);
    }
    state.submit_wizard_secret();
    assert_eq!(
        state.wizard().unwrap().stage,
        WizardStage::ReinitializePasswordConfirm
    );

    for ch in "wrong".chars() {
        state.push_wizard_secret_char(ch);
    }
    state.submit_wizard_secret();
    assert_eq!(
        state.wizard().unwrap().stage,
        WizardStage::ReinitializePassword
    );
    assert!(state
        .wizard()
        .unwrap()
        .message
        .as_deref()
        .unwrap_or_default()
        .contains("不一致"));

    for ch in "new-pass".chars() {
        state.push_wizard_secret_char(ch);
    }
    state.submit_wizard_secret();
    for ch in "new-pass".chars() {
        state.push_wizard_secret_char(ch);
    }
    state.submit_wizard_secret();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::VolumeLabelInput);
    assert_eq!(state.wizard().unwrap().volume_label_input, "普通卷");
    state.submit_wizard_volume_label();
    assert_eq!(
        state.wizard().unwrap().stage,
        WizardStage::ReinitializeConfirm
    );
    assert!(
        state.submit_reinitialize_confirmation().is_none(),
        "reinitialize requires its own YES"
    );

    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let intent = state
        .submit_reinitialize_confirmation()
        .expect("independent YES starts reinitialize");
    assert_eq!(intent.request.partition_index, 1);
    assert_eq!(intent.volume_label, "普通卷");
    assert_eq!(state.wizard().unwrap().stage, WizardStage::Reinitializing);
    assert!(state.is_critical_operation());

    state.finish_post_restore_reinitialize(EncryptedPartitionReinitializeResult {
        partition_index: 1,
        filesystem: FilesystemKind::ExFat,
        result: Ok(()),
    });
    let wizard = state.wizard().unwrap();
    assert_eq!(wizard.stage, WizardStage::PostRestore);
    assert_eq!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .assessment
            .partitions[0]
            .state,
        PostRestorePartitionState::Usable
    );
    assert!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .report
            .metadata_restored
    );
    assert!(!state.is_critical_operation());
}

#[test]
fn legacy_backup_without_volume_label_stays_empty_instead_of_inventing_a_name() {
    let mut state = AppState::new();
    let mut outcome = plain_needs_format_outcome();
    outcome.partitions[0].volume_label_hint = None;
    begin_post_restore(&mut state, outcome);

    state.begin_selected_post_restore_action();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::VolumeLabelInput);
    assert_eq!(state.wizard().unwrap().volume_label_input, "");
    state.submit_wizard_volume_label();
    assert_eq!(state.wizard().unwrap().stage, WizardStage::FormatConfirm);

    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let intent = state
        .submit_post_restore_format_confirmation()
        .expect("empty label is a valid explicit choice");
    assert_eq!(intent.volume_label, "");
}
