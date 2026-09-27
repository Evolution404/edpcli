use super::*;

pub(super) fn dispatch_nav_command(
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
                state::Workspace::Inspect => {}
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
                state::Workspace::Inspect => state
                    .selected_device_disk()
                    .map(state::AdvancedInspectSource::Disk),
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
                let identity = row
                    .identity_pin
                    .as_ref()
                    .map(state::ExpectedIdentity::from_pin);
                if let Some(identity) = identity {
                    state.begin_write_wizard_for_identity(
                        state::WriteKind::Restore,
                        disk,
                        Some(backup),
                        Some(identity),
                    );
                } else {
                    state.set_notice("目标介质身份尚未完成只读采集，请刷新设备后重试。");
                }
            } else {
                state.set_notice("恢复需要先在设备页选定目标 U 盘，再进入备份页选择备份。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupCreate => {
            if let Some(row) = state.selected_device() {
                let disk = row.disk;
                let identity = row
                    .identity_pin
                    .as_ref()
                    .map(state::ExpectedIdentity::from_pin);
                if let Some(identity) = identity {
                    state.begin_write_wizard_for_identity(
                        state::WriteKind::BackupCreate,
                        disk,
                        None,
                        Some(identity),
                    );
                } else {
                    state.set_notice("目标介质身份尚未完成只读采集，请刷新设备后重试。");
                }
            } else {
                state.set_notice("创建备份需要先在设备页选定 U 盘。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupCreateDeep => {
            if let Some(row) = state.selected_device() {
                let disk = row.disk;
                let identity = row
                    .identity_pin
                    .as_ref()
                    .map(state::ExpectedIdentity::from_pin);
                if let Some(identity) = identity {
                    state.begin_write_wizard_for_identity(
                        state::WriteKind::BackupCreateDeep,
                        disk,
                        None,
                        Some(identity),
                    );
                } else {
                    state.set_notice("目标介质身份尚未完成只读采集，请刷新设备后重试。");
                }
            } else {
                state.set_notice("深度备份需要先在设备页选定 U 盘。");
            }
            StateEffect::None
        }
        NavCommand::VerifyBackup => {
            if let Some(path) = state.selected_backup_path() {
                state.set_notice("正在后台校验当前备份…");
                state.begin_backup_verify_run(path.clone());
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

pub(super) fn palette_action_to_nav(action: command::PaletteAction) -> NavCommand {
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

pub(super) fn dispatch_tui_action(
    state: &mut AppState,
    tasks: &mut TaskHub,
    action: keymap::TuiAction,
    role: keymap::WidgetRole,
    backup_dir: &std::path::Path,
    viewport_height: usize,
    viewport_width: u16,
) -> StateEffect {
    let mut clipboard = clipboard::ClipboardService;
    let outcome = controller::dispatch_action(
        state,
        action,
        role,
        viewport_height,
        viewport_width,
        &mut clipboard,
    );
    if !outcome.handled {
        return StateEffect::None;
    }
    let mut effect = outcome.effect;
    if let Some(request) = outcome.request {
        effect = execute_action_request(state, tasks, request, backup_dir, viewport_height);
    }
    effect
}

fn execute_action_request(
    state: &mut AppState,
    tasks: &mut TaskHub,
    request: controller::ActionRequest,
    backup_dir: &std::path::Path,
    viewport_height: usize,
) -> StateEffect {
    match request {
        controller::ActionRequest::Navigate(command) => {
            dispatch_nav_command(state, tasks, command, backup_dir, viewport_height)
        }
        controller::ActionRequest::InspectSelection { force_hex } => {
            open_advanced_inspect_selection(state, tasks, force_hex);
            StateEffect::None
        }
        controller::ActionRequest::InspectPreview { source, lba } => {
            if let Err(message) = tasks.request_advanced_inspect_preview(source, lba) {
                state.advanced_inspect_sector_finish(lba, Err(message.to_string()));
            }
            StateEffect::None
        }
        controller::ActionRequest::ProvisionKeyProbe { disk } => {
            if let Err(message) = tasks.request_provision_key_probe(disk) {
                state.provision_finish_key_probe(Err(message.to_string()));
            }
            StateEffect::None
        }
        controller::ActionRequest::ProvisionSourcePasswordVerify => {
            start_provision_source_password_verify(state, tasks);
            StateEffect::None
        }
        controller::ActionRequest::ProvisionPlan => {
            start_provision_plan(state, tasks);
            StateEffect::None
        }
    }
}

pub(super) fn open_advanced_inspect_selection(
    state: &mut AppState,
    tasks: &mut TaskHub,
    force_hex: bool,
) {
    let detail_selected = state.advanced_inspect_focused_pane()
        == Some(crate::tui::pane::PaneId::InspectDetail)
        && state.advanced_inspect_detail_selected_row().is_some();
    let request = if detail_selected {
        state.advanced_inspect_detail_open_selected()
    } else if force_hex && state.advanced_inspect_selected_field().is_some() {
        state.advanced_inspect_open_selected_field()
    } else if state.advanced_inspect_selected_sector_lba().is_some() {
        state.advanced_inspect_open_selected_sector()
    } else if let Some(field) = state.advanced_inspect_selected_field() {
        if field.key == crate::inspect::InspectFieldKey::Lba8Elabel {
            state.advanced_inspect_view_selected_field();
        } else {
            state.advanced_inspect_focus_pane(crate::tui::pane::PaneId::InspectDetail);
        }
        None
    } else {
        state.advanced_inspect_enter_selected();
        None
    };

    if let Some((source, lba)) = request {
        if let Err(message) = tasks.request_advanced_inspect_sector(source, lba) {
            if message == "已有扇区读取正在执行" {
                state.advanced_inspect_mark_decode_pending(lba, false);
            } else {
                state.advanced_inspect_sector_finish(lba, Err(message.to_string()));
            }
        }
    }
}

pub(super) fn start_provision_source_password_verify(state: &mut AppState, tasks: &mut TaskHub) {
    let Some(disk) = state.selected_device_disk() else {
        state.provision_mut().message = Some("目标 USB 已不存在，请返回设备页重新选择。".into());
        return;
    };
    match state.provision_source_password_verify_request() {
        Ok(Some((domain, password))) => {
            state.provision_mut().message = Some(match domain {
                crate::provision::KeyDomainRole::Share => "正在只读验证交换域来源密码…".into(),
                crate::provision::KeyDomainRole::Encrypt => "正在只读验证保密域来源密码…".into(),
            });
            if let Err(message) =
                tasks.request_provision_source_password_verify(disk, domain, password)
            {
                state.provision_finish_source_password_verify(domain, Err(message.to_string()));
            }
        }
        Ok(None) => {
            state.provision_mut().message =
                Some("当前字段不是来源密码；v 仅验证来源密码域。".into());
        }
        Err(message) => {
            state.provision_mut().message = Some(message);
        }
    }
}

pub(super) fn start_provision_plan(state: &mut AppState, tasks: &mut TaskHub) {
    let Some(disk) = state.selected_device_disk() else {
        state.provision_mut().message = Some("目标 USB 已不存在，请返回设备页重新选择。".into());
        return;
    };
    let request = if state.provision().kind == state::ProvisionKind::Plain {
        match state.provision_plain_plan() {
            Ok(plan) => {
                let mut request =
                    crate::application::provision::PlainProvisionRequest::from_plan(&plan);
                request.key_domains = crate::provision::KeyDomainSecrets::new(
                    crate::provision::KeyDomainSecretPair::new(
                        (!state.provision().form.share_source_password.is_empty())
                            .then_some(state.provision().form.share_source_password.as_bytes()),
                        None::<&[u8]>,
                    ),
                    crate::provision::KeyDomainSecretPair::new(
                        (!state.provision().form.encrypt_source_password.is_empty())
                            .then_some(state.provision().form.encrypt_source_password.as_bytes()),
                        None::<&[u8]>,
                    ),
                );
                crate::application::provision::ProvisionRequest::Plain(request)
            }
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
