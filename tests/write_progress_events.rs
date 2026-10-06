//! 写盘服务类型化进度事件的契约测试:
//! - UI 层 `render_write_event` 在无色模式下逐字节锁死 CLI 文本；
//! - 带 ANSI 的样式渲染仍经 ui::wrap；
//! - backup-create/restore 实际发出的事件序列(录制型 Prompter)。
//!
//! 全部不碰真盘。

use edpcli::application::support::METADATA_IMAGE_LEN;
use edpcli::application::WriteEvent;
use edpcli::cli::terminal_ui as ui;
use edpcli::cli::terminal_ui::render_write_event;

fn plain(event: &WriteEvent) -> String {
    ui::set_enabled_for_tests(false);
    let text = render_write_event(event);
    ui::reset_enabled_for_tests();
    text
}

fn styled(event: &WriteEvent) -> String {
    ui::set_enabled_for_tests(true);
    let text = render_write_event(event);
    ui::reset_enabled_for_tests();
    text
}

// ══════════════════════════════════════════════════════════════════
// 渲染黄金基线(纯文本)
// ══════════════════════════════════════════════════════════════════
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
            file_name: "disk6_x.bin".into(),
        }),
        "  [1] 2026-09-17 00:00   disk6_x.bin\n"
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
        plain(&WriteEvent::RestoreDryRunNotice {
            path: "/b/x.bin".into(),
            disk: 6,
        }),
        format!(
            "[dry-run] 将还原 /b/x.bin → disk6 LBA0-12 ({}B) — 未写入。\n",
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
        "元数据恢复成功，读回校验通过。\n文件系统未恢复；部分分区可能需要格式化。\n"
    );
    assert_eq!(
        plain(&WriteEvent::PostRestoreAssessment {
            assessment: edpcli::application::post_restore::PostRestoreAssessment::default(),
        }),
        "恢复后状态：无法可靠判断\n"
    );
}

#[test]
fn styled_events_keep_ansi_wrap() {
    let created = styled(&WriteEvent::BackupCreated {
        path: "/b/disk6.bin".into(),
    });
    assert!(created.contains("\x1b[32m备份\x1b[0m"), "{created}");
}

// ══════════════════════════════════════════════════════════════════
// 事件序列行为测试(进程内，镜像 CLI 写盘安全装配)
// ══════════════════════════════════════════════════════════════════
// 辅助项只被下方 macOS 门控的测试使用；非 macOS 目标上必须同样门控，
// 否则 dead_code 会让 clippy -D warnings 失败。
#[cfg(target_os = "macos")]
#[derive(Default)]
struct EventRecorderPrompter {
    events: Vec<WriteEvent>,
}

#[cfg(target_os = "macos")]
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
}

#[cfg(target_os = "macos")]
fn tag(event: &WriteEvent) -> &'static str {
    match event {
        WriteEvent::BackupCreated { .. } => "backup-created",
        WriteEvent::RestoreMatchesHeader { .. } => "restore-matches-header",
        WriteEvent::RestoreMatchRow { .. } => "restore-match-row",
        WriteEvent::RestoreSelectionRetry { .. } => "restore-selection-retry",
        WriteEvent::BackupShaVerified { .. } => "backup-sha-verified",
        WriteEvent::RestoreDryRunNotice { .. } => "restore-dry-run-notice",
        WriteEvent::RestoreTargetHeader { .. } => "restore-target-header",
        WriteEvent::RestoreWriteCompleted => "restore-write-completed",
        WriteEvent::PostRestoreAssessment { .. } => "post-restore-assessment",
    }
}

#[cfg(target_os = "macos")]
struct FixedClock;

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
struct SerialRunner {
    inner: crate::common::FakeRunner,
}

#[cfg(target_os = "macos")]
impl edpcli::platform::system::CmdRunner for SerialRunner {
    fn check_output(&self, cmd: &[&str], timeout: std::time::Duration) -> std::io::Result<String> {
        self.inner.check_output(cmd, timeout)
    }

    fn hardware_serial(&self, _disk: u32) -> Option<String> {
        Some("NETAC-EVENT-TEST-001".into())
    }
}

