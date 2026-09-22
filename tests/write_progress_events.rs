//! 写盘服务类型化进度事件的契约测试:
//! - `render_event_text` / `render_convert_report` 在无色模式下逐字节锁死 CLI 文本
//!   (黄金基线，防事件化改造造成输出漂移)；
//! - 带 ANSI 的样式渲染仍经 ui::wrap；
//! - apply/backup-create/restore 实际发出的事件序列(录制型 Prompter)。
//!
//! 全部不碰真盘。

mod common;

use edpcli::application::write::render_event_text;
use edpcli::application::WriteEvent;
use edpcli::common::METADATA_IMAGE_LEN;
use edpcli::sectors::{render_convert_report, ConvertReport};
use edpcli::ui;

fn plain(event: &WriteEvent) -> String {
    ui::set_enabled_for_tests(false);
    let text = render_event_text(event);
    ui::reset_enabled_for_tests();
    text
}

fn styled(event: &WriteEvent) -> String {
    ui::set_enabled_for_tests(true);
    let text = render_event_text(event);
    ui::reset_enabled_for_tests();
    text
}

fn plain_convert(report: &ConvertReport) -> String {
    ui::set_enabled_for_tests(false);
    let text = render_convert_report(report);
    ui::reset_enabled_for_tests();
    text
}

// ══════════════════════════════════════════════════════════════════
// 渲染黄金基线(纯文本)
// ══════════════════════════════════════════════════════════════════
#[test]
fn render_header_and_backup_listing_events() {
    let event = WriteEvent::ApplyDeviceHeader {
        disk: 6,
        size_text: "29.8 GB".into(),
        vid: "0dd8".into(),
        pid: "170c".into(),
    };
    assert_eq!(plain(&event), "盘  disk6 · 29.8 GB · USB 0dd8:170c\n");

    assert_eq!(
        plain(&WriteEvent::ExistingBackupsHeader { count: 2 }),
        "\n备份  本盘已有 2 份(写入时会自动再备份):\n"
    );
    let rows = vec![
        ("2026-09-17 00:00".to_string(), true),
        ("2026-09-10 08:00".to_string(), false),
    ];
    // 菜单直通 backup_menu_str，不额外补换行(旧 output! 语义)
    assert_eq!(
        plain(&WriteEvent::ExistingBackupsMenu { rows: rows.clone() }),
        ui::backup_menu_str(&rows)
    );
    assert_eq!(
        plain(&WriteEvent::NoExistingBackups),
        "\n备份  尚无; 写入时自动创建首个备份\n"
    );
}

#[test]
fn render_hint_and_dry_run_events() {
    assert_eq!(
        plain(&WriteEvent::AlreadyNopwdHint),
        "\n提示: 该盘已是改造后的免密盘 — 再次写入只会重写相同内容(实测幂等)。\n"
    );
    assert_eq!(
        plain(&WriteEvent::DryRunPreview {
            disk: 6,
            needs_force: false
        }),
        "操作  以上为预览(dry-run), 未写盘。执行写入: edpcli apply --disk 6\n"
    );
    assert_eq!(
        plain(&WriteEvent::DryRunPreview {
            disk: 6,
            needs_force: true
        }),
        "操作  以上为预览(dry-run), 未写盘。执行写入: edpcli apply --disk 6 (该盘已是免密盘, 须加 --force)\n"
    );
    assert_eq!(
        plain(&WriteEvent::ForceRewriteNotice),
        "--force: 继续重写。本次自动备份将标记为免密状态(文件名含 _nopwd); 加密原盘备份是更早时间戳那份。\n"
    );
}

#[test]
fn render_backup_and_completion_events() {
    assert_eq!(
        plain(&WriteEvent::BackupCreated {
            path: "/b/disk6.bin".into()
        }),
        "备份  /b/disk6.bin\n"
    );
    assert_eq!(
        plain(&WriteEvent::BackupCreatedIsNopwd),
        "注意: 本份备份为【免密状态】快照 — 还原它不会回到加密原盘。\n"
    );
    assert_eq!(
        plain(&WriteEvent::RestoreCommandHint {
            path: "/b/disk6.bin".into(),
            disk: 6,
        }),
        "还原  edpcli backup restore \"/b/disk6.bin\" --disk 6 --yes\n"
    );
    assert_eq!(
        plain(&WriteEvent::ApplyWriteCompleted),
        "已写入, 读回校验通过。请拔出 U 盘重新插入, 数据区格式化 exFAT/NTFS 即得免密可写区。\n"
    );
}

