//! CLI 端到端: 子进程(用法错误 / 系统盘防护)
//! + 进程内 backup/restore 流程(只读备份与还原安全门禁)。
//!
//! 全部不碰真盘。

#![cfg(target_os = "macos")]

use crate::common;

use std::fs;
use std::process::Command;

use common::*;
use edpcli::application::write::{backup_create_flow, restore_flow, Ctx};
use edpcli::common::{EXIT_BACKUP, EXIT_CANCELLED, EXIT_OK, EXIT_TARGET, SECTOR};
use edpcli::diskio::FileDev;
use edpcli::diskio::SectorDev;
use edpcli::edpb::{self, CoreCapture};

// ══════════════════════════════════════════════════════════════════
// 子进程测试(真二进制)
// ══════════════════════════════════════════════════════════════════
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_edpcli"))
}

#[test]
fn removed_offline_convert_command_is_a_usage_error() {
    let r = bin().arg("convert").output().unwrap();
    assert_eq!(r.status.code(), Some(2));
}

#[test]
fn system_disk_refused_without_elevation() {
    // 系统盘拒绝发生在提权之前: 无 sudo 提示, 直接退出码 3。
    let runner = edpcli::sysinfo::SysRunner;
    let system_disk = (0..128u32)
        .find(|&disk| edpcli::platform::is_system_disk(&runner, disk))
        .expect("macOS system disk");
    let r = bin()
        .args(["backup", "restore", "--disk", &system_disk.to_string()])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("系统盘"), "{}", stderr);
    assert!(!stderr.contains("sudo"), "{}", stderr);
}

#[test]
fn usage_errors_exit_two() {
    for args in [
        vec!["bogus"],
        vec!["run", "--disk"],
        vec!["run", "--disk", "x"],
        vec!["apply"],
        vec!["run", "--force"],
    ] {
        let r = bin().args(&args).output().unwrap();
        assert_eq!(r.status.code(), Some(2), "{:?} 应为用法错误", args);
    }
}

// ══════════════════════════════════════════════════════════════════
// 进程内流程测试(backup / restore)
// ══════════════════════════════════════════════════════════════════
fn ctx<'a>(
    runner: &'a FakeRunner,
    prompt: &'a mut dyn edpcli::cli::Prompter,
    bak: &'a std::path::Path,
) -> Ctx<'a> {
    Ctx {
        runner,
        clock: &FixedClockForCli,
        prompt,
        backup_dir: bak.to_path_buf(),
    }
}

struct FixedClockForCli;
impl edpcli::diskio::Clock for FixedClockForCli {
    fn now_epoch(&self) -> i64 {
        1789603200
    }
    fn fmt_ts(&self, _e: i64) -> String {
        "20260917_000000".into()
    }
    fn fmt_human(&self, _e: i64) -> String {
        "2026-09-17 00:00".into()
    }
}

fn write_test_edpb(
    path: &std::path::Path,
    data: &[u8],
    device_id: &str,
    vid: &str,
    pid: &str,
    total_sectors: u64,
    state: &str,
) {
    let onlyid = edpcli::diskio::lba4_label_id_from(&data[4 * SECTOR..5 * SECTOR]);
    let capture = CoreCapture {
        snapshot_id: path.file_name().unwrap().to_string_lossy().into_owned(),
        created_epoch: 1_789_603_200,
        disk_number: Some(26),
        vid: vid.into(),
        pid: pid.into(),
        device_id: device_id.into(),
        onlyid,
        total_sectors: Some(total_sectors),
        logical_sector_size: SECTOR as u32,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: state.into(),
        lba0_12: data,
    };
    edpb::write_core_backup(path, &capture).unwrap();
}

fn write_netac_edpb(path: &std::path::Path, data: &[u8], state: &str) {
    write_test_edpb(
        path,
        data,
        "disk&ven_netac&prod_onlydisk",
        "0dd8",
        "2005",
        122_880_000,
        state,
    );
}

fn write_lexar_edpb(path: &std::path::Path, data: &[u8], state: &str) {
    write_test_edpb(
        path,
        data,
        "disk&ven_lexar&prod_usb_flash_drive",
        "21c4",
        "0cd1",
        243_625_984,
        state,
    );
}

struct SwapOnReopenDev {
    before: Vec<u8>,
    after: Vec<u8>,
    switched: bool,
    writes: usize,
}

