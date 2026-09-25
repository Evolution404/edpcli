//! Interactive ratatui/crossterm frontend.
//!
//! Business work is delegated to `crate::application`; this module owns only terminal lifecycle,
//! event dispatch and rendering.

pub mod animation;
pub mod command;
pub mod event;
pub mod keymap;
pub mod render;
pub mod state;
pub mod task;
pub mod theme;

use std::io::{self, IsTerminal, Stdout};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event as ct_event, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::common::{EXIT_IO, EXIT_OK, EXIT_USAGE};
use event::KeyMapper;
use state::{AppState, NavCommand, StateEffect};
use task::TaskHub;

const RESUME_KIND_FLAG: &str = "--_resume-kind";
const RESUME_DISK_FLAG: &str = "--_resume-disk";
const RESUME_BACKUP_FLAG: &str = "--_resume-backup";
const RESUME_ONLYID_FLAG: &str = "--_resume-onlyid";
const RESUME_DEVICE_ID_FLAG: &str = "--_resume-device-id";

/// Serialize a confirmed write intent for an elevated TUI restart.
///
/// The disk is converted to the platform-native selector before crossing the privilege boundary;
/// restore additionally pins the exact backup path. The resumed TUI requires a second explicit YES.
pub fn resume_argv(intent: &state::WriteIntent) -> Vec<String> {
    let mut argv = vec![
        "tui".to_string(),
        RESUME_KIND_FLAG.to_string(),
        match intent.kind {
            state::WriteKind::Restore => "restore".to_string(),
            state::WriteKind::BackupCreate => "backup-create".to_string(),
            state::WriteKind::BackupCreateDeep => "backup-create-deep".to_string(),
        },
        RESUME_DISK_FLAG.to_string(),
        crate::application::pin_disk_selector(intent.disk),
    ];
    if let Some(path) = &intent.backup {
        argv.push(RESUME_BACKUP_FLAG.to_string());
        argv.push(path.to_string_lossy().into_owned());
    }
    if let Some(identity) = &intent.expected_identity {
        if let Some(onlyid) = &identity.onlyid {
            argv.push(RESUME_ONLYID_FLAG.to_string());
            argv.push(onlyid.clone());
        }
        if let Some(device_id) = &identity.device_id {
            argv.push(RESUME_DEVICE_ID_FLAG.to_string());
            argv.push(device_id.clone());
        }
    }
    argv
}

