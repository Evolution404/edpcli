//! CLI 端到端: 子进程(离线 convert / 用法错误 / 系统盘防护)
//! + 进程内 apply/restore 流程(防重复写入守卫、写扇集合、还原交互)。
//!
//! 全部不碰真盘。

mod common;

use std::fs;
use std::process::Command;

use common::*;
use nopwd::cli::{apply_flow, restore_flow, Ctx};
use nopwd::common::{SECTOR, EXIT_ALREADY_NOPWD, EXIT_BACKUP, EXIT_CANCELLED, EXIT_OK, EXIT_TARGET};
use nopwd::diskio::FileDev;

// ══════════════════════════════════════════════════════════════════
// 子进程测试(真二进制)
// ══════════════════════════════════════════════════════════════════
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nopwd"))
}

#[test]
fn offline_convert_matches_golden() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("cli_offline");
    let snap = tmp.0.join("snap");
    fs::create_dir_all(&snap).unwrap();
    for lba in 0..14u32 {
        fs::write(
            snap.join(format!("LBA{:02}.bin", lba)),
            &data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR],
        )
        .unwrap();
    }
    let out = tmp.0.join("out");
    let r = bin()
        .args(["convert", "--dir", snap.to_str().unwrap(), "--id", "disk&ven_netac&prod_onlydisk",
               "--out", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let g = golden("netac");
    for (lba, want) in [(0u32, g.lba0), (6, g.lba6), (7, g.lba7), (12, g.lba12)] {
        let got = md5(&fs::read(out.join(format!("LBA{:02}.bin", lba))).unwrap());
        assert_eq!(got, want, "LBA{}", lba);
    }
    // netac LBA9 非零 → 清零产物
    assert_eq!(fs::read(out.join("LBA09.bin")).unwrap(), vec![0u8; SECTOR]);
    let stdout = String::from_utf8_lossy(&r.stdout);
    assert!(stdout.contains("59.75GB"), "{}", stdout); // 布局回显
}

#[test]
fn offline_requires_id() {
    let tmp = TmpDir::new("cli_noid");
    let snap = tmp.0.join("snap");
    fs::create_dir_all(&snap).unwrap();
    let r = bin().args(["convert", "--dir", snap.to_str().unwrap()]).output().unwrap();
    assert_eq!(r.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("--id"), "{}", stderr);
}

#[test]
fn system_disk_refused_without_elevation() {
    // 系统盘拒绝发生在提权之前: 无 sudo 提示, 直接退出码 3
    let r = bin().args(["run", "--disk", "1"]).output().unwrap();
    assert_eq!(r.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("系统盘"), "{}", stderr);
    assert!(!stderr.contains("sudo"), "{}", stderr);
}

#[test]
fn usage_errors_exit_two() {
    for args in [vec!["bogus"], vec!["run", "--disk"], vec!["run", "--disk", "x"],
                 vec!["apply", "--size", "abc"], vec!["run", "--force"]] {
        let r = bin().args(&args).output().unwrap();
        assert_eq!(r.status.code(), Some(2), "{:?} 应为用法错误", args);
    }
}

// ══════════════════════════════════════════════════════════════════
// 进程内流程测试(apply 守卫 / restore)
// ══════════════════════════════════════════════════════════════════
fn ctx<'a>(runner: &'a FakeRunner, prompt: &'a mut dyn nopwd::cli::Prompter, bak: &'a std::path::Path) -> Ctx<'a> {
    Ctx {
        runner,
        clock: &FixedClockForCli,
        prompt,
        backup_dir: bak.to_path_buf(),
    }
}

struct FixedClockForCli;
impl nopwd::diskio::Clock for FixedClockForCli {
    fn now_epoch(&self) -> i64 { 1789603200 }
    fn fmt_ts(&self, _e: i64) -> String { "20260917_000000".into() }
    fn fmt_human(&self, _e: i64) -> String { "2026-09-17 00:00".into() }
}

fn assert_lbas(img: &[u8], expect: &[u8], lbas: &[usize]) {
    for &l in lbas {
        assert_eq!(&img[l * SECTOR..(l + 1) * SECTOR], &expect[l * SECTOR..(l + 1) * SECTOR], "LBA{}", l);
    }
}

#[test]
fn apply_refuses_without_force() {
    let Some((conv, did)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    let tmp = TmpDir::new("apply_refuse");
    let bak = tmp.0.join("bak");
    fs::create_dir_all(&bak).unwrap();
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &conv).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let e = apply_flow(true, false, 6, None, &mut ctx(&runner, &mut prompt, &bak), &mut dev).unwrap_err();
    assert_eq!(e.code, EXIT_ALREADY_NOPWD);
    assert!(e.msg.contains("nopwd apply --disk 6 --force"), "{}", e.msg);
    // 未备份未写盘: 镜像逐字节未动, 备份目录空
    assert_eq!(fs::read(&img_path).unwrap(), conv);
    assert!(fs::read_dir(&bak).unwrap().count() == 0);
    let _ = did;
}