impl SwapOnReopenDev {
    fn new(before: Vec<u8>, after: Vec<u8>) -> Self {
        Self {
            before,
            after,
            switched: false,
            writes: 0,
        }
    }

    fn active(&self) -> &[u8] {
        if self.switched {
            &self.after
        } else {
            &self.before
        }
    }
}

impl SectorDev for SwapOnReopenDev {
    fn read_sector(&mut self, lba: u32) -> std::io::Result<Vec<u8>> {
        let start = lba as usize * SECTOR;
        let end = start + SECTOR;
        self.active()
            .get(start..end)
            .map(|bytes| bytes.to_vec())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!("test sparse disk does not materialize LBA{lba}"),
                )
            })
    }

    fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> std::io::Result<()> {
        self.writes += 1;
        Ok(())
    }

    fn reopen_rdwr(&mut self, _wait: std::time::Duration) -> std::io::Result<()> {
        self.switched = true;
        Ok(())
    }
}

#[test]
fn backup_create_is_read_only_and_verifiable() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };

    // 手动 backup create 的 runner 故意删除卸载命令：只读路径若误入
    // prepare_write/unmount，此测试会立即失败。
    let mut readonly_runner = netac_runner(6);
    readonly_runner
        .canned
        .remove("diskutil unmountDisk force disk6");
    let tmp = TmpDir::new("backup_create_readonly");
    let manual_dir = tmp.0.join("manual");
    let mut manual_prompt = ScriptPrompter {
        inputs: vec![],
        idx: 0,
    };
    let mut manual_dev = SwapOnReopenDev::new(orig.clone(), orig.clone());
    let manual = backup_create_flow(
        6,
        &mut ctx(&readonly_runner, &mut manual_prompt, &manual_dir),
        &mut manual_dev,
    )
    .unwrap();

    assert!(!manual.is_nopwd);
    assert_eq!(manual_prompt.idx, 0, "backup create 不应要求写盘确认");
    assert!(!manual_dev.switched, "backup create 不得 reopen 为读写");
    assert_eq!(manual_dev.writes, 0, "backup create 不得写 U 盘");
    assert_eq!(edpb::read_raw_protocol(&manual.path).unwrap(), orig);
    let manual_verified = edpb::verify_file(&manual.path).unwrap();
    assert_eq!(
        manual_verified.manifest.snapshot.capture_level,
        edpcli::edpb::CaptureLevel::Metadata
    );
    assert!(manual_verified
        .manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "derived.capture_issues"));

    assert!(edpb::verify_file(&manual.path).is_ok());
    assert!(!std::path::PathBuf::from(format!("{}.sha256", manual.path.display())).exists());
}

#[test]
fn restore_numeric_target_uses_backup_selector_and_current_disk_identity() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    let tmp = TmpDir::new("restore_numeric_selector");
    let backup_dir = tmp.0.join("bak");
    fs::create_dir_all(&backup_dir).unwrap();
    let target = backup_dir.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.edpb",
    );
    write_netac_edpb(&target, &orig, "encrypted");

    let mut prompt = ScriptPrompter {
        inputs: vec!["NO".into()],
        idx: 0,
    };
    let mut dev = SwapOnReopenDev::new(orig.clone(), orig);
    let error = restore_flow(
        Some("1".into()),
        6,
        &mut ctx(&runner, &mut prompt, &backup_dir),
        &mut dev,
    )
    .unwrap_err();

    assert_eq!(
        error.code, EXIT_CANCELLED,
        "编号 1 应先由 BackupSelector 解析为当前盘备份，再进入恢复确认"
    );
    assert_eq!(prompt.idx, 1);
    assert!(!dev.switched);
    assert_eq!(dev.writes, 0);
}

#[test]
fn restore_explicit_nopwd_backup_blocked() {
    let Some((conv, did)) = passwordless_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_block");
    // 当前盘是免密盘(conv), 备份也是免密快照 → 硬拦截不写入
    let bakfile = tmp.0.join("conv.edpb");
    write_netac_edpb(&bakfile, &conv, "passwordless");
    let img_path = tmp.0.join("disk.img");
    let original = load_disk_image("netac").unwrap();
    fs::write(&img_path, &original).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let code = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap();
    assert_eq!(code, EXIT_OK); // 正常返回(提示未写入), 与 Python 行为一致
    assert_eq!(fs::read(&img_path).unwrap(), original); // 未写盘
    let _ = did;
}

