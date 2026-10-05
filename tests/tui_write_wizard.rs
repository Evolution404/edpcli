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
fn backup_create_skips_yes_but_restore_requires_exact_media_write_authorization() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::BackupCreate, 6, None);
    assert_eq!(state.wizard().expect("wizard").stage, WizardStage::Confirm);
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    assert_eq!(state.wizard().unwrap().confirmation, "");
    assert_eq!(
        state.confirm_backup_create(),
        Some(WriteIntent {
            kind: WriteKind::BackupCreate,
            disk: 6,
            backup: None,
            expected_identity: None,
        })
    );
    assert!(state.is_critical_operation());
    state.finish_write(Ok(()));
    state.navigate(NavCommand::Escape, 20);

    state.begin_write_wizard(WriteKind::Restore, 9, Some("backup.edpb".into()));
    for ch in ['Y', 'E', 'S', 'x'] {
        state.push_wizard_confirmation(ch);
    }
    assert!(state.submit_wizard_confirmation().is_none());
    assert!(!state.is_critical_operation());
    state.clear_wizard_confirmation();
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    assert!(state.submit_wizard_confirmation().is_some());
    assert!(state.is_critical_operation());
}

#[test]
fn restore_intent_pins_both_disk_and_backup_path() {
    let mut state = AppState::new();
    let path = PathBuf::from("backup/example.bin");
    state.begin_write_wizard(WriteKind::Restore, 9, Some(path.clone()));
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
    let _ = state.confirm_backup_create();
    assert!(state.is_critical_operation());

    state.finish_write(Ok(()));
    assert!(!state.is_critical_operation());
    assert_eq!(state.wizard().expect("wizard").stage, WizardStage::Result);
}

#[test]
fn running_operation_rejects_new_wizards_but_allows_read_only_workspace_navigation() {
    let mut state = AppState::new();
    assert!(state.begin_write_wizard(WriteKind::BackupCreate, 6, None));
    let _ = state.confirm_backup_create();

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
        layout: Err("fixture layout not projected".into()),
        format_target_pin: None,
    }
}

#[test]
fn post_restore_disk_layout_free_region_clears_partition_action_target() {
    use edpcli::application::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};
    use edpcli::tui::pane::PaneId;

    let mut outcome = plain_needs_format_outcome();
    outcome.layout = DiskLayoutModel::canonical_plain_plan(
        outcome.total_sectors,
        vec![DiskLayoutSegment {
            label: "普通分区".into(),
            start_lba: outcome.assessment.partitions[0].start_lba,
            sector_count: outcome.assessment.partitions[0].sector_count,
            kind: DiskRegionKind::Plain,
        }],
    );

    let mut state = AppState::new();
    begin_post_restore(&mut state, outcome);
    assert_eq!(
        state.post_restore_result_focused_pane(),
        PaneId::ResultPartitions
    );
    assert_eq!(
        state
            .wizard()
            .unwrap()
            .post_restore_workbench
            .selected_partition,
        Some(0)
    );

    let kind = edpcli::tui::table_layout::TableKind::ResultPartitions;
    assert_eq!(state.active_table_kind(), Some(kind));
    assert!(!state.move_table_column_for_viewport(kind, true, 160, 30));
    assert_eq!(state.table_active_column(kind), 0);
    assert_eq!(
        state.post_restore_result_focused_pane(),
        PaneId::ResultPartitions
    );
    state.post_restore_result_spatial_focus(1, 0);
    assert_eq!(
        state.post_restore_result_focused_pane(),
        PaneId::ResultDiskLayout
    );
    state.post_restore_result_spatial_focus(0, 1);
    assert_eq!(
        state.post_restore_result_focused_pane(),
        PaneId::ResultVerification
    );
    state.post_restore_result_spatial_focus(-1, 0);
    assert_eq!(
        state.post_restore_result_focused_pane(),
        PaneId::ResultPartitions
    );

    state.post_restore_result_shift_pane(false);
    assert_eq!(
        state.post_restore_result_focused_pane(),
        PaneId::ResultDiskLayout
    );
    state.move_post_restore_result_selection(-1, 8);
    assert_eq!(
        state
            .wizard()
            .unwrap()
            .post_restore_workbench
            .selected_partition,
        None
    );

    state.begin_selected_post_restore_action();
    let wizard = state.wizard().unwrap();
    assert_eq!(wizard.stage, WizardStage::PostRestore);
    assert_eq!(
        wizard.message.as_deref(),
        Some("当前激活区域不是可处理分区。")
    );
}

#[test]
fn restore_post_processing_requires_a_second_yes_before_plain_format() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::Restore, 4, Some("plain.edpb".into()));
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
    assert_eq!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .assessment
            .partitions[0]
            .detected_filesystem,
        Some(FilesystemKind::ExFat),
        "the completion row must show the filesystem verified by formatting"
    );
    assert!(format_screen(&state, 120, 40).contains("exFAT"));
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
    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let _ = state.submit_wizard_confirmation();
    state.finish_restore(Ok(outcome));
}

