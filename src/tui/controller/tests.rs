use super::*;
use crate::tui::clipboard::ClipboardOutcome;

struct NullClipboard;

impl ClipboardBackend for NullClipboard {
    fn copy(&mut self, _content: &str) -> ClipboardOutcome {
        ClipboardOutcome::Unsupported
    }
}

#[test]
fn backup_device_tree_does_not_swallow_panel_navigation_actions() {
    let mut state = AppState::new();
    let _ = state.navigate(NavCommand::WorkspaceBackups, 20);
    state.focus_backups_pane(PaneId::BackupDevices);
    let mut clipboard = NullClipboard;

    let right = dispatch_action(
        &mut state,
        TuiAction::PanelRight,
        WidgetRole::Tree,
        20,
        120,
        &mut clipboard,
    );
    assert!(right.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);

    let left = dispatch_action(
        &mut state,
        TuiAction::PanelLeft,
        WidgetRole::Table,
        20,
        120,
        &mut clipboard,
    );
    assert!(left.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupDevices);

    let down = dispatch_action(
        &mut state,
        TuiAction::PanelDown,
        WidgetRole::Tree,
        20,
        120,
        &mut clipboard,
    );
    assert!(down.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupSummary);

    let up = dispatch_action(
        &mut state,
        TuiAction::PanelUp,
        WidgetRole::Other,
        20,
        120,
        &mut clipboard,
    );
    assert!(up.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);

    state.focus_backups_pane(PaneId::BackupDevices);
    let next = dispatch_action(
        &mut state,
        TuiAction::PanelNext,
        WidgetRole::Tree,
        20,
        120,
        &mut clipboard,
    );
    assert!(next.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
}

#[test]
fn native_4kn_mode0_requests_readonly_key_probe_and_independent_password_verification() {
    use crate::provision::{DiskProvisionKind, KeyDomainRole, SourcePasswordKnowledge};
    use crate::tui::state::{ProvisionKind, ProvisionPasswordVerificationState, ProvisionStage};

    let mut row = crate::disk_scan::Row {
        disk: 6,
        size: 255_944_818_688,
        vid: "3535".into(),
        pid: "0901".into(),
        proto: "USB".into(),
        serial: None,
        hardware_model: None,
        device_id: Some("disk&ven_test&prod_test".into()),
        identity_pin: None,
        onlyid: Some("1402259934".into()),
        dept: None,
        user: None,
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 0,
        n_possible_baks: 0,
        denied: false,
        probe_error: None,
        provision_kind: DiskProvisionKind::Mode0,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: None,
    };
    let mut snapshot = crate::media_identity::MediaIdentitySnapshot::default();
    snapshot.hardware.logical_sector_size = Some(4096);
    snapshot.hardware.total_sectors = Some(row.size / 4096);
    snapshot.protocol.provision_kind = Some(DiskProvisionKind::Mode0);
    snapshot.protocol.device_id = row.device_id.clone();
    snapshot.protocol.onlyid = row.onlyid.clone();
    row.identity_pin = Some(crate::media_identity::MediaIdentityPin::new(
        snapshot,
        &[0u8; 512],
    ));
    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    let outcome = super::provision::dispatch_provision(&mut state, TuiAction::Activate, 20)
        .expect("scheme activation handled");
    assert!(matches!(
        outcome.request,
        Some(ActionRequest::ProvisionKeyProbe { disk: 6 })
    ));
    assert_eq!(state.provision().kind, ProvisionKind::Mode0);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(
        state.provision().share_source_verification,
        ProvisionPasswordVerificationState::Verifying
    );
    assert_eq!(
        state.provision().encrypt_source_verification,
        ProvisionPasswordVerificationState::Verifying
    );

    // Default attempt is unknown; typed source values are independently
    // verified via the read-only application service, not a 512B-only shim.
    state.provision_finish_key_probe(Ok(crate::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::Unknown),
        encrypt: Some(SourcePasswordKnowledge::Unknown),
        share_opaque_profile: true,
        encrypt_opaque_profile: true,
    }));
    let fields = state.provision_visible_fields();
    let source_fields = fields
        .iter()
        .enumerate()
        .filter(|(_, f)| f.0 == "原密码")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(source_fields.len(), 2);
    for (idx, domain) in [KeyDomainRole::Share, KeyDomainRole::Encrypt]
        .into_iter()
        .enumerate()
    {
        state.provision_mut().field_selected = source_fields[idx];
        let (selected, _candidate, revision) = state
            .provision_source_password_verify_request()
            .unwrap()
            .unwrap();
        assert_eq!(selected, domain);
        state.provision_finish_source_password_verify(
            domain,
            revision,
            Ok(SourcePasswordKnowledge::UserVerified),
        );
    }
    assert_eq!(
        state.provision().form.share_source_knowledge,
        SourcePasswordKnowledge::UserVerified
    );
    assert_eq!(
        state.provision().form.encrypt_source_knowledge,
        SourcePasswordKnowledge::UserVerified
    );
    // Source authentication cannot make an unapproved 4Kn physical plan.
    assert!(state.provision_request().unwrap_err().contains("4Kn"));
}