#[test]
fn restore_detects_nopwd_from_edpb_manifest_when_current_device_id_is_unavailable() {
    let Some((conv, _)) = passwordless_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let mut runner = netac_runner(26);
    runner.canned.remove("ioreg -r -c IOSCSITargetDevice -l");

    let tmp = TmpDir::new("restore_nopwd_without_current_did");
    let bakfile = tmp.0.join(
        "disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260917_120000.edpb",
    );
    write_netac_edpb(&bakfile, &conv, "passwordless");

    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &original).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();

    let code = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap();
    assert_eq!(code, EXIT_OK);
    assert_eq!(
        fs::read(&img_path).unwrap(),
        original,
        "即使当前盘 device_id 识别失败，也必须从 EDPB Manifest 识别免密快照并拒绝写入"
    );
}

#[test]
fn restore_rejects_legacy_bin_when_device_id_is_unavailable() {
    let Some((conv, _)) = passwordless_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let mut runner = netac_runner(26);
    runner.canned.remove("ioreg -r -c IOSCSITargetDevice -l");

    let tmp = TmpDir::new("restore_unknown_did");
    let bakfile = tmp.0.join("renamed.bin");
    fs::write(&bakfile, &conv).unwrap();
    fs::write(
        format!("{}.sha256", bakfile.display()),
        format!("{}\n", sha256(&conv)),
    )
    .unwrap();
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &original).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();

    let e = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(e.code, EXIT_BACKUP);
    assert!(e.msg.contains(".edpb"), "{}", e.msg);
    assert_eq!(fs::read(&img_path).unwrap(), original);
}

#[test]
fn restore_refuses_when_current_disk_identity_tag_is_zero() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_zero_current_identity");
    let bakfile = tmp.0.join(
        "disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260917_120000.edpb",
    );
    write_netac_edpb(&bakfile, &original, "encrypted");

    let mut current = original.clone();
    current[4 * SECTOR..4 * SECTOR + 16].fill(0);
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &current).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();

    let e = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(e.code, EXIT_BACKUP);
    assert!(e.msg.contains("身份"), "{}", e.msg);
    assert_eq!(fs::read(&img_path).unwrap(), current);
}

#[test]
fn restore_picker_selects_newest_and_writes() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let (Some((conv, _)),) = (passwordless_image("netac"),) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_pick");
    let bak = tmp.0.join("bak");
    fs::create_dir_all(&bak).unwrap();
    // 两份备份: 旧的(原盘内容)较新、新的(免密快照)较旧 → 选择 1 = 最新(mtime 大者)
    let older = bak.join("disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260101_000000.edpb");
    let newer = bak.join("disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260916_230000.edpb");
    write_netac_edpb(&older, &orig, "encrypted");
    write_netac_edpb(&newer, &conv, "passwordless");
    set_mtime(&older, 1_700_000_000);
    set_mtime(&newer, 1_790_000_000);
    // 当前盘为原盘; 选择"1"(最新 = 免密快照) → 应被硬拦截不写入
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter {
        inputs: vec!["1".into(), "YES".into()],
        idx: 0,
    };
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let code = restore_flow(None, 26, &mut ctx(&runner, &mut prompt, &bak), &mut dev).unwrap();
    assert_eq!(code, EXIT_OK);
    assert_eq!(fs::read(&img_path).unwrap(), orig); // 免密快照 → 未写

    // 选择"2"(原盘备份) + YES → 完整写入 13 扇(内容同原盘, 校验写路径无异常)
    let mut prompt2 = ScriptPrompter {
        inputs: vec!["2".into(), "YES".into()],
        idx: 0,
    };
    let mut dev2 = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let code2 = restore_flow(None, 26, &mut ctx(&runner, &mut prompt2, &bak), &mut dev2).unwrap();
    assert_eq!(code2, EXIT_OK);
    assert_eq!(fs::read(&img_path).unwrap(), orig);
}

#[test]
fn restore_edpb_payload_hash_mismatch_rejected() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_edpb_hash");
    let bakfile = tmp.0.join("broken.edpb");
    write_netac_edpb(&bakfile, &orig, "encrypted");
    let verified = edpb::verify_file(&bakfile).unwrap();
    let data_offset = verified.manifest.artifacts[0].storage.data_offset as usize;
    let mut bytes = fs::read(&bakfile).unwrap();
    bytes[data_offset + 23] ^= 0x5A;
    fs::write(&bakfile, bytes).unwrap();
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let e = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(e.code, EXIT_BACKUP);
    assert!(e.msg.contains("SHA-256"), "{}", e.msg);
}

