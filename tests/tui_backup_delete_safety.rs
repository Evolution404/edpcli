mod common;

use std::fs;
use std::path::{Path, PathBuf};

use edpcli::application::{delete_backup_exact, scan_backup_workspace};
use edpcli::diskio::md5_sidecar_path;

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
    let target = root.join(name);
    fs::copy(&source, &target).expect("copy fixture");
    let bytes = fs::read(&target).expect("read copied fixture");
    fs::write(
        md5_sidecar_path(&target),
        format!("{}\n", edpcli::md5::md5_hex(&bytes)),
    )
    .expect("write md5 sidecar");
    target
}

fn expected_md5(root: &Path, path: &Path) -> String {
    scan_backup_workspace(root)
        .into_iter()
        .find(|row| row.path == path)
        .and_then(|row| row.content_md5)
        .expect("scanned backup content md5")
}

#[test]
fn delete_rejects_a_backup_replaced_after_selection() {
    let tmp = common::TmpDir::new("tui_backup_delete_stale");
    let target = copy_fixture(&tmp.0, ORIGINAL);
    let _keep = copy_fixture(&tmp.0, SNAPSHOT);
    let expected = expected_md5(&tmp.0, &target);

    let mut changed = fs::read(&target).expect("read target");
    changed[0] ^= 0x5a;
    fs::write(&target, changed).expect("replace target bytes");

    let error = delete_backup_exact(&tmp.0, &target, &expected).expect_err("must fail closed");
    assert!(error.contains("已变化"), "{error}");
    assert!(target.exists(), "stale replacement must never be deleted");
}

#[test]
fn delete_removes_selected_backup_and_its_sidecar_when_another_copy_remains() {
    let tmp = common::TmpDir::new("tui_backup_delete_pair");
    let target = copy_fixture(&tmp.0, ORIGINAL);
    let keep = copy_fixture(&tmp.0, SNAPSHOT);
    let sidecar = md5_sidecar_path(&target);
    let expected = expected_md5(&tmp.0, &target);

    delete_backup_exact(&tmp.0, &target, &expected).expect("delete selected backup");

    assert!(!target.exists());
    assert!(!sidecar.exists());
    assert!(keep.exists());
}

#[test]
fn delete_refuses_to_remove_the_last_backup_for_a_device() {
    let tmp = common::TmpDir::new("tui_backup_delete_last");
    let target = copy_fixture(&tmp.0, ORIGINAL);
    let expected = expected_md5(&tmp.0, &target);

    let error = delete_backup_exact(&tmp.0, &target, &expected).expect_err("must keep one backup");
    assert!(error.contains("至少保留 1 份"), "{error}");
    assert!(target.exists());
}
