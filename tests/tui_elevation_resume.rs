use std::path::PathBuf;

use edpcli::application::pin_disk_selector;
use edpcli::tui::state::{ExpectedIdentity, WriteIntent, WriteKind};
use edpcli::tui::{parse_resume_args, resume_argv};

fn resume_disk_value(argv: &[String]) -> &str {
    let index = argv
        .iter()
        .position(|arg| arg == "--_resume-disk")
        .expect("resume disk flag");
    argv.get(index + 1).expect("resume disk value")
}

#[test]
fn elevation_resume_argv_pins_backup_create_disk_as_native_selector() {
    let intent = WriteIntent {
        kind: WriteKind::BackupCreate,
        disk: 6,
        backup: None,
        expected_identity: None,
    };
    let argv = resume_argv(&intent);
    assert_eq!(resume_disk_value(&argv), pin_disk_selector(intent.disk));
}

#[test]
fn elevation_resume_argv_pins_restore_disk_and_backup() {
    let intent = WriteIntent {
        kind: WriteKind::Restore,
        disk: 9,
        backup: Some(PathBuf::from("backup/a b.bin")),
        expected_identity: None,
    };
    let argv = resume_argv(&intent);

    assert_eq!(resume_disk_value(&argv), pin_disk_selector(intent.disk));
    let backup_index = argv
        .iter()
        .position(|arg| arg == "--_resume-backup")
        .expect("resume backup flag");
    assert_eq!(
        argv.get(backup_index + 1).map(String::as_str),
        Some("backup/a b.bin")
    );
}

#[test]
fn parse_resume_accepts_an_explicit_selector_without_weakening_native_pinning() {
    let argv = vec![
        "tui".to_string(),
        "--_resume-kind".to_string(),
        "backup-create".to_string(),
        "--_resume-disk".to_string(),
        "6".to_string(),
    ];
    assert_eq!(
        parse_resume_args(&argv).expect("numeric selector remains accepted by platform parser"),
        Some(WriteIntent {
            kind: WriteKind::BackupCreate,
            disk: 6,
            backup: None,
            expected_identity: None,
        })
    );
}

#[test]
fn malformed_resume_state_fails_closed() {
    let argv = vec![
        "tui".to_string(),
        "--_resume-kind".to_string(),
        "restore".to_string(),
        "--_resume-disk".to_string(),
        "7".to_string(),
    ];
    assert!(parse_resume_args(&argv).is_err());
}

#[test]
fn elevation_resume_preserves_the_identity_the_user_confirmed() {
    // Linux 的平台原生选择器来自当前块设备枚举，目标盘不存在时会正确地
    // fail-closed——完整 argv 往返不能在无该设备的机器上断言。这里用数字
    // 选择器直接构造 resume argv，只验证身份字段(onlyid/device_id)在解析侧
    // 完整保留；平台原生选择器本身的往返由上面两个 argv 测试覆盖。
    let argv = vec![
        "tui".to_string(),
        "--_resume-kind".to_string(),
        "backup-create".to_string(),
        "--_resume-disk".to_string(),
        "6".to_string(),
        "--_resume-onlyid".to_string(),
        "1402259934".to_string(),
        "--_resume-device-id".to_string(),
        "disk&ven_netac&prod_onlydisk".to_string(),
    ];
    assert_eq!(
        parse_resume_args(&argv).unwrap(),
        Some(WriteIntent {
            kind: WriteKind::BackupCreate,
            disk: 6,
            backup: None,
            expected_identity: Some(ExpectedIdentity {
                onlyid: Some("1402259934".into()),
                device_id: Some("disk&ven_netac&prod_onlydisk".into()),
            }),
        })
    );
}
