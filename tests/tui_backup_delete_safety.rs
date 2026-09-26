use crate::common;

use std::fs;
use std::path::{Path, PathBuf};

use edpcli::application::{delete_backup_exact, scan_backup_workspace};
use edpcli::edpb::{self, CoreCapture};

const ORIGINAL: &str =
    "disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260827_191701.bin";
const SNAPSHOT: &str =
    "disk26_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_nopwd_20260916_233626.bin";

fn copy_fixture(root: &Path, name: &str) -> PathBuf {
    let source = Path::new(common::FIXTURE_DIR).join(name);
    assert!(
        source.is_file(),
        "missing backup fixture {}",
        source.display()
    );
    let target = root.join(name).with_extension("edpb");
    let bytes = fs::read(&source).expect("read protocol fixture");
    let meta = edpcli::diskio::parse_backup_name(target.file_name().unwrap().to_str().unwrap())
        .expect("EDPB name");
    edpb::write_core_backup(
        &target,
        &CoreCapture {
            snapshot_id: name.into(),
            created_epoch: 1_789_000_000,
            disk_number: Some(meta.disk),
            vid: meta.vid,
            pid: meta.pid,
            device_id: meta.device_id,
            onlyid: meta.onlyid,
            total_sectors: meta.secs,
            logical_sector_size: 512,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: if meta.tagged_nopwd {
                "passwordless"
            } else {
                "encrypted"
            }
            .into(),
            lba0_12: &bytes,
        },
    )
    .expect("write EDPB fixture");
    target
}

fn expected_sha256(root: &Path, path: &Path) -> String {
    scan_backup_workspace(root)
        .into_iter()
        .find(|row| row.path == path)
        .and_then(|row| row.content_sha256)
        .expect("scanned backup content sha256")
}

#[test]
fn delete_rejects_a_backup_replaced_after_selection() {
    let tmp = common::TmpDir::new("tui_backup_delete_stale");
    let target = copy_fixture(&tmp.0, ORIGINAL);
    let _keep = copy_fixture(&tmp.0, SNAPSHOT);
    let expected = expected_sha256(&tmp.0, &target);

    let mut changed = fs::read(&target).expect("read target");
    changed[0] ^= 0x5a;
    fs::write(&target, changed).expect("replace target bytes");

    let error = delete_backup_exact(&tmp.0, &target, &expected).expect_err("must fail closed");
    assert!(matches!(
        error,
        edpcli::application::backup::BackupDeleteError::Plan(
            edpcli::application::backup::DeletePlanError::Changed { .. }
        )
    ));
    assert!(target.exists(), "stale replacement must never be deleted");
}

#[test]
fn delete_removes_selected_edpb_when_another_copy_remains() {
    let tmp = common::TmpDir::new("tui_backup_delete_pair");
    let target = copy_fixture(&tmp.0, ORIGINAL);
    let keep = copy_fixture(&tmp.0, SNAPSHOT);
    let expected = expected_sha256(&tmp.0, &target);

    delete_backup_exact(&tmp.0, &target, &expected).expect("delete selected backup");

    assert!(!target.exists());
    assert!(keep.exists());
}

#[test]
fn delete_refuses_to_remove_the_last_backup_for_a_device() {
    let tmp = common::TmpDir::new("tui_backup_delete_last");
    let target = copy_fixture(&tmp.0, ORIGINAL);
    let expected = expected_sha256(&tmp.0, &target);

    let error = delete_backup_exact(&tmp.0, &target, &expected).expect_err("must keep one backup");
    assert!(matches!(
        error,
        edpcli::application::backup::BackupDeleteError::Plan(
            edpcli::application::backup::DeletePlanError::RetentionFloor
        )
    ));
    assert!(target.exists());
}
