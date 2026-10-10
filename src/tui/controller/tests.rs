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
    assert_eq!(
        state.provision_total_sectors(),
        Some(255_944_818_688 / 4096)
    );
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
        share_source_algorithm: Some(crate::provision::OfficialLabelAlgorithm::Sms4),
        encrypt_source_algorithm: Some(crate::provision::OfficialLabelAlgorithm::Sms4),
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
    // Native geometry is not a separate TUI-only write permission: the
    // shared native planner and transaction revalidate all physical facts.
    assert!(state
        .provision_request()
        .as_ref()
        .err()
        .is_none_or(|e| !e.contains("4Kn")));

    // The 4Kn source-bound audit has an independent review state which cannot
    // be converted into PreparedProvision or accepted by Enter / Export.
    state.provision_mut().kind = ProvisionKind::Mode1;
    assert!(state.provision_unique_mode0_native_backup().is_err());
    let backup = |name: &str| crate::application::BackupWorkspaceItem {
        display_cached: false,
        index: 1,
        path: std::path::PathBuf::from(format!("/tmp/{name}.edpb")),
        file_name: format!("{name}.edpb"),
        display_time: "2026-10-10".into(),
        size_bytes: Some(255_944_818_688),
        vid: Some("3535".into()),
        pid: Some("0901".into()),
        device_id: Some("disk&ven_test&prod_test".into()),
        onlyid: Some("1402259934".into()),
        identity: None,
        user: None,
        dept: None,
        provision_kind: Some(DiskProvisionKind::Mode0),
        integrity_status:
            crate::infrastructure::backup_store::catalog::BackupIntegrityStatus::Verified,
        size_ok: true,
        verification_error: None,
        content_sha256: Some("a".repeat(64)),
        coverage: None,
        restore_preview: None,
    };
    state.replace_backups(vec![backup("a")]);
    assert_eq!(
        state.provision_unique_mode0_native_backup().unwrap(),
        std::path::PathBuf::from("/tmp/a.edpb")
    );
    state.replace_backups(vec![backup("a"), backup("b")]);
    assert!(state
        .provision_unique_mode0_native_backup()
        .unwrap_err()
        .contains("多份"));
    let mut corrupt = backup("bad");
    corrupt.verification_error = Some("SHA256 mismatch".into());
    state.replace_backups(vec![corrupt]);
    assert!(state.provision_unique_mode0_native_backup().is_err());
    state.provision_set_planning();
    state.provision_finish_native_readonly_plan(Ok(
        crate::application::provision::native_preflight::Native4knReadOnlyPreflight {
            disk: 6,
            device_identity: "disk&ven_test&prod_test".into(),
            source_backup_sha256: "a".repeat(64),
            planned_write_sha256: "b".repeat(64),
            verified_original_blocks: 17,
            write_blocks: 100,
            format_block_count: 84,
            logical_sector_bytes: 4096,
            total_sectors: 255_944_818_688 / 4096,
            first_partition_lba: 63,
            first_partition_sectors: 49_979_361,
            preserved_encrypted_partition_lba: 49_979_648,
        },
    ));
    assert_eq!(state.provision().stage, ProvisionStage::Review);
    assert!(state.provision().native_readonly_review.is_some());
    assert!(state.provision().prepared.is_none());
    for action in [TuiAction::Activate, TuiAction::Export] {
        let outcome = super::provision::dispatch_provision(&mut state, action, 20)
            .expect("read-only native review handles action");
        assert!(outcome.handled);
        assert_eq!(state.provision().stage, ProvisionStage::Review);
        assert!(state.provision().prepared.is_none());
    }
    state.provision_begin_confirm();
    state.provision_begin_export();
    assert_eq!(state.provision().stage, ProvisionStage::Review);
    assert!(state.provision_take_for_write().is_none());
    assert!(state.provision_take_export().is_none());
    state.provision_return_review_to_form();
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert!(state.provision().native_readonly_review.is_none());

    // Previously all 4Kn non-Mode0->Mode1 choices hit the special-source
    // hard refusal. Plain targets now get an independent native-LBA draft.
    state.provision_mut().scheme_selected = 4;
    state.provision_begin_selected();
    let draft = state.provision_native_geometry_readonly_plan().unwrap();
    assert_eq!(draft.source_kind, DiskProvisionKind::Mode0);
    assert_eq!(draft.target_kind, ProvisionKind::Plain);
    assert_eq!(draft.total_sectors, 255_944_818_688 / 4096);
    assert!(draft.lce_lba.is_none());
    state.provision_set_planning();
    state.provision_finish_native_geometry_readonly_plan(Ok(draft));
    assert_eq!(state.provision().stage, ProvisionStage::Review);
    assert!(state.provision().native_geometry_review.is_some());
    assert!(state.provision().prepared.is_none());
    for action in [TuiAction::Activate, TuiAction::Export] {
        super::provision::dispatch_provision(&mut state, action, 20).unwrap();
        assert_eq!(state.provision().stage, ProvisionStage::Review);
        assert!(state.provision_take_for_write().is_none());
        assert!(state.provision_take_export().is_none());
    }
    state.provision_return_review_to_form();

    // Same-mode Mode1->Mode1 also has a native read-only draft, not a
    // misleading requirement that its source must be Mode0.
    let mut another = crate::disk_scan::Row {
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
        provision_kind: DiskProvisionKind::Mode1,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: Some(crate::backup_metadata::Lba7CompatibilityGeometry {
            start_lba: 62_476_561,
            sector_count: 1,
            lba7_pointer_entries: vec![],
            official_partition_mode: Some("mode1".into()),
            chs_expected_start_lba: None,
        }),
    };

    let mut new_pin = crate::media_identity::MediaIdentitySnapshot::default();
    new_pin.hardware.logical_sector_size = Some(4096);
    new_pin.hardware.total_sectors = Some(another.size / 4096);
    new_pin.protocol.provision_kind = Some(DiskProvisionKind::Mode1);
    new_pin.protocol.device_id = another.device_id.clone();
    new_pin.protocol.onlyid = another.onlyid.clone();
    another.identity_pin = Some(crate::media_identity::MediaIdentityPin::new(
        new_pin,
        &[0u8; 512],
    ));
    state.replace_devices(vec![another]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    state.provision_mut().scheme_selected = 1;
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    let draft = state.provision_native_geometry_readonly_plan().unwrap();
    assert_eq!(draft.source_kind, DiskProvisionKind::Mode1);
    assert_eq!(draft.target_kind, ProvisionKind::Mode1);
    assert!(draft.lce_is_source_verified);
    assert_eq!(draft.sector_bytes, 4096);
    assert!(!draft.partitions.is_empty());
    state.provision_set_planning();
    state.provision_finish_native_geometry_readonly_plan(Ok(draft));
    assert!(state.provision().prepared.is_none());
    assert_eq!(state.provision().stage, ProvisionStage::Review);
    super::provision::dispatch_provision(&mut state, TuiAction::Activate, 20).unwrap();
    assert_eq!(state.provision().stage, ProvisionStage::Review);
}