#[test]
fn render_restore_events() {
    assert_eq!(
        plain(&WriteEvent::RestoreMatchesHeader {
            disk: 6,
            onlyid: "1402259934".into(),
            count: 2,
        }),
        "disk6 · onlyid=1402259934 匹配备份 2 个:\n"
    );
    assert_eq!(
        plain(&WriteEvent::RestoreMatchRow {
            index: 1,
            time: "2026-09-17 00:00".into(),
            is_nopwd: true,
            file_name: "disk6_x.bin".into(),
        }),
        "  [1] 2026-09-17 00:00   免密状态   disk6_x.bin\n"
    );
    assert_eq!(
        plain(&WriteEvent::RestoreSelectionRetry {
            message: "无此编号".into()
        }),
        "无此编号\n"
    );
    assert_eq!(
        plain(&WriteEvent::BackupShaVerified {
            digest: "aaaa".into()
        }),
        "SHA-256 校验通过  aaaa\n"
    );
    assert_eq!(
        plain(&WriteEvent::RestoreSnapshotNopwdWarning),
        "注意: 该备份为【免密状态】快照 — 还原后仍是免密盘, 不会回到加密原盘。\n"
    );
    assert_eq!(
        plain(&WriteEvent::RestoreDryRunNotice {
            path: "/b/x.bin".into(),
            disk: 6,
        }),
        format!(
            "[dry-run] 将还原 /b/x.bin → disk6 LBA0-12 ({}B) — 未写入(免密快照不作还原)。\n",
            METADATA_IMAGE_LEN
        )
    );
    assert_eq!(
        plain(&WriteEvent::RestoreTargetHeader {
            path: "/b/x.bin".into()
        }),
        "还原  /b/x.bin\n"
    );
    assert_eq!(
        plain(&WriteEvent::RestoreWriteCompleted),
        "已还原, 读回校验通过。请拔出重插。\n"
    );
}

#[test]
fn render_convert_reports() {
    let identity = ConvertReport::Identity {
        device_id: "disk&ven_netac".into(),
        crc: 0x1A2B3C4D,
        k0: 0x3C4D,
    };
    assert_eq!(
        plain_convert(&identity),
        "标识  disk&ven_netac  (CRC32 0x1A2B3C4D, K0 0x3C4D)\n"
    );

    let layout = ConvertReport::Layout {
        share: 1_000_008,
        enc_start: 1_000_071,
        enc_size: 1_000_000_000,
    };
    let layout_text = plain_convert(&layout);
    assert!(layout_text.starts_with("布局\n"), "{layout_text}");
    for anchor in [
        "区域",
        "LBA 范围",
        "大小",
        "说明",
        "Share",
        "Encrypt",
        "明文数据区，系统直接挂载读写",
        "原样保留不动",
    ] {
        assert!(layout_text.contains(anchor), "缺 {anchor}: {layout_text}");
    }
    assert!(layout_text.ends_with('\n'));

    for clears in [false, true] {
        let plan = ConvertReport::SectorPlan {
            share: 1_000_008,
            clears_lba9: clears,
        };
        let plan_text = plain_convert(&plan);
        assert!(plan_text.starts_with("\n将写入 5 个扇区:\n"), "{plan_text}");
        assert!(
            plan_text.contains(if clears {
                "清零(当前存在)"
            } else {
                "已是零，不写"
            }),
            "{plan_text}"
        );
        assert!(
            plan_text.contains("不动   LBA4/8/11(盘身份) · 其余保留扇区 · 表尾状态 · LBA12 0x170..0x1FF 明文 · 盘尾区域"),
            "{plan_text}"
        );
        assert!(plan_text.ends_with('\n'));
    }
}

#[test]
fn styled_events_keep_ansi_wrap() {
    let header = styled(&WriteEvent::ApplyDeviceHeader {
        disk: 6,
        size_text: "29.8 GB".into(),
        vid: "0dd8".into(),
        pid: "170c".into(),
    });
    assert!(header.contains("\x1b[1m盘\x1b[0m"), "{header}");
    let created = styled(&WriteEvent::BackupCreated {
        path: "/b/disk6.bin".into(),
    });
    assert!(created.contains("\x1b[32m备份\x1b[0m"), "{created}");
    let preview = styled(&WriteEvent::DryRunPreview {
        disk: 6,
        needs_force: false,
    });
    assert!(preview.contains("\x1b[2m操作"), "{preview}");
}

// ══════════════════════════════════════════════════════════════════
// 事件序列行为测试(进程内，镜像 cli_offline 装配)
// ══════════════════════════════════════════════════════════════════
#[derive(Default)]
struct EventRecorderPrompter {
    events: Vec<WriteEvent>,
}

impl edpcli::cli::Prompter for EventRecorderPrompter {
    fn prompt_line(&mut self, _msg: &str) -> String {
        String::new()
    }

    fn confirm_yes(&mut self, _msg: &str) -> bool {
        true
    }

    fn write_event(&mut self, event: WriteEvent) {
        self.events.push(event);
    }

    // 写流程必须全部经 write_event 上报；绕过即视为违规。
    fn output(&mut self, _msg: &str) {
        panic!("写流程不得经 output 直出文本");
    }
}