#[test]
fn apply_force_writes_same_sectors_and_tags_backup() {
    let Some((conv, did)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    let tmp = TmpDir::new("apply_force");
    let bak = tmp.0.join("bak");
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &conv).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let code = apply_flow(true, true, 6, None, &mut ctx(&runner, &mut prompt, &bak), &mut dev).unwrap();
    assert_eq!(code, EXIT_OK);
    let after = fs::read(&img_path).unwrap();
    // 免密盘重写: 只写 {0,6,7,12}(LBA9 已零不写) — 内容不变
    assert_lbas(&after, &conv, &[0, 6, 7, 12]);
    assert_lbas(&after, &conv, &[9]); // 未动
    // 备份已建且打 _nopwd 标
    let mut names: Vec<String> = fs::read_dir(&bak)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.retain(|n| n.ends_with(".bin"));
    assert_eq!(names.len(), 1);
    assert!(names[0].contains("_nopwd"), "{:?}", names);
    let _ = did;
}

#[test]
fn apply_original_disk_not_blocked_and_dry_run_no_write() {
    let (Some(orig), Some((conv, _))) = (load_disk_image("netac"), converted_image("netac")) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    // 原盘: 正常写入 {0,6,7,12,9}
    let tmp = TmpDir::new("apply_orig");
    let bak = tmp.0.join("bak");
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let code = apply_flow(true, false, 6, None, &mut ctx(&runner, &mut prompt, &bak), &mut dev).unwrap();
    assert_eq!(code, EXIT_OK);
    let after = fs::read(&img_path).unwrap();
    assert_lbas(&after, &orig, &[1, 2, 3, 4, 5, 8, 10, 11, 13]); // 非目标扇区不动
    // 5 个目标扇区 == 合成免密镜像(注意 netac 的 LBA6 转换是恒等: 0x1CA 原本即 128480)
    assert_lbas(&after, &conv, &[0, 6, 7, 9, 12]);
    // dry-run: 不写盘
    let tmp2 = TmpDir::new("apply_dry");
    let img2 = tmp2.0.join("disk.img");
    fs::write(&img2, &orig).unwrap();
    let mut prompt2 = ScriptPrompter::yes();
    let mut dev2 = FileDev::open_rdwr(img2.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let code2 = apply_flow(false, false, 6, None, &mut ctx(&runner, &mut prompt2, &tmp2.0.join("bak2")), &mut dev2).unwrap();
    assert_eq!(code2, EXIT_OK);
    assert_eq!(fs::read(&img2).unwrap(), orig);
}

#[test]
fn apply_cancel_at_prompt_leaves_disk_untouched() {
    let Some((_conv, _)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    let tmp = TmpDir::new("apply_cancel");
    let bak = tmp.0.join("bak");
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, load_disk_image("netac").unwrap()).unwrap(); // 原盘 → 不会被拒
    let orig = fs::read(&img_path).unwrap();
    let mut prompt = ScriptPrompter { inputs: vec!["no".into()], idx: 0 };
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let e = apply_flow(true, false, 6, None, &mut ctx(&runner, &mut prompt, &bak), &mut dev).unwrap_err();
    assert_eq!(e.code, EXIT_CANCELLED);
    assert_eq!(fs::read(&img_path).unwrap(), orig); // 已备份但未写盘
}

#[test]
fn restore_explicit_nopwd_backup_blocked() {
    let Some((conv, did)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_block");
    // 当前盘是免密盘(conv), 备份也是免密快照 → 硬拦截不写入
    let bakfile = tmp.0.join("conv.bin");
    fs::write(&bakfile, &conv).unwrap();
    fs::write(tmp.0.join("conv.bin.md5"), format!("{}\n", md5(&conv))).unwrap();
    let img_path = tmp.0.join("disk.img");
    let original = load_disk_image("netac").unwrap();
    fs::write(&img_path, &original).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
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
fn restore_detects_nopwd_from_backup_name_when_current_device_id_is_unavailable() {
    let Some((conv, _)) = converted_image("netac") else {
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
        "disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260917_120000.bin",
    );
    fs::write(&bakfile, &conv).unwrap();
    fs::write(
        format!("{}.md5", bakfile.display()),
        format!("{}\n", md5(&conv)),
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
        "即使当前盘 device_id 识别失败，也必须从备份文件名识别免密快照并拒绝写入"
    );
}

#[test]
fn restore_refuses_unknown_backup_identity_when_device_id_is_unavailable() {
    let Some((conv, _)) = converted_image("netac") else {
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
        format!("{}.md5", bakfile.display()),
        format!("{}\n", md5(&conv)),
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
    assert!(e.msg.contains("device_id") || e.msg.contains("身份"), "{}", e.msg);
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
        "disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260917_120000.bin",
    );
    fs::write(&bakfile, &original).unwrap();
    fs::write(
        format!("{}.md5", bakfile.display()),
        format!("{}\n", md5(&original)),
    )
    .unwrap();

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
    let (Some((conv, _)), ) = (converted_image("netac"), ) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_pick");
    let bak = tmp.0.join("bak");
    fs::create_dir_all(&bak).unwrap();
    // 两份备份: 旧的(原盘内容)较新、新的(免密快照)较旧 → 选择 1 = 最新(mtime 大者)
    let older = bak.join("disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260101_000000.bin");
    let newer = bak.join("disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260916_230000.bin");
    fs::write(&older, &orig).unwrap();
    fs::write(format!("{}.md5", older.display()), format!("{}\n", md5(&orig))).unwrap();
    fs::write(&newer, &conv).unwrap();
    fs::write(format!("{}.md5", newer.display()), format!("{}\n", md5(&conv))).unwrap();
    set_mtime(&older, 1_700_000_000);
    set_mtime(&newer, 1_790_000_000);
    // 当前盘为原盘; 选择"1"(最新 = 免密快照) → 应被硬拦截不写入
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter { inputs: vec!["1".into(), "YES".into()], idx: 0 };
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let code = restore_flow(None, 26, &mut ctx(&runner, &mut prompt, &bak), &mut dev).unwrap();
    assert_eq!(code, EXIT_OK);
    assert_eq!(fs::read(&img_path).unwrap(), orig); // 免密快照 → 未写

    // 选择"2"(原盘备份) + YES → 完整写入 14 扇(内容同原盘, 校验写路径无异常)
    let mut prompt2 = ScriptPrompter { inputs: vec!["2".into(), "YES".into()], idx: 0 };
    let mut dev2 = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let code2 = restore_flow(None, 26, &mut ctx(&runner, &mut prompt2, &bak), &mut dev2).unwrap();
    assert_eq!(code2, EXIT_OK);
    assert_eq!(fs::read(&img_path).unwrap(), orig);
}

#[test]
fn restore_md5_mismatch_rejected() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_md5");
    let bakfile = tmp.0.join("broken.bin");
    fs::write(&bakfile, &orig).unwrap();
    fs::write(tmp.0.join("broken.bin.md5"), "deadbeefdeadbeefdeadbeefdeadbeef\n").unwrap();
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let e = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(e.code, EXIT_BACKUP);
    assert!(e.msg.contains("MD5"), "{}", e.msg);
}

#[test]
fn restore_accepts_standard_md5_sidecar_format_case_insensitively() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_md5_standard_format");
    let bakfile = tmp.0.join(
        "disk26_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260917_120000.bin",
    );
    fs::write(&bakfile, &orig).unwrap();
    fs::write(
        format!("{}.md5", bakfile.display()),
        format!(
            "{}  {}\n",
            md5(&orig).to_uppercase(),
            bakfile.file_name().unwrap().to_string_lossy()
        ),
    )
    .unwrap();
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
fn restore_missing_md5_is_rejected() {
    let Some(orig) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(26);
    let tmp = TmpDir::new("restore_missing_md5");
    let bakfile = tmp.0.join("missing-md5.bin");
    fs::write(&bakfile, &orig).unwrap();
    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &orig).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev =
        FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let err = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();
    assert_eq!(err.code, EXIT_BACKUP);
    assert!(err.msg.contains(".md5"), "{}", err.msg);
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
    let bakfile = tmp.0.join("other-disk.bin");
    fs::write(&bakfile, &other).unwrap();
    fs::write(
        tmp.0.join("other-disk.bin.md5"),
        format!("{}\n", md5(&other)),
    )
    .unwrap();

    let img_path = tmp.0.join("disk.img");
    fs::write(&img_path, &current).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev =
        FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let err = restore_flow(
        Some(bakfile.to_string_lossy().into_owned()),
        26,
        &mut ctx(&runner, &mut prompt, &tmp.0),
        &mut dev,
    )
    .unwrap_err();

    assert_eq!(err.code, EXIT_BACKUP);
    assert!(err.msg.contains("另一块盘"), "{}", err.msg);
    assert_eq!(fs::read(&img_path).unwrap(), current, "身份不匹配时不得写盘");
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
    let mut dev = FileDev::open_rdwr(img_path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let e = restore_flow(None, 26, &mut ctx(&runner, &mut prompt, &empty_bak), &mut dev).unwrap_err();
    assert_eq!(e.code, EXIT_BACKUP);
    assert!(e.msg.contains("未找到本盘备份"), "{}", e.msg);
}

#[test]
fn apply_system_disk_guard_in_flow() {
    // 流程内部的系统盘防护(disk < 2)
    let runner = netac_runner(1);
    let tmp = TmpDir::new("guard");
    let img = tmp.0.join("d.img");
    fs::write(&img, vec![0u8; 14 * SECTOR]).unwrap();
    let mut prompt = ScriptPrompter::yes();
    let mut dev = FileDev::open_rdwr(img.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let e = apply_flow(true, false, 1, None, &mut ctx(&runner, &mut prompt, &tmp.0), &mut dev).unwrap_err();
    assert_eq!(e.code, EXIT_TARGET);
    assert!(e.msg.contains("系统盘"), "{}", e.msg);
}