#[test]
#[cfg(target_os = "macos")]
fn backup_create_and_restore_event_sequence() {
    use crate::common::*;
    use edpcli::application::write::{backup_create_flow, restore_flow_typed, Ctx};
    use edpcli::diskio::FileDev;

    let Some((conv, _did)) = mode1_fixture_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = SerialRunner {
        inner: netac_runner(6),
    };
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
    let created = backup_create_flow(6, &mut ctx, &mut dev).unwrap();
    assert!(created.edp_protocol_saved);
    assert!(created.partition_count > 0);
    let tags: Vec<&str> = create_prompt.events.iter().map(tag).collect();
    assert_eq!(tags, vec!["backup-created"], "{:?}", create_prompt.events);

    let mut restore_prompt = EventRecorderPrompter::default();
    let mut ctx = Ctx {
        runner: &runner,
        clock: &FixedClock,
        prompt: &mut restore_prompt,
        backup_dir: bak,
    };
    let report = restore_flow_typed(
        Some(created.path.to_string_lossy().into_owned()),
        6,
        &mut ctx,
        &mut dev,
    )
    .unwrap();
    assert!(report.report.metadata_restored);
    assert!(report.report.readback_verified);
    assert_eq!(
        report.report.restored_artifact_ids,
        vec!["raw.protocol.lba0_12"]
    );
    let tags: Vec<&str> = restore_prompt.events.iter().map(tag).collect();
    assert_eq!(
        tags,
        vec![
            "backup-sha-verified",
            "restore-target-header",
            "restore-write-completed",
            "post-restore-assessment",
        ],
        "{:?}",
        restore_prompt.events
    );
}

#[test]
#[cfg(target_os = "macos")]
fn structured_restore_checks_authorization_and_digest_and_isolates_observer_failure() {
    use crate::common::*;
    use edpcli::application::media_identity::{MediaIdentityPin, MediaIdentityResumePin};
    use edpcli::application::media_identity_observer::observe_media_identity_readonly;
    use edpcli::application::write::{
        backup_create_flow, restore_metadata_on_device, Ctx, RestoreMetadataRequest,
    };
    use edpcli::diskio::FileDev;
    let (image, _) = mode1_fixture_image("netac").expect("tracked fixture");
    let runner = SerialRunner {
        inner: netac_runner(6),
    };
    let tmp = TmpDir::new("structured_restore");
    let image_path = tmp.0.join("disk.img");
    std::fs::write(&image_path, &image).unwrap();
    let mut dev = FileDev::open_rdwr(
        image_path.to_str().unwrap(),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let mut prompt = EventRecorderPrompter::default();
    let backup = backup_create_flow(
        6,
        &mut Ctx {
            runner: &runner,
            clock: &FixedClock,
            prompt: &mut prompt,
            backup_dir: tmp.0.clone(),
        },
        &mut dev,
    )
    .unwrap();
    let observed = observe_media_identity_readonly(&runner, 6, &mut dev).unwrap();
    let mut request = RestoreMetadataRequest {
        disk: 6,
        backup_path: backup.path.clone(),
        confirmed: false,
        expected_target: MediaIdentityResumePin::from_pin(&MediaIdentityPin::new(
            observed.snapshot,
            &observed.protocol_image,
        )),
        expected_backup_sha256: edpcli::edpb::VerifiedBackupReader::open(&backup.path)
            .unwrap()
            .verified()
            .file_sha256
            .clone(),
    };
    let before = std::fs::read(&image_path).unwrap();
    assert_eq!(
        restore_metadata_on_device(&runner, &request, &mut dev, &mut |_| panic!(
            "no event before confirmation"
        ))
        .unwrap_err()
        .code,
        130
    );
    assert_eq!(std::fs::read(&image_path).unwrap(), before);
    request.confirmed = true;
    let expected_digest = request.expected_backup_sha256.clone();
    request.expected_backup_sha256 = "changed-after-confirmation".into();
    assert_eq!(
        restore_metadata_on_device(&runner, &request, &mut dev, &mut |_| {})
            .unwrap_err()
            .code,
        5
    );
    assert_eq!(std::fs::read(&image_path).unwrap(), before);
    request.expected_backup_sha256 = expected_digest;
    let outcome = restore_metadata_on_device(&runner, &request, &mut dev, &mut |_| {
        panic!("observer failed")
    })
    .unwrap();
    assert!(outcome.report.metadata_restored && outcome.report.readback_verified);
}