#[test]
fn plain_source_can_review_native_edp_geometry_without_source_lce_or_write_permission() {
    use crate::provision::DiskProvisionKind;
    use crate::tui::state::{ProvisionKind, ProvisionStage};
    let mut row = crate::disk_scan::Row {
        disk: 7,
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
        provision_kind: DiskProvisionKind::Plain,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: None,
    };

    let mut snapshot = crate::media_identity::MediaIdentitySnapshot::default();
    snapshot.hardware.logical_sector_size = Some(4096);
    snapshot.hardware.total_sectors = Some(row.size / 4096);
    snapshot.protocol.provision_kind = Some(DiskProvisionKind::Plain);
    snapshot.protocol.device_id = row.device_id.clone();
    snapshot.protocol.onlyid = row.onlyid.clone();
    row.identity_pin = Some(crate::media_identity::MediaIdentityPin::new(
        snapshot,
        &[0u8; 512],
    ));
    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(7));
    state.provision_mut().scheme_selected = 1;
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    let draft = state.provision_native_geometry_readonly_plan().unwrap();
    assert_eq!(draft.source_kind, DiskProvisionKind::Plain);
    assert_eq!(draft.target_kind, ProvisionKind::Mode1);
    assert!(!draft.lce_is_source_verified);
    assert_eq!(draft.lce_lba, Some(62_476_561)); // Same translated-CHS target as native CLI
    assert!(!draft.partitions.is_empty());
    state.provision_set_planning();
    state.provision_finish_native_geometry_readonly_plan(Ok(draft));
    assert_eq!(state.provision().stage, ProvisionStage::Review);
    assert!(state.provision().native_geometry_review.is_some());
    assert!(state.provision().prepared.is_none());
    for action in [TuiAction::Activate, TuiAction::Export] {
        super::provision::dispatch_provision(&mut state, action, 20).unwrap();
        assert_eq!(state.provision().stage, ProvisionStage::Review);
        assert!(state.provision_take_for_write().is_none());
    }
}