/// Parse the private state used only for an elevated TUI restart.
///
/// Any partial, duplicated or contradictory state fails closed. Public TUI invocations with no
/// private flags return `Ok(None)`.
pub fn parse_resume_args(argv: &[String]) -> Result<Option<state::WriteIntent>, String> {
    let mut kind = None;
    let mut disk = None;
    let mut backup = None;
    let mut onlyid = None;
    let mut device_id = None;
    let mut saw_resume = false;

    let mut i = usize::from(argv.first().is_some_and(|arg| arg == "tui"));
    while i < argv.len() {
        let arg = &argv[i];
        if arg == crate::elevate::ELEVATED_FLAG {
            i += 1;
            continue;
        }
        let mut take = |flag: &str| -> Result<String, String> {
            i += 1;
            if i >= argv.len() {
                return Err(format!("错误: {flag} 缺少参数值"));
            }
            Ok(argv[i].clone())
        };
        match arg.as_str() {
            RESUME_KIND_FLAG => {
                if kind.is_some() {
                    return Err(format!("错误: {RESUME_KIND_FLAG} 重复指定"));
                }
                saw_resume = true;
                let value = take(RESUME_KIND_FLAG)?;
                kind = Some(match value.as_str() {
                    "restore" => state::WriteKind::Restore,
                    "backup-create" => state::WriteKind::BackupCreate,
                    "backup-create-deep" => state::WriteKind::BackupCreateDeep,
                    _ => return Err(format!("错误: 非法 TUI resume kind: {value}")),
                });
            }
            RESUME_DISK_FLAG => {
                if disk.is_some() {
                    return Err(format!("错误: {RESUME_DISK_FLAG} 重复指定"));
                }
                saw_resume = true;
                let value = take(RESUME_DISK_FLAG)?;
                disk = Some(crate::application::parse_pinned_disk_selector(&value)?);
            }
            RESUME_BACKUP_FLAG => {
                if backup.is_some() {
                    return Err(format!("错误: {RESUME_BACKUP_FLAG} 重复指定"));
                }
                saw_resume = true;
                backup = Some(std::path::PathBuf::from(take(RESUME_BACKUP_FLAG)?));
            }
            RESUME_ONLYID_FLAG => {
                if onlyid.is_some() {
                    return Err(format!("错误: {RESUME_ONLYID_FLAG} 重复指定"));
                }
                saw_resume = true;
                onlyid = Some(take(RESUME_ONLYID_FLAG)?);
            }
            RESUME_DEVICE_ID_FLAG => {
                if device_id.is_some() {
                    return Err(format!("错误: {RESUME_DEVICE_ID_FLAG} 重复指定"));
                }
                saw_resume = true;
                device_id = Some(take(RESUME_DEVICE_ID_FLAG)?);
            }
            other => return Err(format!("错误: tui 不认识内部 resume 参数 {other}")),
        }
        i += 1;
    }

    if !saw_resume {
        return Ok(None);
    }
    let kind = kind.ok_or_else(|| format!("错误: 缺少 {RESUME_KIND_FLAG}"))?;
    let disk = disk.ok_or_else(|| format!("错误: 缺少 {RESUME_DISK_FLAG}"))?;
    match kind {
        state::WriteKind::BackupCreate | state::WriteKind::BackupCreateDeep if backup.is_some() => {
            Err("错误: 非 Restore resume 不允许携带备份路径".into())
        }
        state::WriteKind::Restore if backup.is_none() => {
            Err(format!("错误: restore resume 缺少 {RESUME_BACKUP_FLAG}"))
        }
        _ => Ok(Some(state::WriteIntent {
            kind,
            disk,
            backup,
            expected_identity: (onlyid.is_some() || device_id.is_some())
                .then_some(state::ExpectedIdentity { onlyid, device_id }),
        })),
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(mut terminal) => {
                if let Err(error) = terminal.clear() {
                    let _ = disable_raw_mode();
                    let _ = execute!(terminal.backend_mut(), Show, LeaveAlternateScreen);
                    return Err(error);
                }
                Ok(Self { terminal })
            }
            Err(error) => {
                let _ = disable_raw_mode();
                let mut stdout = io::stdout();
                let _ = execute!(stdout, Show, LeaveAlternateScreen);
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn startup_elevation_argv(argv: &[String], elevated: bool) -> Option<Vec<String>> {
    if elevated {
        return None;
    }
    if argv.first().is_some_and(|arg| arg == "tui") {
        Some(argv.to_vec())
    } else {
        // bare `edpcli` 由 cli 层路由到 TUI；跨越 sudo/UAC 边界时显式补上
        // `tui`，避免 elevated child 因只剩内部哨兵而回落到普通 CLI 解析。
        Some(vec!["tui".to_string()])
    }
}

fn ensure_elevated_before_tui(argv: &[String]) {
    if let Some(elevation_argv) = startup_elevation_argv(argv, crate::elevate::is_root()) {
        crate::elevate::ensure_elevated(&elevation_argv);
        unreachable!();
    }
}

enum LoopExit {
    Done,
    Elevate(state::WriteIntent),
}

fn dispatch_nav_command(
    state: &mut AppState,
    tasks: &mut TaskHub,
    command: NavCommand,
    backup_dir: &std::path::Path,
    viewport_height: usize,
) -> StateEffect {
    if let Some(effect) = state.guard_critical_command(command) {
        return effect;
    }
    match command {
        NavCommand::Refresh => {
            match state.workspace() {
                state::Workspace::Devices => {
                    tasks.request_device_scan(backup_dir.to_path_buf());
                    state.set_device_scan_pending(true);
                }
                state::Workspace::Backups => {
                    tasks.request_backup_scan(backup_dir.to_path_buf());
                    state.set_backup_scan_pending(true);
                }
                state::Workspace::Provision => {
                    tasks.request_device_scan(backup_dir.to_path_buf());
                    state.set_device_scan_pending(true);
                }
            }
            StateEffect::None
        }
        NavCommand::OpenInspect => {
            let source = match state.workspace() {
                state::Workspace::Devices | state::Workspace::Provision => state
                    .selected_device_disk()
                    .map(state::AdvancedInspectSource::Disk),
                state::Workspace::Backups => state
                    .selected_backup_path()
                    .map(state::AdvancedInspectSource::Backup),
            };
            if let Some(source) = source {
                if state.begin_advanced_inspect(source) {
                    match state.advanced_inspect_request() {
                        Ok((source, request)) => {
                            if let Err(message) = tasks.request_advanced_inspect(source, request) {
                                state.advanced_inspect_finish(Err(message.to_string()));
                            }
                        }
                        Err(message) => state.advanced_inspect_finish(Err(message)),
                    }
                }
            } else {
                state.set_notice("全盘检查需要先选定物理盘或 EDPB 备份。");
            }
            StateEffect::None
        }
        NavCommand::BeginRestore => {
            if let (Some(row), Some(backup)) =
                (state.selected_device(), state.selected_backup_path())
            {
                let disk = row.disk;
                let identity = state::ExpectedIdentity {
                    onlyid: row.onlyid.clone(),
                    device_id: row.device_id.clone(),
                };
                state.begin_write_wizard_for_identity(
                    state::WriteKind::Restore,
                    disk,
                    Some(backup),
                    Some(identity),
                );
            } else {
                state.set_notice("恢复需要先在设备页选定目标 U 盘，再进入备份页选择备份。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupCreate => {
            if let Some(row) = state.selected_device() {
                let disk = row.disk;
                let identity = state::ExpectedIdentity {
                    onlyid: row.onlyid.clone(),
                    device_id: row.device_id.clone(),
                };
                state.begin_write_wizard_for_identity(
                    state::WriteKind::BackupCreate,
                    disk,
                    None,
                    Some(identity),
                );
            } else {
                state.set_notice("创建备份需要先在设备页选定 U 盘。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupCreateDeep => {
            if let Some(row) = state.selected_device() {
                let disk = row.disk;
                let identity = state::ExpectedIdentity {
                    onlyid: row.onlyid.clone(),
                    device_id: row.device_id.clone(),
                };
                state.begin_write_wizard_for_identity(
                    state::WriteKind::BackupCreateDeep,
                    disk,
                    None,
                    Some(identity),
                );
            } else {
                state.set_notice("深度备份需要先在设备页选定 U 盘。");
            }
            StateEffect::None
        }
        NavCommand::VerifyBackup => {
            if let Some(path) = state.selected_backup_path() {
                state.set_notice("正在后台校验当前备份…");
                tasks.request_backup_verify(path, backup_dir.to_path_buf());
            } else {
                state.set_notice("当前没有可校验的备份。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupDelete => {
            if let Some((path, expected_sha256)) = state.selected_backup_delete_target() {
                state.begin_backup_delete(path, expected_sha256);
            } else {
                state.set_notice("当前备份缺少可固定的内容摘要，拒绝删除。");
            }
            StateEffect::None
        }
        NavCommand::ToggleBackupSelection => {
            if state.workspace() == state::Workspace::Backups {
                state.toggle_selected_backup();
            } else {
                state.set_notice("批量选择只在备份页可用。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupBatchDelete => {
            if state.workspace() != state::Workspace::Backups {
                let _ = state.navigate(NavCommand::WorkspaceBackups, viewport_height);
            }
            if let Some(targets) = state.begin_backup_batch_delete() {
                if let Err(message) =
                    tasks.request_backup_batch_delete_plan(targets, backup_dir.to_path_buf())
                {
                    state.backup_batch_delete_finish_plan(Err(message.to_string()));
                }
            }
            StateEffect::None
        }
        NavCommand::BeginBackupPrune => {
            if state.workspace() != state::Workspace::Backups {
                let _ = state.navigate(NavCommand::WorkspaceBackups, viewport_height);
            }
            if !state.begin_backup_prune() {
                state.set_notice("已有关键操作或清理向导正在执行。");
            }
            StateEffect::None
        }
        _ => state.navigate(command, viewport_height),
    }
}

fn palette_action_to_nav(action: command::PaletteAction) -> NavCommand {
    match action {
        command::PaletteAction::Devices => NavCommand::WorkspaceDevices,
        command::PaletteAction::Backups => NavCommand::WorkspaceBackups,
        command::PaletteAction::Provision => NavCommand::WorkspaceProvision,
        command::PaletteAction::Inspect => NavCommand::OpenInspect,
        command::PaletteAction::Restore => NavCommand::BeginRestore,
        command::PaletteAction::BackupCreate => NavCommand::BeginBackupCreate,
        command::PaletteAction::BackupCreateDeep => NavCommand::BeginBackupCreateDeep,
        command::PaletteAction::BackupVerify => NavCommand::VerifyBackup,
        command::PaletteAction::BackupDelete => NavCommand::BeginBackupDelete,
        command::PaletteAction::BackupBatchDelete => NavCommand::BeginBackupBatchDelete,
        command::PaletteAction::BackupPrune => NavCommand::BeginBackupPrune,
        command::PaletteAction::Refresh => NavCommand::Refresh,
        command::PaletteAction::Help => NavCommand::Help,
        command::PaletteAction::Quit => NavCommand::Quit,
    }
}

fn keymap_action_to_nav(action: keymap::TuiAction) -> Option<NavCommand> {
    use keymap::TuiAction;

    Some(match action {
        TuiAction::MoveUp => NavCommand::Up,
        TuiAction::MoveDown => NavCommand::Down,
        TuiAction::Top => NavCommand::Top,
        TuiAction::Bottom => NavCommand::Bottom,
        TuiAction::HalfPageUp => NavCommand::HalfPageUp,
        TuiAction::HalfPageDown => NavCommand::HalfPageDown,
        TuiAction::Back => NavCommand::Escape,
        TuiAction::Quit => NavCommand::Quit,
        TuiAction::Help => NavCommand::Help,
        TuiAction::Search => NavCommand::Search,
        TuiAction::NextMatch => NavCommand::NextMatch,
        TuiAction::PreviousMatch => NavCommand::PreviousMatch,
        TuiAction::Command => NavCommand::CommandPalette,
        TuiAction::Refresh => NavCommand::Refresh,
        TuiAction::WorkspaceNext => NavCommand::NextWorkspace,
        TuiAction::WorkspacePrevious => NavCommand::PreviousWorkspace,
        _ => return None,
    })
}

fn dispatch_tui_action(
    state: &mut AppState,
    tasks: &mut TaskHub,
    action: keymap::TuiAction,
    backup_dir: &std::path::Path,
    viewport_height: usize,
) -> StateEffect {
    use keymap::TuiAction;

    if let Some(command) = keymap_action_to_nav(action) {
        return dispatch_nav_command(state, tasks, command, backup_dir, viewport_height);
    }

    match action {
        TuiAction::Plan if state.workspace() == state::Workspace::Devices => {
            if let Err(message) = state.begin_provision_for_selected_device() {
                state.set_notice(message);
            }
            StateEffect::None
        }
        TuiAction::Insert
            if matches!(
                state.workspace(),
                state::Workspace::Devices | state::Workspace::Backups
            ) =>
        {
            dispatch_nav_command(
                state,
                tasks,
                NavCommand::OpenInspect,
                backup_dir,
                viewport_height,
            )
        }
        TuiAction::Activate | TuiAction::Open => match state.workspace() {
            state::Workspace::Devices => {
                if let Err(message) = state.begin_provision_for_selected_device() {
                    state.set_notice(message);
                }
                StateEffect::None
            }
            state::Workspace::Backups => dispatch_nav_command(
                state,
                tasks,
                NavCommand::OpenInspect,
                backup_dir,
                viewport_height,
            ),
            state::Workspace::Provision => StateEffect::None,
        },
        TuiAction::Toggle if state.workspace() == state::Workspace::Backups => {
            dispatch_nav_command(
                state,
                tasks,
                NavCommand::ToggleBackupSelection,
                backup_dir,
                viewport_height,
            )
        }
        TuiAction::ViewOrVerify if state.workspace() == state::Workspace::Backups => {
            dispatch_nav_command(
                state,
                tasks,
                NavCommand::VerifyBackup,
                backup_dir,
                viewport_height,
            )
        }
        TuiAction::Delete if state.workspace() == state::Workspace::Backups => {
            let command = if state.backup_selection_count() > 0 {
                NavCommand::BeginBackupBatchDelete
            } else {
                NavCommand::BeginBackupDelete
            };
            dispatch_nav_command(state, tasks, command, backup_dir, viewport_height)
        }
        TuiAction::Add
            if matches!(
                state.workspace(),
                state::Workspace::Devices | state::Workspace::Backups
            ) =>
        {
            state.begin_backup_create_choice();
            StateEffect::None
        }
        TuiAction::Restore if state.workspace() == state::Workspace::Backups => {
            dispatch_nav_command(
                state,
                tasks,
                NavCommand::BeginRestore,
                backup_dir,
                viewport_height,
            )
        }
        _ => StateEffect::None,
    }
}

fn open_advanced_inspect_selection(state: &mut AppState, tasks: &mut TaskHub) {
    let request = if state.advanced_inspect_selected_sector_lba().is_some() {
        state.advanced_inspect_open_selected_sector()
    } else if state.advanced_inspect_selected_field().is_some() {
        state.advanced_inspect_open_selected_field()
    } else {
        state.advanced_inspect_enter_selected();
        None
    };

    if let Some((source, lba)) = request {
        if let Err(message) = tasks.request_advanced_inspect_sector(source, lba) {
            state.advanced_inspect_sector_finish(lba, Err(message.to_string()));
        }
    }
}

fn start_provision_plan(state: &mut AppState, tasks: &mut TaskHub) {
    let Some(disk) = state.selected_device_disk() else {
        state.provision_mut().message = Some("目标 USB 已不存在，请返回设备页重新选择。".into());
        return;
    };
    let request = if state.provision().kind == state::ProvisionKind::Plain {
        match state.provision_plain_plan() {
            Ok(plan) => crate::application::provision::ProvisionRequest::Plain(
                crate::application::provision::PlainProvisionRequest::from_plan(&plan),
            ),
            Err(message) => {
                state.provision_mut().message = Some(message);
                return;
            }
        }
    } else {
        match state.provision_request() {
            Ok(request) => {
                crate::application::provision::ProvisionRequest::Official(Box::new(request))
            }
            Err(message) => {
                state.provision_mut().message = Some(message);
                return;
            }
        }
    };
    state.provision_set_planning();
    if let Err(message) = tasks.request_provision_plan(disk, request) {
        state.provision_finish_plan(Err(message.to_string()));
    }
}

fn run_loop(resume: Option<state::WriteIntent>) -> io::Result<LoopExit> {
    let mut session = TerminalSession::enter()?;
    let mut state = AppState::new();
    let mut last_animation_tick = Instant::now();
    let motion_mode = animation::MotionMode::from_env();
    let mut redraw_requested = true;
    let mut notice_was_visible = false;
    let mut last_render_at: Option<Instant> = None;
    if let Some(intent) = resume {
        state.begin_write_wizard(intent.kind, intent.disk, intent.backup);
    }
    let mut keys = KeyMapper::new();
    let mut tasks = TaskHub::new();
    let backup_dir = crate::diskio::resolve_backup_dir(None);
    tasks.request_device_scan(backup_dir.clone());
    tasks.request_backup_scan(backup_dir.clone());
    state.set_device_scan_pending(true);
    state.set_backup_scan_pending(true);

    let result = (|| -> io::Result<LoopExit> {
        loop {
            let now = Instant::now();
            if let Some(interval) = motion_mode.tick_interval() {
                if now.duration_since(last_animation_tick) >= interval {
                    state.advance_animation();
                    last_animation_tick = now;
                    redraw_requested = true;
                }
            }

            let updates = tasks.poll();
            redraw_requested |= updates.has_updates();
            if let Some(rows) = updates.devices {
                state.replace_devices(rows);
            }
            if let Some(rows) = updates.backups {
                state.replace_backups(rows);
            }
            if let Some(message) = updates.device_error {
                state.set_device_scan_pending(false);
                state.set_notice(message);
            }
            if let Some(message) = updates.backup_error {
                state.set_backup_scan_pending(false);
                state.set_notice(message);
            }
            if let Some((_operation_id, event)) = updates.write_progress {
                state.set_write_progress(event);
            }
            if let Some((_operation_id, result)) = updates.write {
                let refresh_backups = result.is_ok()
                    && state.wizard().is_some_and(|wizard| {
                        matches!(
                            wizard.kind,
                            state::WriteKind::BackupCreate | state::WriteKind::BackupCreateDeep
                        )
                    });
                state.finish_write(result);
                if refresh_backups {
                    tasks.request_backup_scan(backup_dir.clone());
                    state.set_backup_scan_pending(true);
                }
            }
            if let Some((path, result)) = updates.backup_verify {
                match result {
                    Ok(()) => state.set_notice(format!(
                        "备份 {} 校验通过：大小与 SHA-256 正常。",
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("<无效文件名>")
                    )),
                    Err(message) => state.set_notice(message),
                }
            }
            if let Some((_operation_id, result)) = updates.backup_delete {
                let refresh_backups = result.is_ok();
                state.finish_backup_delete(result);
                if refresh_backups {
                    tasks.request_backup_scan(backup_dir.clone());
                    state.set_backup_scan_pending(true);
                }
            }
            if let Some(result) = updates.backup_batch_delete_plan {
                state.backup_batch_delete_finish_plan(result);
            }
            if let Some((_operation_id, result)) = updates.backup_batch_delete_execute {
                let refresh_backups = result.is_ok();
                state.backup_batch_delete_finish_execute(result);
                if refresh_backups {
                    tasks.request_backup_scan(backup_dir.clone());
                    state.set_backup_scan_pending(true);
                }
            }
            if let Some(result) = updates.backup_prune_plan {
                state.backup_prune_finish_plan(result);
            }
            if let Some((_operation_id, result)) = updates.backup_prune_execute {
                let refresh_backups = result.is_ok();
                state.backup_prune_finish_execute(result);
                if refresh_backups {
                    tasks.request_backup_scan(backup_dir.clone());
                    state.set_backup_scan_pending(true);
                }
            }
            if let Some((_operation_id, result)) = updates.provision_backup {
                let success = result.is_ok();
                state.provision_finish_backup_save(result);
                if success {
                    tasks.request_backup_scan(backup_dir.clone());
                    state.set_backup_scan_pending(true);
                }
            }
            if let Some(result) = updates.provision_plan {
                state.provision_finish_plan(result);
            }
            if let Some((_operation_id, message)) = updates.provision_progress {
                state.provision_mut().message = Some(message);
            }
            if let Some((_operation_id, result)) = updates.provision_write {
                let success = result.is_ok();
                state.provision_finish_write(result);
                if success {
                    tasks.request_device_scan(backup_dir.clone());
                    tasks.request_backup_scan(backup_dir.clone());
                    state.set_device_scan_pending(true);
                    state.set_backup_scan_pending(true);
                }
            }
            if let Some(result) = updates.provision_export {
                state.provision_finish_export(result);
            }
            if let Some(result) = updates.advanced_inspect {
                state.advanced_inspect_finish(result);
            }
            if let Some((lba, result)) = updates.advanced_inspect_sector {
                state.advanced_inspect_sector_finish(lba, result);
            }
            if state.take_deferred_exit() == StateEffect::ExitRequested {
                break;
            }

            let notice_visible = state.notice().is_some();
            if notice_visible != notice_was_visible {
                notice_was_visible = notice_visible;
                redraw_requested = true;
            }

            let now = Instant::now();
            const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(16);
            if redraw_requested
                && last_render_at.is_none_or(|last| now.duration_since(last) >= MIN_RENDER_INTERVAL)
            {
                session.terminal.draw(|frame| render::draw(frame, &state))?;
                redraw_requested = false;
                last_render_at = Some(now);
            }
            let poll_timeout = if redraw_requested {
                last_render_at
                    .map(|last| MIN_RENDER_INTERVAL.saturating_sub(now.duration_since(last)))
                    .unwrap_or_default()
                    .min(Duration::from_millis(animation::TICK_INTERVAL_MS))
            } else {
                Duration::from_millis(animation::TICK_INTERVAL_MS)
            };
            if !ct_event::poll(poll_timeout)? {
                continue;
            }

            redraw_requested = true;
            match ct_event::read()? {
                ct_event::Event::Key(key) => {
                    if !event::is_actionable_key(&key) {
                        continue;
                    }
                    if let Some(stage) = state.advanced_inspect().map(|advanced| advanced.stage) {
                        use state::AdvancedInspectStage;
                        match stage {
                            AdvancedInspectStage::Running => {
                                if key.code == ct_event::KeyCode::Esc {
                                    state.set_notice("全盘检查正在后台读取结构，请等待完成。");
                                }
                                continue;
                            }
                            AdvancedInspectStage::Browser => {
                                if let Some(prompt) = state.advanced_inspect_prompt() {
                                    let mode = if matches!(
                                        prompt,
                                        state::AdvancedInspectPrompt::Jump { .. }
                                    ) {
                                        state::InputMode::Command
                                    } else {
                                        state::InputMode::Search
                                    };
                                    if let Some(action) = keys.map(mode, key) {
                                        match action {
                                            keymap::TuiAction::Back => {
                                                state.advanced_inspect_cancel_prompt();
                                            }
                                            keymap::TuiAction::Backspace => {
                                                state.advanced_inspect_prompt_backspace();
                                            }
                                            keymap::TuiAction::Submit => {
                                                match state.advanced_inspect_submit_prompt() {
                                                    Ok(Some((source, lba))) => {
                                                        if let Err(message) = tasks
                                                            .request_advanced_inspect_sector(
                                                                source, lba,
                                                            )
                                                        {
                                                            state.advanced_inspect_sector_finish(
                                                                lba,
                                                                Err(message.to_string()),
                                                            );
                                                        }
                                                    }
                                                    Ok(None) => {}
                                                    Err(message) => state.set_notice(message),
                                                }
                                            }
                                            keymap::TuiAction::Text(' ')
                                                if mode == state::InputMode::Command =>
                                            {
                                                state.advanced_inspect_toggle_jump_unit();
                                            }
                                            keymap::TuiAction::Text(ch) => {
                                                state.advanced_inspect_prompt_push(ch);
                                            }
                                            _ => {}
                                        }
                                    }
                                    continue;
                                }

                                let sector_detail =
                                    state.advanced_inspect().is_some_and(|advanced| {
                                        advanced.panel == state::AdvancedInspectPanel::Detail
                                            && advanced.sector.is_some()
                                    });
                                if sector_detail {
                                    let Some(action) = keys.map(state::InputMode::Normal, key)
                                    else {
                                        continue;
                                    };
                                    use keymap::TuiAction;
                                    match action {
                                        TuiAction::MoveLeft => {
                                            state.advanced_inspect_sector_move_cursor(-1);
                                        }
                                        TuiAction::MoveRight => {
                                            state.advanced_inspect_sector_move_cursor(1);
                                        }
                                        TuiAction::MoveUp => {
                                            state.advanced_inspect_sector_move_cursor(-16);
                                        }
                                        TuiAction::MoveDown => {
                                            state.advanced_inspect_sector_move_cursor(16);
                                        }
                                        TuiAction::ViewOrVerify => {
                                            state.advanced_inspect_sector_cycle_mode();
                                        }
                                        TuiAction::Open | TuiAction::Toggle => {
                                            state.advanced_inspect_sector_toggle_field();
                                        }
                                        TuiAction::RowStart => {
                                            state.advanced_inspect_sector_row_start();
                                        }
                                        TuiAction::RowEnd => {
                                            state.advanced_inspect_sector_row_end();
                                        }
                                        TuiAction::Top => {
                                            state.advanced_inspect_sector_top();
                                        }
                                        TuiAction::Bottom => {
                                            state.advanced_inspect_sector_bottom();
                                        }
                                        TuiAction::HalfPageUp => {
                                            state.advanced_inspect_sector_half_page(true);
                                        }
                                        TuiAction::HalfPageDown => {
                                            state.advanced_inspect_sector_half_page(false);
                                        }
                                        TuiAction::Yank => {
                                            let _ = state.advanced_inspect_sector_yank(false);
                                        }
                                        TuiAction::YankRaw => {
                                            let _ = state.advanced_inspect_sector_yank(true);
                                        }
                                        TuiAction::PageUp | TuiAction::PageDown => {
                                            let delta =
                                                if action == TuiAction::PageUp { -1 } else { 1 };
                                            if let Some((source, lba)) =
                                                state.advanced_inspect_shift_sector(delta)
                                            {
                                                if let Err(message) = tasks
                                                    .request_advanced_inspect_sector(source, lba)
                                                {
                                                    state.advanced_inspect_sector_finish(
                                                        lba,
                                                        Err(message.to_string()),
                                                    );
                                                }
                                            }
                                        }
                                        TuiAction::Back => {
                                            state.advanced_inspect_close_sector();
                                        }
                                        TuiAction::InspectJump => {
                                            state.advanced_inspect_begin_jump();
                                        }
                                        TuiAction::Search => {
                                            state.advanced_inspect_begin_search();
                                        }
                                        TuiAction::NextMatch | TuiAction::PreviousMatch => {
                                            if let Err(message) = state
                                                .advanced_inspect_search_next(
                                                    action == TuiAction::PreviousMatch,
                                                )
                                            {
                                                state.set_notice(message);
                                            }
                                        }
                                        _ => {}
                                    }
                                    continue;
                                }

                                let Some(action) = keys.map(state::InputMode::Normal, key) else {
                                    continue;
                                };
                                use keymap::TuiAction;
                                match action {
                                    TuiAction::InspectJump => {
                                        state.advanced_inspect_begin_jump();
                                    }
                                    TuiAction::Search => {
                                        state.advanced_inspect_begin_search();
                                    }
                                    TuiAction::NextMatch | TuiAction::PreviousMatch => {
                                        if let Err(message) = state.advanced_inspect_search_next(
                                            action == TuiAction::PreviousMatch,
                                        ) {
                                            state.set_notice(message);
                                        }
                                    }
                                    TuiAction::MoveUp => state.advanced_inspect_move_tree(-1),
                                    TuiAction::MoveDown => state.advanced_inspect_move_tree(1),
                                    TuiAction::MoveLeft => {
                                        state.advanced_inspect_collapse_or_parent();
                                    }
                                    TuiAction::MoveRight => {
                                        state.advanced_inspect_expand_or_child();
                                    }
                                    TuiAction::Top => state.advanced_inspect_tree_top(),
                                    TuiAction::Bottom => state.advanced_inspect_tree_bottom(),
                                    TuiAction::Open => state.advanced_inspect_toggle_selected(),
                                    TuiAction::Activate => {
                                        open_advanced_inspect_selection(&mut state, &mut tasks);
                                    }
                                    TuiAction::PanelNext
                                    | TuiAction::PanelRight
                                    | TuiAction::PanelDown => {
                                        state.advanced_inspect_shift_panel(false);
                                    }
                                    TuiAction::PanelPrevious
                                    | TuiAction::PanelLeft
                                    | TuiAction::PanelUp => {
                                        state.advanced_inspect_shift_panel(true);
                                    }
                                    TuiAction::HalfPageUp => {
                                        if state.advanced_inspect().is_some_and(|advanced| {
                                            advanced.panel == state::AdvancedInspectPanel::Tree
                                        }) {
                                            state.advanced_inspect_move_tree(-10);
                                        } else {
                                            state.advanced_inspect_scroll_detail(-10);
                                        }
                                    }
                                    TuiAction::HalfPageDown => {
                                        if state.advanced_inspect().is_some_and(|advanced| {
                                            advanced.panel == state::AdvancedInspectPanel::Tree
                                        }) {
                                            state.advanced_inspect_move_tree(10);
                                        } else {
                                            state.advanced_inspect_scroll_detail(10);
                                        }
                                    }
                                    TuiAction::Back => {
                                        if !state.advanced_inspect_close_sector() {
                                            state.close_advanced_inspect();
                                        }
                                    }
                                    TuiAction::Help => {
                                        let _ = state.navigate(NavCommand::Help, 1);
                                    }
                                    _ => {}
                                }
                                continue;
                            }
                        }
                    }
                    if state.workspace() == state::Workspace::Provision {
                        use keymap::TuiAction;
                        use state::ProvisionStage;

                        let Some(action) = keys.map(state.input_mode(), key) else {
                            continue;
                        };
                        let viewport_height =
                            session.terminal.size()?.height.saturating_sub(9) as usize;

                        if matches!(
                            action,
                            TuiAction::WorkspaceNext
                                | TuiAction::WorkspacePrevious
                                | TuiAction::Refresh
                                | TuiAction::Help
                                | TuiAction::Command
                        ) {
                            match dispatch_tui_action(
                                &mut state,
                                &mut tasks,
                                action,
                                &backup_dir,
                                viewport_height,
                            ) {
                                StateEffect::ExitRequested => break,
                                StateEffect::ExitDeferred | StateEffect::None => {}
                            }
                            continue;
                        }

                        match state.provision().stage {
                            ProvisionStage::SelectDisk => match action {
                                TuiAction::MoveUp => {
                                    let _ = state.navigate(NavCommand::Up, viewport_height);
                                }
                                TuiAction::MoveDown => {
                                    let _ = state.navigate(NavCommand::Down, viewport_height);
                                }
                                TuiAction::Top => {
                                    let _ = state.navigate(NavCommand::Top, viewport_height);
                                }
                                TuiAction::Bottom => {
                                    let _ = state.navigate(NavCommand::Bottom, viewport_height);
                                }
                                TuiAction::Activate => {
                                    if state.provision_select_disk().is_none() {
                                        state.set_notice("请选择可读取的 USB 整盘目标。");
                                    }
                                }
                                TuiAction::Back => {
                                    let _ = state.navigate(NavCommand::Escape, viewport_height);
                                }
                                _ => {}
                            },
                            ProvisionStage::BackupPrompt => match action {
                                TuiAction::MoveUp => {
                                    let _ = state.navigate(NavCommand::Up, viewport_height);
                                }
                                TuiAction::MoveDown => {
                                    let _ = state.navigate(NavCommand::Down, viewport_height);
                                }
                                TuiAction::Activate => {
                                    if state.selected() == 0 {
                                        let Some((disk, onlyid, device_id)) =
                                            state.selected_device().map(|row| {
                                                (
                                                    row.disk,
                                                    row.onlyid.clone(),
                                                    row.device_id.clone(),
                                                )
                                            })
                                        else {
                                            state.set_notice(
                                                "目标 USB 已不存在，请返回设备页重新选择。",
                                            );
                                            continue;
                                        };
                                        let identity =
                                            state::ExpectedIdentity { onlyid, device_id };
                                        state.provision_begin_backup_save();
                                        if let Err(message) = tasks.request_provision_backup(
                                            disk,
                                            identity,
                                            backup_dir.clone(),
                                        ) {
                                            state.provision_finish_backup_save(Err(
                                                message.to_string()
                                            ));
                                        }
                                    } else {
                                        state.provision_skip_backup();
                                    }
                                }
                                TuiAction::Back => {
                                    let _ = state.navigate(NavCommand::Escape, viewport_height);
                                }
                                _ => {}
                            },
                            ProvisionStage::BackupSaving => {
                                if action == TuiAction::Back {
                                    state.set_notice("正在保存当前盘，请等待完成。");
                                }
                            }
                            ProvisionStage::Menu => match action {
                                TuiAction::MoveUp => {
                                    let _ = state.navigate(NavCommand::Up, viewport_height);
                                }
                                TuiAction::MoveDown => {
                                    let _ = state.navigate(NavCommand::Down, viewport_height);
                                }
                                TuiAction::Top => {
                                    let _ = state.navigate(NavCommand::Top, viewport_height);
                                }
                                TuiAction::Bottom => {
                                    let _ = state.navigate(NavCommand::Bottom, viewport_height);
                                }
                                TuiAction::Activate => {
                                    if state.selected_device_disk().is_none() {
                                        state.set_notice(
                                            "物理制盘需要先在制盘页明确选择 USB 目标。",
                                        );
                                    } else {
                                        state.provision_begin_selected();
                                    }
                                }
                                TuiAction::Back => {
                                    let _ = state.navigate(NavCommand::Escape, viewport_height);
                                }
                                _ => {}
                            },
                            ProvisionStage::Form
                                if state.input_mode() == state::InputMode::Insert =>
                            {
                                match action {
                                    TuiAction::Text(ch) => state.provision_push_char(ch),
                                    TuiAction::Backspace => state.provision_backspace(),
                                    TuiAction::DeleteChar => state.provision_delete_char(),
                                    TuiAction::CursorLeft => state.provision_move_cursor(-1),
                                    TuiAction::CursorRight => state.provision_move_cursor(1),
                                    TuiAction::CursorHome => state.provision_cursor_home(),
                                    TuiAction::CursorEnd => state.provision_cursor_end(),
                                    TuiAction::Submit | TuiAction::Back => {
                                        state.provision_end_insert()
                                    }
                                    _ => {}
                                }
                            }
                            ProvisionStage::Form => match action {
                                TuiAction::MoveUp => state.provision_move_field(-1),
                                TuiAction::MoveDown => state.provision_move_field(1),
                                TuiAction::Top => {
                                    let count = state.provision_field_count();
                                    state.provision_move_field(-(count as isize));
                                }
                                TuiAction::Bottom => {
                                    let count = state.provision_field_count();
                                    state.provision_move_field(count as isize);
                                }
                                TuiAction::HalfPageUp => state.provision_move_field(-5),
                                TuiAction::HalfPageDown => state.provision_move_field(5),
                                TuiAction::MoveLeft | TuiAction::MoveRight | TuiAction::Toggle => {
                                    state.provision_toggle_selected_option();
                                }
                                TuiAction::Insert => {
                                    state.provision_begin_insert();
                                }
                                TuiAction::Fill => {
                                    state.provision_fill_selected_capacity();
                                }
                                TuiAction::Add => {
                                    state.provision_plain_add_partition();
                                }
                                TuiAction::Delete => {
                                    state.provision_plain_delete_selected_partition();
                                }
                                TuiAction::Activate | TuiAction::Plan | TuiAction::Write => {
                                    start_provision_plan(&mut state, &mut tasks);
                                }
                                TuiAction::Export => {
                                    state.set_notice("请先按 p 生成只读计划，再从计划页导出镜像。");
                                }
                                TuiAction::Back => {
                                    let _ = state.navigate(NavCommand::Escape, viewport_height);
                                }
                                _ => {}
                            },
                            ProvisionStage::Planning => {
                                if action == TuiAction::Back {
                                    state.set_notice("制盘计划正在后台生成，请等待完成。");
                                }
                            }
                            ProvisionStage::Review => match action {
                                TuiAction::Activate | TuiAction::Write => {
                                    state.provision_begin_confirm();
                                }
                                TuiAction::Export => state.provision_begin_export(),
                                TuiAction::Back => {
                                    let _ = state.navigate(NavCommand::Escape, viewport_height);
                                }
                                _ => {}
                            },
                            ProvisionStage::ExportPath => match action {
                                TuiAction::Text(ch) => state.provision_export_push_char(ch),
                                TuiAction::Backspace => state.provision_export_backspace(),
                                TuiAction::Submit => {
                                    if let Some((prepared, path)) = state.provision_take_export() {
                                        if let Err(message) =
                                            tasks.request_provision_export(prepared, path)
                                        {
                                            state.provision_finish_export(Err(message.to_string()));
                                        }
                                    }
                                }
                                TuiAction::Back => state.provision_cancel_export(),
                                _ => {}
                            },
                            ProvisionStage::Exporting => {
                                if action == TuiAction::Back {
                                    state.set_notice("镜像正在后台导出，请等待完成。");
                                }
                            }
                            ProvisionStage::Confirm => match action {
                                TuiAction::Text(ch) => state.provision_push_confirmation(ch),
                                TuiAction::Backspace => state.provision_backspace_confirmation(),
                                TuiAction::Submit => {
                                    if let Some(prepared) = state.provision_take_for_write() {
                                        if let Err(message) = tasks
                                            .request_provision_write(prepared, backup_dir.clone())
                                        {
                                            state.provision_finish_write(Err(message.to_string()));
                                        }
                                    }
                                }
                                TuiAction::Cancel | TuiAction::Back => {
                                    let _ = state.navigate(NavCommand::Escape, viewport_height);
                                }
                                TuiAction::Confirm => {
                                    state.set_notice("破坏性写盘仍需精确输入大写 YES 后按 Enter。")
                                }
                                _ => {}
                            },
                            ProvisionStage::Running => {
                                match dispatch_tui_action(
                                    &mut state,
                                    &mut tasks,
                                    action,
                                    &backup_dir,
                                    viewport_height,
                                ) {
                                    StateEffect::ExitRequested => break,
                                    StateEffect::ExitDeferred | StateEffect::None => {}
                                }
                            }
                            ProvisionStage::Result => {
                                if matches!(action, TuiAction::Activate | TuiAction::Back) {
                                    state.provision_reset();
                                }
                            }
                        }
                        continue;
                    }
                    if state.backup_create_choice().is_some() {
                        if let Some(action) = keys.map(state::InputMode::Normal, key) {
                            match action {
                                keymap::TuiAction::MoveUp => {
                                    state.move_backup_create_choice(-1);
                                }
                                keymap::TuiAction::MoveDown => {
                                    state.move_backup_create_choice(1);
                                }
                                keymap::TuiAction::Activate => {
                                    if let Some(choice) = state.take_backup_create_choice() {
                                        let command = match choice {
                                            state::BackupCreateChoice::Metadata => {
                                                NavCommand::BeginBackupCreate
                                            }
                                            state::BackupCreateChoice::Deep => {
                                                NavCommand::BeginBackupCreateDeep
                                            }
                                        };
                                        let viewport_height =
                                            session.terminal.size()?.height.saturating_sub(9)
                                                as usize;
                                        let _ = dispatch_nav_command(
                                            &mut state,
                                            &mut tasks,
                                            command,
                                            &backup_dir,
                                            viewport_height,
                                        );
                                    }
                                }
                                keymap::TuiAction::Back => state.cancel_backup_create_choice(),
                                _ => {}
                            }
                        }
                        continue;
                    }

                    if let Some(stage) = state.backup_batch_delete().map(|batch| batch.stage) {
                        use state::BackupBatchDeleteStage;
                        match stage {
                            BackupBatchDeleteStage::Planning => {
                                if keys.map(state::InputMode::Normal, key)
                                    == Some(keymap::TuiAction::Back)
                                {
                                    state.set_notice("批量删除计划正在后台生成，请等待完成。");
                                }
                                continue;
                            }
                            BackupBatchDeleteStage::Review => {
                                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                                    match action {
                                        keymap::TuiAction::Activate => {
                                            state.backup_batch_delete_begin_confirm();
                                        }
                                        keymap::TuiAction::Back => {
                                            state.close_backup_batch_delete();
                                        }
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            BackupBatchDeleteStage::Confirm => {
                                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                                    match action {
                                        keymap::TuiAction::Text(ch) => {
                                            state.backup_batch_delete_push_confirmation(ch);
                                        }
                                        keymap::TuiAction::Backspace => {
                                            state.backup_batch_delete_backspace();
                                        }
                                        keymap::TuiAction::Submit => {
                                            if let Some(plan) =
                                                state.backup_batch_delete_take_for_execute()
                                            {
                                                if let Err(message) = tasks
                                                    .request_backup_batch_delete_execute(
                                                        plan,
                                                        backup_dir.clone(),
                                                    )
                                                {
                                                    state.backup_batch_delete_finish_execute(Err(
                                                        message.to_string(),
                                                    ));
                                                }
                                            }
                                        }
                                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                                            state.close_backup_batch_delete();
                                        }
                                        keymap::TuiAction::Confirm => state.set_notice(
                                            "批量删除仍需精确输入大写 YES 后按 Enter。",
                                        ),
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            BackupBatchDeleteStage::Running => {}
                            BackupBatchDeleteStage::Result => {
                                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                                    if matches!(
                                        action,
                                        keymap::TuiAction::Activate | keymap::TuiAction::Back
                                    ) {
                                        state.close_backup_batch_delete();
                                    }
                                }
                                continue;
                            }
                        }
                    }
                    if let Some(stage) = state.backup_prune().map(|prune| prune.stage) {
                        use state::BackupPruneStage;
                        match stage {
                            BackupPruneStage::Input => {
                                if let Some(action) = keys.map(state::InputMode::Insert, key) {
                                    match action {
                                        keymap::TuiAction::Text(ch) if ch.is_ascii_digit() => {
                                            state.backup_prune_push_digit(ch);
                                        }
                                        keymap::TuiAction::Backspace => {
                                            state.backup_prune_backspace()
                                        }
                                        keymap::TuiAction::Submit => {
                                            match state.backup_prune_start_plan() {
                                                Ok(keep) => {
                                                    if let Err(message) = tasks
                                                        .request_backup_prune_plan(
                                                            backup_dir.clone(),
                                                            keep,
                                                        )
                                                    {
                                                        state.backup_prune_finish_plan(Err(
                                                            message.to_string(),
                                                        ));
                                                    }
                                                }
                                                Err(message) => {
                                                    if let Some(prune) = state.backup_prune_mut() {
                                                        prune.message = Some(message);
                                                    }
                                                }
                                            }
                                        }
                                        keymap::TuiAction::Back => state.close_backup_prune(),
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            BackupPruneStage::Planning => {
                                if keys.map(state::InputMode::Normal, key)
                                    == Some(keymap::TuiAction::Back)
                                {
                                    state.set_notice("清理计划正在后台生成，请等待完成。");
                                }
                                continue;
                            }
                            BackupPruneStage::Review => {
                                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                                    match action {
                                        keymap::TuiAction::Activate => {
                                            state.backup_prune_begin_confirm()
                                        }
                                        keymap::TuiAction::Back => state.close_backup_prune(),
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            BackupPruneStage::Confirm => {
                                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                                    match action {
                                        keymap::TuiAction::Text(ch) => {
                                            state.backup_prune_push_confirmation(ch);
                                        }
                                        keymap::TuiAction::Backspace => {
                                            state.backup_prune_backspace()
                                        }
                                        keymap::TuiAction::Submit => {
                                            if let Some(prepared) =
                                                state.backup_prune_take_for_execute()
                                            {
                                                if let Err(message) = tasks
                                                    .request_backup_prune_execute(
                                                        prepared,
                                                        backup_dir.clone(),
                                                    )
                                                {
                                                    state.backup_prune_finish_execute(Err(
                                                        message.to_string()
                                                    ));
                                                }
                                            }
                                        }
                                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                                            state.close_backup_prune()
                                        }
                                        keymap::TuiAction::Confirm => state.set_notice(
                                            "备份清理仍需精确输入大写 YES 后按 Enter。",
                                        ),
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            BackupPruneStage::Running => {}
                            BackupPruneStage::Result => {
                                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                                    if matches!(
                                        action,
                                        keymap::TuiAction::Activate | keymap::TuiAction::Back
                                    ) {
                                        state.close_backup_prune();
                                    }
                                }
                                continue;
                            }
                        }
                    }
                    if let Some(stage) = state.backup_delete().map(|delete| delete.stage) {
                        match stage {
                            state::WizardStage::Confirm => {
                                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                                    match action {
                                        keymap::TuiAction::Text(ch) => {
                                            state.push_backup_delete_confirmation(ch);
                                        }
                                        keymap::TuiAction::Backspace => {
                                            state.backspace_backup_delete_confirmation();
                                        }
                                        keymap::TuiAction::Submit => {
                                            if let Some((path, expected_sha256)) =
                                                state.submit_backup_delete_confirmation()
                                            {
                                                if let Err(message) = tasks.request_backup_delete(
                                                    path,
                                                    expected_sha256,
                                                    backup_dir.clone(),
                                                ) {
                                                    state.finish_backup_delete(Err(
                                                        message.to_string()
                                                    ));
                                                }
                                            }
                                        }
                                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                                            let _ = state.navigate(NavCommand::Escape, 1);
                                        }
                                        keymap::TuiAction::Confirm => state.set_notice(
                                            "删除备份仍需精确输入大写 YES 后按 Enter。",
                                        ),
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            state::WizardStage::Running => {}
                            state::WizardStage::Result => {
                                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                                    if matches!(
                                        action,
                                        keymap::TuiAction::Activate | keymap::TuiAction::Back
                                    ) {
                                        let _ = state.navigate(NavCommand::Escape, 1);
                                    }
                                }
                                continue;
                            }
                        }
                    }

                    if let Some(stage) = state.wizard().map(|wizard| wizard.stage) {
                        match stage {
                            state::WizardStage::Confirm => {
                                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                                    match action {
                                        keymap::TuiAction::Text(ch) => {
                                            state.push_wizard_confirmation(ch);
                                        }
                                        keymap::TuiAction::Backspace => {
                                            state.backspace_wizard_confirmation();
                                        }
                                        keymap::TuiAction::Submit => {
                                            if let Some(intent) = state.submit_wizard_confirmation()
                                            {
                                                if !crate::elevate::is_root() {
                                                    return Ok(LoopExit::Elevate(intent));
                                                }
                                                if matches!(
                                                    intent.kind,
                                                    state::WriteKind::BackupCreate
                                                        | state::WriteKind::BackupCreateDeep
                                                ) {
                                                    if let Err(message) = tasks
                                                        .request_backup_create(
                                                            intent,
                                                            backup_dir.clone(),
                                                        )
                                                    {
                                                        state
                                                            .finish_write(Err(message.to_string()));
                                                    }
                                                } else if let Err(message) =
                                                    tasks.request_write(intent, backup_dir.clone())
                                                {
                                                    state.finish_write(Err(message.to_string()));
                                                }
                                            }
                                        }
                                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                                            let _ = state.navigate(NavCommand::Escape, 1);
                                        }
                                        keymap::TuiAction::Confirm => state.set_notice(
                                            "破坏性操作仍需精确输入大写 YES 后按 Enter。",
                                        ),
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            state::WizardStage::Running => {}
                            state::WizardStage::Result => {
                                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                                    if matches!(
                                        action,
                                        keymap::TuiAction::Activate | keymap::TuiAction::Back
                                    ) {
                                        let _ = state.navigate(NavCommand::Escape, 1);
                                    }
                                }
                                continue;
                            }
                        }
                    }

                    if matches!(
                        state.input_mode(),
                        state::InputMode::Search | state::InputMode::Command
                    ) {
                        if let Some(action) = keys.map(state.input_mode(), key) {
                            match action {
                                keymap::TuiAction::Text(ch) => {
                                    state.push_input_char(ch);
                                }
                                keymap::TuiAction::Backspace => {
                                    state.backspace_input();
                                }
                                keymap::TuiAction::Back => {
                                    let _ = state.navigate(NavCommand::Escape, 1);
                                }
                                keymap::TuiAction::Submit => {
                                    if state.input_mode() == state::InputMode::Search {
                                        state.submit_search();
                                    } else {
                                        let input = state.take_input();
                                        state.cancel_input();
                                        match command::parse_command(&input) {
                                            Ok(action) => {
                                                let viewport_height = session
                                                    .terminal
                                                    .size()?
                                                    .height
                                                    .saturating_sub(9)
                                                    as usize;
                                                if action == command::PaletteAction::Provision {
                                                    if state.workspace()
                                                        != state::Workspace::Devices
                                                    {
                                                        let _ = state.navigate(
                                                            NavCommand::WorkspaceDevices,
                                                            viewport_height,
                                                        );
                                                        state.set_notice(
                                                            "请在设备页选定 USB 盘后按 Enter 进入制盘。",
                                                        );
                                                    } else if let Err(message) =
                                                        state.begin_provision_for_selected_device()
                                                    {
                                                        state.set_notice(message);
                                                    }
                                                } else {
                                                    let effect = dispatch_nav_command(
                                                        &mut state,
                                                        &mut tasks,
                                                        palette_action_to_nav(action),
                                                        &backup_dir,
                                                        viewport_height,
                                                    );
                                                    if effect == StateEffect::ExitRequested {
                                                        break;
                                                    }
                                                }
                                            }
                                            Err(message) => state.set_notice(message),
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        continue;
                    }

                    if let Some(action) = keys.map(state.input_mode(), key) {
                        let viewport_height =
                            session.terminal.size()?.height.saturating_sub(9) as usize;
                        match dispatch_tui_action(
                            &mut state,
                            &mut tasks,
                            action,
                            &backup_dir,
                            viewport_height,
                        ) {
                            StateEffect::ExitRequested => break,
                            StateEffect::ExitDeferred | StateEffect::None => {}
                        }
                    }
                }
                ct_event::Event::Resize(_, _) => {}
                _ => {}
            }

            if state.take_deferred_exit() == StateEffect::ExitRequested {
                break;
            }
        }
        Ok(LoopExit::Done)
    })();

    if result.is_err() && tasks.active_operation().is_some() {
        // Restore raw mode/alternate screen before waiting. The transaction keeps
        // running without its UI and is allowed to complete rollback/readback.
        drop(session);
        tasks.wait_for_critical_operation();
    }
    result
}

/// Run the interactive TUI only when both stdin and stdout are terminals.
pub fn run() -> i32 {
    if !is_interactive_terminal() {
        eprintln!("错误: edpcli tui 需要交互式 TTY；脚本请继续使用 CLI v2 子命令。");
        return EXIT_USAGE;
    }

    let argv: Vec<String> = std::env::args().skip(1).collect();

    // 在进入 raw mode / alternate screen 之前获取管理员权限。这样 macOS/Linux
    // 直接在普通终端显示 sudo 密码提示，Windows 直接走 UAC；授权后一次进入
    // 完整能力 TUI，不再等到写盘确认时退出界面再重启。
    ensure_elevated_before_tui(&argv);

    let resume = match parse_resume_args(&argv) {
        Ok(value) => value,
        Err(message) => {
            eprintln!("{message}");
            return EXIT_USAGE;
        }
    };

    match run_loop(resume) {
        Ok(LoopExit::Done) => EXIT_OK,
        Ok(LoopExit::Elevate(intent)) => {
            let argv = resume_argv(&intent);
            crate::elevate::ensure_elevated(&argv);
            unreachable!()
        }
        Err(error) => {
            eprintln!("错误: TUI 终端初始化或事件循环失败: {error}");
            EXIT_IO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provision_form_uses_explicit_insert_mode_for_text_editing() {
        let mut state = AppState::new();
        state.navigate(NavCommand::WorkspaceProvision, 20);
        state.provision_mut().stage = state::ProvisionStage::Form;
        state.provision_mut().field_selected = 0;
        state.provision_mut().form.label_id = "12345".into();

        assert!(state.provision_begin_insert());
        assert_eq!(state.input_mode(), state::InputMode::Insert);
        assert_eq!(state.workspace(), state::Workspace::Provision);

        state.provision_move_cursor(-1);
        assert_eq!(state.provision_field_cursor(), 4);
        state.provision_end_insert();
        assert_eq!(state.input_mode(), state::InputMode::Normal);
        assert_eq!(state.workspace(), state::Workspace::Provision);
    }

    #[test]
    fn tty_gate_is_purely_a_terminal_capability_check() {
        let _ = is_interactive_terminal();
    }

    #[test]
    fn bare_tui_route_becomes_explicit_across_the_elevation_boundary() {
        assert_eq!(
            startup_elevation_argv(&[], false),
            Some(vec!["tui".to_string()])
        );
        assert_eq!(
            startup_elevation_argv(&["tui".to_string()], false),
            Some(vec!["tui".to_string()])
        );
        assert_eq!(startup_elevation_argv(&[], true), None);
    }
}
