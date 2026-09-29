use super::state;

const RESUME_KIND_FLAG: &str = "--_resume-kind";
const RESUME_DISK_FLAG: &str = "--_resume-disk";
const RESUME_BACKUP_FLAG: &str = "--_resume-backup";
const RESUME_IDENTITY_PIN_FLAG: &str = "--_resume-identity-pin";

/// Serialize a confirmed write intent for an elevated TUI restart.
///
/// The disk is converted to the platform-native selector before crossing the privilege boundary;
/// restore additionally pins the exact backup path. Media-write authorization is re-established by the TUI before any write begins.
pub fn resume_argv(intent: &state::WriteIntent) -> Vec<String> {
    let mut argv = vec![
        "tui".to_string(),
        RESUME_KIND_FLAG.to_string(),
        match intent.kind {
            state::WriteKind::Restore => "restore".to_string(),
            state::WriteKind::BackupCreate => "backup-create".to_string(),
        },
        RESUME_DISK_FLAG.to_string(),
        crate::application::pin_disk_selector(intent.disk),
    ];
    if let Some(path) = &intent.backup {
        argv.push(RESUME_BACKUP_FLAG.to_string());
        argv.push(path.to_string_lossy().into_owned());
    }
    if let Some(identity) = &intent.expected_identity {
        argv.push(RESUME_IDENTITY_PIN_FLAG.to_string());
        argv.push(serde_json::to_string(identity).unwrap_or_else(|_| "{}".into()));
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
    let mut identity_pin = None;
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
            RESUME_IDENTITY_PIN_FLAG => {
                if identity_pin.is_some() {
                    return Err(format!("错误: {RESUME_IDENTITY_PIN_FLAG} 重复指定"));
                }
                saw_resume = true;
                let raw = take(RESUME_IDENTITY_PIN_FLAG)?;
                let pin: state::ExpectedIdentity = serde_json::from_str(&raw)
                    .map_err(|error| format!("错误: TUI resume identity pin 无效: {error}"))?;
                pin.validate().map_err(|error| format!("错误: {error}"))?;
                identity_pin = Some(pin);
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
    let identity_pin =
        identity_pin.ok_or_else(|| format!("错误: TUI resume 缺少 {RESUME_IDENTITY_PIN_FLAG}"))?;
    match kind {
        state::WriteKind::BackupCreate if backup.is_some() => {
            Err("错误: 非 Restore resume 不允许携带备份路径".into())
        }
        state::WriteKind::Restore if backup.is_none() => {
            Err(format!("错误: restore resume 缺少 {RESUME_BACKUP_FLAG}"))
        }
        _ => Ok(Some(state::WriteIntent {
            kind,
            disk,
            backup,
            expected_identity: Some(identity_pin),
        })),
    }
}
