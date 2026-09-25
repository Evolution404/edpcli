use edpcli::disk_scan::Row;
use edpcli::tui::state::AppState;

fn device() -> Row {
    Row {
        disk: 6,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        device_id: Some("disk&ven_demo&prod_u335".into()),
        onlyid: Some("ABCDEF0123456789".into()),
        dept: Some("输电运检中心".into()),
        user: Some("张三".into()),
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 3,
        denied: false,
        probe_error: None,
        is_nopwd: false,
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        partitions: None,
    }
}

#[test]
fn selected_device_is_exposed_for_dashboard_detail_panel() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);

    let selected = state.selected_device().expect("selected device");
    assert_eq!(selected.disk, 6);
    assert_eq!(selected.onlyid.as_deref(), Some("ABCDEF0123456789"));
    assert_eq!(selected.n_baks, 3);
}

#[test]
fn device_dashboard_renders_identity_detail_fields() {
    let render = include_str!("../src/tui/devices/render.rs");
    for field in ["onlyid", "device_id", "已有备份"] {
        assert!(
            render.contains(field),
            "device detail panel must render {field}"
        );
    }
}