#[test]
fn virtual_4kn_disk_uses_native_prepared_review_and_write_eligibility() {
    use crate::application::provision::native_flow::{
        NativePreparedProvision, NativePreviewPartition,
    };
    use crate::domain::hardware::{HardwareProbe, InquiryInfo, NativeTransport};
    use crate::filesystem::FilesystemKind;
    use crate::provision::{DiskProvisionKind, ProvisionTarget};
    use crate::tui::state::ProvisionStage;

    let total = 131_072u64;
    let mut identity = crate::media_identity::MediaIdentitySnapshot::default();
    identity.hardware.logical_sector_size = Some(4096);
    identity.hardware.total_sectors = Some(total);
    identity.protocol.provision_kind = Some(DiskProvisionKind::Plain);
    let raw_source = vec![0; 13 * 512];
    let pin = crate::media_identity::MediaIdentityPin::new(identity.clone(), &raw_source);
    let row = crate::disk_scan::Row {
        disk: 6,
        size: total * 4096,
        vid: "0dd8".into(),
        pid: "2005".into(),
        proto: "Disk Image".into(),
        serial: None,
        hardware_model: Some("EDPTEST DiskImage".into()),
        device_id: None,
        identity_pin: Some(pin.clone()),
        onlyid: None,
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
        provision_kind: DiskProvisionKind::Plain,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: None,
    };

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    let plan =
        crate::application::provision::native_image::plan_native_plain_image(total, 4096, &[])
            .unwrap();
    let native = NativePreparedProvision {
        disk: 6,
        source: DiskProvisionKind::Plain,
        target: ProvisionTarget::Plain,
        device_id: "disk&ven_EDPTEST&prod_DiskImage".into(),
        plan,
        source_native_prefix: vec![vec![0; 4096]; 13],
        source_protocol_projection: raw_source,
        before_pin: pin,
        probe: HardwareProbe {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            transport: NativeTransport::Uas,
            windows_pnp_instance_id: None,
            inquiry: Some(InquiryInfo {
                vendor: "EDPTEST".into(),
                product: "DiskImage".into(),
                revision: "1.00".into(),
            }),
        },
        onlyid: None,
        lce_extent: None,
        partitions: vec![NativePreviewPartition {
            role: None,
            start_lba: 2048,
            sector_count: total - 2048,
            filesystem: Some(FilesystemKind::ExFat),
            formatted: true,
            physically_encrypted: false,
            disposition: None,
            password_disposition: None,
        }],
    };
    state.provision_set_planning();
    state.provision_finish_plan(Ok(
        crate::application::provision::PreparedProvision::Native(Box::new(native)),
    ));
    assert_eq!(state.provision().stage, ProvisionStage::Review);
    let preview = state.provision_confirmation_view_model().unwrap();
    assert_eq!(preview.target.disk, 6);
    assert_eq!(preview.layout.logical_sector_bytes, 4096);
    assert_eq!(
        preview.layout.sector_byte_len(preview.target.total_sectors),
        Some(total * 4096)
    );
    assert_eq!(
        preview.layout.sector_byte_len(total - 2048),
        Some((total - 2048) * 4096)
    );
    let primary = preview
        .regions
        .iter()
        .find(|region| region.sector_count == total - 2048)
        .expect("native 4Kn partition in complete review");
    assert_eq!(
        primary.data_effect,
        crate::tui::state::ProvisionConfirmationDataEffect::Clear
    );
    assert!(state.provision().prepared.is_some());
}