fn start_format_with_restore(outcome: MetadataRestoreOutcome, encrypted: bool) -> AppState {
    let mut state = AppState::new();
    begin_post_restore(&mut state, outcome);
    state.begin_selected_post_restore_action();
    if state.wizard().unwrap().stage == WizardStage::PasswordInput {
        for ch in "progress-secret-123".chars() {
            state.push_wizard_secret_char(ch);
        }
        state.submit_wizard_secret();
    }
    state.submit_wizard_volume_label();
    for ch in "YES".chars() {
        state.push_wizard_confirmation(ch);
    }
    if encrypted {
        assert!(state.submit_encrypted_format_confirmation().is_some());
    } else {
        assert!(state.submit_post_restore_format_confirmation().is_some());
    }
    state
}

fn format_work_event() -> edpcli::application::progress::ProgressEvent {
    use edpcli::application::progress::{
        FormatStep, LogPolicy, OperationKind, Phase, ProgressEvent, Step, TransactionActivity,
        TransactionActivityPhase, WorkProgress,
    };
    let mut event = ProgressEvent::new(
        Phase::Format,
        Step::PostRestoreFormat(FormatStep::Write),
        5,
        10,
    )
    .with_work(WorkProgress::from_activity(TransactionActivity {
        phase: TransactionActivityPhase::FormatWrite,
        current: 32,
        total: 64,
    }));
    event.operation = OperationKind::PostRestoreFormat;
    event.log_policy = LogPolicy::SnapshotOnly;
    event
}

fn format_screen(state: &AppState, width: u16, height: u16) -> String {
    use ratatui::{backend::TestBackend, Terminal};
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| edpcli::tui::render::draw(frame, state))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

#[test]
fn post_restore_format_renders_real_work_and_sync_wait_at_all_sizes() {
    use edpcli::application::progress::{FormatStep, LogPolicy, Phase, Step};
    let mut state = start_format_with_restore(plain_needs_format_outcome(), false);
    let restored = state.wizard().unwrap().run.clone().unwrap();
    state.set_post_restore_format_progress(format_work_event());
    for (width, height) in [(40, 12), (80, 24), (120, 40)] {
        let text = format_screen(&state, width, height);
        for required in [
            "恢复后格式化",
            "disk4",
            "分区1",
            "写入文件系统结构",
            "32/64sector·50%",
            "已用时",
        ] {
            assert!(
                text.contains(required),
                "{width}x{height}: missing {required}\n{text}"
            );
        }
    }
    let mut sync = format_work_event();
    sync.step = Step::PostRestoreFormat(FormatStep::Sync);
    sync.work = None;
    sync.detail = Some("同步写入缓存到介质".into());
    sync.log_policy = LogPolicy::AppendOnChange;
    state.set_post_restore_format_progress(sync);
    for (width, height) in [(40, 12), (80, 24), (120, 40)] {
        let text = format_screen(&state, width, height);
        assert!(
            text.contains("同步写入缓存到介质"),
            "{width}x{height}: {text}"
        );
        assert!(text.contains("已用时"));
        assert!(
            !text.contains("32/64sector"),
            "old work must disappear during sync"
        );
    }
    assert_eq!(
        state.wizard().unwrap().run.as_ref().unwrap().latest,
        restored.latest
    );
    assert_eq!(
        state
            .wizard()
            .unwrap()
            .format_run
            .as_ref()
            .unwrap()
            .operation,
        edpcli::application::progress::OperationKind::PostRestoreFormat
    );
    assert!(
        state
            .wizard()
            .unwrap()
            .format_run
            .as_ref()
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .phase
            != Phase::Complete
    );
}

#[test]
fn post_restore_format_progress_is_independent_and_failure_never_reaches_completion() {
    use edpcli::application::progress::{FormatStep, Severity, Step};
    let mut state = start_format_with_restore(plain_needs_format_outcome(), false);
    let start = state
        .wizard()
        .unwrap()
        .format_run
        .as_ref()
        .unwrap()
        .started_at;
    state.set_post_restore_format_progress(format_work_event());
    let mut rollback = format_work_event();
    rollback.step = Step::PostRestoreFormat(FormatStep::RollbackWrite);
    rollback.severity = Severity::Warning;
    rollback.stage = Some(edpcli::application::progress::StageProgress::new(0, 10));
    state.set_post_restore_format_progress(rollback);
    state.finish_post_restore_format(PostRestoreFormatResult {
        partition_index: 1,
        filesystem: FilesystemKind::ExFat,
        result: Err("写入失败，已完整回滚".into()),
    });
    let wizard = state.wizard().unwrap();
    assert!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .report
            .metadata_restored
    );
    assert!(
        wizard
            .restore_outcome
            .as_ref()
            .unwrap()
            .report
            .readback_verified
    );
    assert_eq!(
        wizard
            .format_run
            .as_ref()
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .severity,
        Severity::Error
    );
    assert_eq!(
        wizard
            .format_run
            .as_ref()
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .stage
            .unwrap()
            .current,
        5
    );
    assert!(
        wizard
            .format_run
            .as_ref()
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .overall
            .basis_points()
            < 10_000
    );
    let latest = wizard.format_run.as_ref().unwrap().latest.clone();
    state.set_post_restore_format_progress(format_work_event());
    assert_eq!(
        state.wizard().unwrap().format_run.as_ref().unwrap().latest,
        latest,
        "late progress after the result must be ignored"
    );

    state.begin_selected_post_restore_action();
    state.submit_wizard_volume_label();
    for ch in "YES".chars() {
        state.push_wizard_confirmation(ch);
    }
    assert!(state.submit_post_restore_format_confirmation().is_some());
    let run = state.wizard().unwrap().format_run.as_ref().unwrap();
    assert!(run.started_at > start);
    assert_eq!(run.log.len(), 1);
    assert_eq!(run.latest.as_ref().unwrap().overall.basis_points(), 0);
}

