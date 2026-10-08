use super::*;

#[test]
fn key_probe_keeps_latest_form_and_discards_requests_on_exit() {
    let mut hub = TaskHub::new();
    let first = KeyProbeContext {
        disk: 6,
        session_id: 100,
    };
    hub.retain_key_probe_context(Some(first));
    let generation = hub.provision.key_probe_slot.try_begin().unwrap();
    for session_id in 101..1101 {
        hub.request_key_probe_session(KeyProbeContext {
            disk: 7,
            session_id,
        })
        .unwrap();
    }
    assert!(hub.provision.key_probe_slot.single_flight.is_running());
    match hub.provision.key_probe_slot.finish_latest(generation) {
        LatestCompletion::Restart { request, .. } => assert_eq!(
            request,
            KeyProbeContext {
                disk: 7,
                session_id: 1100
            }
        ),
        _ => panic!("latest target must be restarted after old read completes"),
    }
    hub.request_key_probe_session(KeyProbeContext {
        disk: 8,
        session_id: 1101,
    })
    .unwrap();
    hub.retain_key_probe_context(None);
    assert!(hub.provision.key_probe_slot.pending_latest.is_none());
    hub.tx
        .send(WorkerResult::Provision(ProvisionWorkerResult::KeyProbe {
            generation,
            context: first,
            result: Err("stale".into()),
        }))
        .unwrap();
    assert!(hub.poll().provision.key_probe.is_none());
    assert!(!hub.provision.key_probe_slot.single_flight.is_running());
}

#[test]
fn key_probe_batch_cannot_mutate_a_reopened_form() {
    let mut state = crate::tui::demo::build_scene("provision-form").unwrap();
    let old = KeyProbeContext {
        disk: state.selected_device_disk().unwrap(),
        session_id: state.provision().session_id,
    };
    state.provision_reset();
    state.provision_mut().form.share_opaque_profile = false;
    let new = KeyProbeContext {
        disk: state.selected_device_disk().unwrap(),
        session_id: state.provision().session_id,
    };
    assert_ne!(old.session_id, new.session_id);
    let probe = crate::application::provision::ProvisionKeyProbe {
        source_kind: crate::provision::DiskProvisionKind::Mode0,
        share: None,
        encrypt: None,
        share_opaque_profile: true,
        encrypt_opaque_profile: true,
    };
    let mut hub = TaskHub::new();
    for (context, expected) in [
        (old, false),
        (KeyProbeContext { disk: 7, ..new }, false),
        (new, true),
    ] {
        let updates = ProvisionUpdates {
            key_probe: Some((context, Ok(probe))),
            ..ProvisionUpdates::default()
        };
        crate::tui::provision_runtime_updates::apply(
            &mut state,
            &mut hub,
            updates,
            std::path::Path::new("unused"),
        );
        assert_eq!(state.provision().form.share_opaque_profile, expected);
    }
}

#[test]
fn post_restore_format_progress_is_gated_and_survives_a_result_in_the_same_poll() {
    use crate::application::progress::{FormatStep, OperationKind, Phase, ProgressEvent, Step};
    let mut hub = TaskHub::new();
    let stale = hub.begin_operation().unwrap();
    assert!(hub.finish_operation(stale));
    let active = hub.begin_operation().unwrap();
    let mut event = ProgressEvent::started(
        OperationKind::PostRestoreFormat,
        Phase::Format,
        Step::PostRestoreFormat(FormatStep::Write),
        "writing",
    );
    for operation_id in [stale, active] {
        hub.tx
            .send(WorkerResult::PostRestoreFormatProgress {
                operation_id,
                event: event.clone(),
            })
            .unwrap();
    }
    let updates = hub.poll();
    assert!(updates.has_updates());
    assert_eq!(updates.post_restore_format_progress.len(), 1);
    assert_eq!(updates.post_restore_format_progress[0].0, active);

    event.step = Step::Completed;
    hub.tx
        .send(WorkerResult::PostRestoreFormatProgress {
            operation_id: active,
            event,
        })
        .unwrap();
    hub.tx
        .send(WorkerResult::PostRestoreFormat {
            assessment: None,
            operation_id: active,
            result: crate::application::post_restore::PostRestoreFormatResult {
                partition_index: 1,
                filesystem: crate::filesystem::FilesystemKind::ExFat,
                result: Ok(()),
            },
        })
        .unwrap();
    let updates = hub.poll();
    assert_eq!(updates.post_restore_format_progress.len(), 1);
    assert!(updates.post_restore_format.is_some());
    assert_eq!(hub.active_operation(), None);
}

#[test]
fn critical_operation_slot_is_exclusive_and_ids_do_not_repeat() {
    let mut hub = TaskHub::new();
    let first = hub.begin_operation().expect("first operation");
    assert_eq!(hub.active_operation(), Some(first));
    assert!(hub.begin_operation().is_err());
    assert!(hub.finish_operation(first));

    let second = hub.begin_operation().expect("second operation");
    assert_ne!(first, second);
}