#[test]
fn restore_valid_edpb_needs_no_external_sidecar() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_edpb_no_sidecar");
    let bakfile = tmp.0.join(
        "disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260917_120000.edpb",
    );
    write_netac_edpb(&bakfile, &orig, "encrypted");
    assert!(!std::path::PathBuf::from(format!("{}.sha256", bakfile.display())).exists());
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();

    let code = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap();
    assert_eq!(code, EXIT_OK);
    assert_eq!(fs::read(&img_path).unwrap(), orig);
}

#[test]
fn restore_truncated_edpb_is_rejected() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_truncated_edpb");
    let bakfile = tmp.0.join("truncated.edpb");
    write_netac_edpb(&bakfile, &orig, "encrypted");
    let mut bytes = fs::read(&bakfile).unwrap();
    bytes.truncate(bytes.len() - 20);
    fs::write(&bakfile, bytes).unwrap();
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let err = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(err.code, EXIT_BACKUP);
    assert!(err.msg.contains("EDPB"), "{}", err.msg);
    assert_eq!(fs::read(&img_path).unwrap(), orig);
}

#[test]
fn restore_explicit_backup_from_other_disk_is_rejected() {
    let (Some(current), Some(other)) = (load_disk_image("netac"), load_disk_image("lexar")) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_wrong_disk");
    let bakfile = tmp.0.join("other-disk.edpb");
    write_lexar_edpb(&bakfile, &other, "encrypted");

    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &current).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let err = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();

    assert_eq!(err.code, EXIT_BACKUP);
    assert!(err.msg.contains("另一块盘"), "{}", err.msg);
    assert_eq!(
        fs::read(&img_path).unwrap(),
        current,
        "身份不匹配时不得写盘"
    );
}

#[test]
fn restore_no_backup_found() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_none");
    let empty_bak = tmp.0.join("none");
    fs::create_dir_all(&empty_bak).unwrap();
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let e = restore_flow(
        None,
        26,
        &mut ctx(&runner, &mut prompt, &empty_bak),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(e.code, EXIT_BACKUP);
    assert!(e.msg.contains("未找到本盘备份"), "{}", e.msg);
}

#[test]
fn restore_refuses_if_disk_identity_changes_after_reopen() {
    let (Some(netac), Some(lexar)) = (load_disk_image("netac"), load_disk_image("lexar")) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    let tmp = TmpDir::new("restore_swap_after_reopen");
    let backup = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260917_120000.edpb",
    );
    write_netac_edpb(&backup, &netac, "encrypted");
    let mut prompt = ScriptPrompter::yes();
    let mut dev = SwapOnReopenDev::new(netac, lexar);

    let e = restore_flow(
        Some(backup.to_string_lossy().into_owned()),
        6,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(e.code, EXIT_TARGET);
    assert!(
        e.msg.contains("身份") || e.msg.contains("换盘"),
        "{}",
        e.msg
    );
    assert_eq!(dev.writes, 0, "身份变化必须在第一笔写入前拦截");
}

#[test]
fn deep_backup_create_never_unmounts_reopens_or_writes() {
    let orig = load_disk_image("netac").expect("committed netac fixture");
    let mut runner = netac_runner(6);
    runner.canned.remove("diskutil unmountDisk force disk6");
    let tmp = TmpDir::new("deep_readonly_flow");
    let mut prompt = ScriptPrompter {
        inputs: vec![],
        idx: 0,
    };
    let mut dev = SwapOnReopenDev::new(orig.clone(), orig.clone());
    let report = edpcli::application::write::backup_create_level_flow(
        6,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
        true,
    )
    .unwrap();
    assert!(!dev.switched);
    assert_eq!(dev.writes, 0);
    assert_eq!(prompt.idx, 0);
    let v = edpb::verify_file(&report.path).unwrap();
    assert_eq!(v.manifest.snapshot.capture_level, edpb::CaptureLevel::Deep);
    assert!(v
        .manifest
        .artifacts
        .iter()
        .any(|a| a.kind == "filesystem_summary"));
    assert_eq!(edpb::read_raw_protocol(&report.path).unwrap(), orig);
}