fn tag(event: &WriteEvent) -> &'static str {
    match event {
        WriteEvent::ApplyDeviceHeader { .. } => "apply-device-header",
        WriteEvent::ExistingBackupsHeader { .. } => "existing-backups-header",
        WriteEvent::ExistingBackupsMenu { .. } => "existing-backups-menu",
        WriteEvent::NoExistingBackups => "no-existing-backups",
        WriteEvent::AlreadyNopwdHint => "already-nopwd-hint",
        WriteEvent::DryRunPreview { .. } => "dry-run-preview",
        WriteEvent::ForceRewriteNotice => "force-rewrite-notice",
        WriteEvent::BackupCreated { .. } => "backup-created",
        WriteEvent::BackupCreatedIsNopwd => "backup-created-is-nopwd",
        WriteEvent::RestoreCommandHint { .. } => "restore-command-hint",
        WriteEvent::ApplyWriteCompleted => "apply-write-completed",
        WriteEvent::RestoreMatchesHeader { .. } => "restore-matches-header",
        WriteEvent::RestoreMatchRow { .. } => "restore-match-row",
        WriteEvent::RestoreSelectionRetry { .. } => "restore-selection-retry",
        WriteEvent::BackupShaVerified { .. } => "backup-sha-verified",
        WriteEvent::RestoreSnapshotNopwdWarning => "restore-snapshot-nopwd-warning",
        WriteEvent::RestoreDryRunNotice { .. } => "restore-dry-run-notice",
        WriteEvent::RestoreTargetHeader { .. } => "restore-target-header",
        WriteEvent::RestoreWriteCompleted => "restore-write-completed",
        WriteEvent::Convert(ConvertReport::Identity { .. }) => "convert-identity",
        WriteEvent::Convert(ConvertReport::Layout { .. }) => "convert-layout",
        WriteEvent::Convert(ConvertReport::SectorPlan { .. }) => "convert-sector-plan",
    }
}

struct FixedClock;

impl edpcli::diskio::Clock for FixedClock {
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

#[test]
#[cfg(target_os = "macos")]
fn apply_dry_run_emits_typed_event_sequence() {
    use common::*;
    use edpcli::cli::{apply_flow, ApplyMode, Ctx};
    use edpcli::common::EXIT_OK;
    use edpcli::diskio::FileDev;

    let Some((conv, _did)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    let tmp = TmpDir::new("wp_dryrun");
    let bak = tmp.0.join("bak");
    std::fs::create_dir_all(&bak).unwrap();
    let img_path = tmp.0.join("disk.img");
    std::fs::write(&img_path, &conv).unwrap();
    let mut prompt = EventRecorderPrompter::default();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let mut ctx = Ctx {
        runner: &runner,
        clock: &FixedClock,
        prompt: &mut prompt,
        backup_dir: bak,
    };
    let code = apply_flow(ApplyMode::DryRun, 6, None, &mut ctx, &mut dev).unwrap();
    assert_eq!(code, EXIT_OK);
    let tags: Vec<&str> = prompt.events.iter().map(tag).collect();
    assert_eq!(
        tags,
        vec![
            "apply-device-header",
            "convert-identity",
            "convert-layout",
            "convert-sector-plan",
            "no-existing-backups",
            "already-nopwd-hint",
            "dry-run-preview",
        ],
        "{:?}",
        prompt.events
    );
    assert!(matches!(
        &prompt.events[6],
        WriteEvent::DryRunPreview {
            needs_force: true,
            ..
        }
    ));
}

#[test]
#[cfg(target_os = "macos")]
fn backup_create_and_restore_dry_run_event_sequence() {
    use common::*;
    use edpcli::cli::{backup_create_flow, restore_flow, Ctx};
    use edpcli::common::EXIT_OK;
    use edpcli::diskio::FileDev;

    let Some((conv, _did)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(6);
    let tmp = TmpDir::new("wp_restore");
    let bak = tmp.0.join("bak");
    std::fs::create_dir_all(&bak).unwrap();
    let img_path = tmp.0.join("disk.img");
    std::fs::write(&img_path, &conv).unwrap();
    let mut dev = FileDev::open_rdwr(
        img_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();

    let mut create_prompt = EventRecorderPrompter::default();
    let mut ctx = Ctx {
        runner: &runner,
        clock: &FixedClock,
        prompt: &mut create_prompt,
        backup_dir: bak.clone(),
    };
    let (created_path, is_nopwd) = backup_create_flow(6, &mut ctx, &mut dev).unwrap();
    assert!(is_nopwd);
    let tags: Vec<&str> = create_prompt.events.iter().map(tag).collect();
    assert_eq!(
        tags,
        vec!["backup-created", "backup-created-is-nopwd"],
        "{:?}",
        create_prompt.events
    );

    let mut restore_prompt = EventRecorderPrompter::default();
    let mut ctx = Ctx {
        runner: &runner,
        clock: &FixedClock,
        prompt: &mut restore_prompt,
        backup_dir: bak,
    };
    let code = restore_flow(
        Some(created_path.to_string_lossy().into_owned()),
        6,
        &mut ctx,
        &mut dev,
    )
    .unwrap();
    assert_eq!(code, EXIT_OK);
    let tags: Vec<&str> = restore_prompt.events.iter().map(tag).collect();
    assert_eq!(
        tags,
        vec![
            "backup-sha-verified",
            "restore-snapshot-nopwd-warning",
            "restore-dry-run-notice",
        ],
        "{:?}",
        restore_prompt.events
    );
}