#[test]
fn progress_batch_retains_every_worker_event_in_order() {
    let mut hub = TaskHub::new();
    let operation_id = hub.begin_operation().unwrap();
    for (index, phase) in [
        crate::application::progress::Phase::Backup,
        crate::application::progress::Phase::Metadata,
        crate::application::progress::Phase::Readback,
    ]
    .into_iter()
    .enumerate()
    {
        hub.tx
            .send(WorkerResult::Provision(ProvisionWorkerResult::Progress {
                operation_id,
                event: crate::application::progress::ProgressEvent::new(
                    phase,
                    crate::application::progress::Step::ProtocolReadback,
                    index as u64,
                    3,
                ),
            }))
            .unwrap();
    }
    let updates = hub.poll();
    assert_eq!(
        updates
            .provision
            .progress
            .into_iter()
            .map(|(_, event)| event.phase)
            .collect::<Vec<_>>(),
        [
            crate::application::progress::Phase::Backup,
            crate::application::progress::Phase::Metadata,
            crate::application::progress::Phase::Readback,
        ]
    );
}

#[test]
fn backup_restore_progress_batch_retains_every_event_in_order() {
    let mut hub = TaskHub::new();
    let operation_id = hub.begin_operation().unwrap();
    for event in [
        crate::application::WriteEvent::BackupCreated {
            path: std::path::PathBuf::from("a.edpb"),
        },
        crate::application::WriteEvent::RestoreWriteCompleted,
    ] {
        hub.tx
            .send(WorkerResult::WriteProgress {
                operation_id,
                event,
            })
            .unwrap();
    }
    let updates = hub.poll();
    assert_eq!(updates.write_progress.len(), 2);
    assert!(matches!(
        updates.write_progress[0].1,
        crate::application::WriteEvent::BackupCreated { .. }
    ));
    assert!(matches!(
        updates.write_progress[1].1,
        crate::application::WriteEvent::RestoreWriteCompleted
    ));
}

#[test]
fn critical_worker_can_be_joined_after_ui_failure() {
    let mut hub = TaskHub::new();
    let operation_id = hub.begin_operation().expect("operation");
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_completed = completed.clone();
    hub.critical_worker = Some(std::thread::spawn(move || {
        worker_completed.store(true, std::sync::atomic::Ordering::SeqCst);
    }));

    hub.wait_for_critical_operation();
    assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(hub.active_operation(), None);
    assert!(operation_id.get() > 0);
}

#[test]
fn refresh_while_scanning_keeps_one_latest_follow_up() {
    let mut hub = TaskHub::new();
    assert!(hub.device_slot.single_flight.try_start());

    hub.request_device_scan(PathBuf::from("first"));
    hub.request_device_scan(PathBuf::from("latest"));

    assert_eq!(
        hub.device_slot
            .pending_latest
            .as_ref()
            .map(|(_, request)| request.root.as_path()),
        Some(std::path::Path::new("latest"))
    );
    assert!(hub.device_slot.single_flight.is_running());
}

#[test]
fn verify_keeps_only_the_latest_queued_target() {
    let mut hub = TaskHub::new();
    assert!(hub.verify_slot.single_flight.try_start());
    hub.request_backup_verify(PathBuf::from("one.bin"), PathBuf::from("backups"));
    hub.request_backup_verify(PathBuf::from("two.bin"), PathBuf::from("backups"));
    assert!(matches!(
        hub.verify_slot.pending_latest.as_ref(),
        Some((_, (path, _))) if path == &PathBuf::from("two.bin")
    ));
}

#[test]
fn source_password_requests_keep_one_latest_per_domain_and_cancel_pending_secrets() {
    let mut hub = TaskHub::new();
    hub.retain_password_session(Some(99));
    for slot in &mut hub.provision.password_verify_slots {
        assert!(slot.single_flight.try_start());
    }
    for revision in 0..1000 {
        for domain in [
            crate::provision::KeyDomainRole::Share,
            crate::provision::KeyDomainRole::Encrypt,
        ] {
            hub.request_source_password_verify_session(
                4,
                domain,
                "test-value".into(),
                revision,
                99,
            )
            .unwrap();
        }
    }
    for slot in &hub.provision.password_verify_slots {
        assert!(slot.single_flight.is_running());
        assert_eq!(slot.pending_latest.as_ref().unwrap().1.revision, 999);
    }
    hub.retain_password_session(None);
    for slot in &hub.provision.password_verify_slots {
        assert!(slot.pending_latest.is_none());
        assert!(slot.single_flight.is_running()); // running read completes, never force-terminate raw I/O
    }
}

#[test]
fn background_work_polling_reflects_singleflight_lifecycle() {
    let mut hub = TaskHub::new();
    assert!(!hub.has_pending_work());
    let generation = hub.device_slot.try_begin().unwrap();
    assert!(hub.has_pending_work());
    assert!(hub.device_slot.finish(generation));
    assert!(!hub.has_pending_work());
}