#[test]
fn encrypted_post_restore_format_shares_progress_without_disclosing_passwords() {
    let mut state = start_format_with_restore(
        encrypted_post_restore_outcome(PostRestorePartitionState::PasswordRequired),
        true,
    );
    state.set_post_restore_format_progress(format_work_event());
    let text = format_screen(&state, 120, 40);
    assert!(text.contains("32/64sector·50%"));
    assert!(!text.contains("progress-secret-123"));
    assert!(!format!("{:?}", state.wizard().unwrap().format_run).contains("progress-secret-123"));
    state.finish_post_restore_encrypted_format(EncryptedPostRestoreFormatResult {
        partition_index: 1,
        filesystem: FilesystemKind::ExFat,
        result: Ok(()),
    });
    assert_eq!(
        state
            .wizard()
            .unwrap()
            .restore_outcome
            .as_ref()
            .unwrap()
            .assessment
            .partitions[0]
            .detected_filesystem,
        Some(FilesystemKind::ExFat)
    );
    assert!(format_screen(&state, 120, 40).contains("exFAT"));
    assert_eq!(
        state
            .wizard()
            .unwrap()
            .format_run
            .as_ref()
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .overall
            .basis_points(),
        10_000
    );
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
    assert_eq!(
        state
            .wizard()
            .unwrap()
            .restore_outcome
            .as_ref()
            .unwrap()
            .assessment
            .partitions[0]
            .detected_filesystem,
        Some(FilesystemKind::ExFat)
    );
    assert!(format_screen(&state, 120, 40).contains("exFAT"));
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

#[test]
fn shrinking_after_yes_cannot_construct_any_media_write_intent() {
    use ratatui::layout::Size;
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::Restore, 9, Some("backup.edpb".into()));
    for ch in "YES".chars() {
        state.push_wizard_confirmation(ch);
    }
    for size in [Size::new(39, 24), Size::new(80, 17), Size::new(12, 8)] {
        state.set_viewport_size(size);
        assert!(state.submit_wizard_confirmation().is_none());
        assert!(state.submit_post_restore_format_confirmation().is_none());
        assert!(state.submit_encrypted_format_confirmation().is_none());
        assert!(state.submit_reinitialize_confirmation().is_none());
        assert!(state.provision_take_for_write().is_none());
        assert!(!state.is_critical_operation());
        assert_eq!(state.wizard().unwrap().stage, WizardStage::Confirm);
    }
    state.set_viewport_size(Size::new(40, 24));
    assert!(state.submit_wizard_confirmation().is_some());
}

#[test]
fn all_confirmation_types_keep_target_warning_input_and_escape_visible() {
    use edpcli::tui::ui::{
        render_write_confirmation_modal, MediaWriteConfirmationKind, WriteConfirmationSpec,
    };
    use ratatui::{backend::TestBackend, text::Line, Terminal};
    for kind in [
        MediaWriteConfirmationKind::Provision,
        MediaWriteConfirmationKind::Restore,
        MediaWriteConfirmationKind::Format,
        MediaWriteConfirmationKind::EncryptedFormat,
        MediaWriteConfirmationKind::Reinitialize,
    ] {
        for (width, height) in [(40, 18), (40, 24), (80, 24), (160, 24)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    render_write_confirmation_modal(
                        frame,
                        WriteConfirmationSpec {
                            kind,
                            title: "确认",
                            target: "disk9 · 125.83GB".into(),
                            warning: "写入不可撤销".into(),
                            details: (0..100)
                                .map(|i| Line::from(format!("详情 {i} 长身份标识")))
                                .collect(),
                            detail_scroll: 0,
                            confirmation: "YES",
                            message: None,
                        },
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
            for required in [
                "disk9",
                "125.83GB",
                "写入不可撤销",
                "> YES",
                "Esc 取消",
                "PgUp/PgDn",
            ] {
                assert!(
                    text.split_whitespace()
                        .collect::<String>()
                        .contains(&required.split_whitespace().collect::<String>()),
                    "{kind:?} {width}x{height} missing {required}: {text}"
                );
            }
        }
    }
}
