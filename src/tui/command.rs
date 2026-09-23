//! Internal TUI command palette.
//!
//! Commands map only to typed application intents; no shell parsing or process execution is allowed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    Devices,
    Backups,
    Provision,
    OfflineConvert,
    Inspect,
    Apply,
    Restore,
    BackupCreate,
    BackupCreateDeep,
    BackupVerify,
    BackupDelete,
    BackupPrune,
    Refresh,
    Help,
    Quit,
}

pub fn parse_command(input: &str) -> Result<PaletteAction, String> {
    let command = input
        .trim()
        .trim_start_matches(':')
        .trim()
        .to_ascii_lowercase();
    match command.as_str() {
        "devices" | "device" | "list" | "info" | "d" => Ok(PaletteAction::Devices),
        "backups" | "backup" | "b" => Ok(PaletteAction::Backups),
        "provision" | "make" | "p" => Ok(PaletteAction::Provision),
        "convert" | "offline-convert" | "offline" | "oc" => Ok(PaletteAction::OfflineConvert),
        "inspect" | "i" => Ok(PaletteAction::Inspect),
        "apply" | "a" => Ok(PaletteAction::Apply),
        "restore" | "r" => Ok(PaletteAction::Restore),
        "backup-create" | "create-backup" | "bc" => Ok(PaletteAction::BackupCreate),
        "backup-deep" | "deep-backup" | "bdp" => Ok(PaletteAction::BackupCreateDeep),
        "backup-verify" | "verify-backup" | "verify" | "v" => Ok(PaletteAction::BackupVerify),
        "backup-delete" | "delete-backup" | "delete" | "bd" => Ok(PaletteAction::BackupDelete),
        "backup-prune" | "prune" | "bp" => Ok(PaletteAction::BackupPrune),
        "refresh" | "reload" => Ok(PaletteAction::Refresh),
        "help" | "h" | "?" => Ok(PaletteAction::Help),
        "quit" | "q" => Ok(PaletteAction::Quit),
        "" => Err("命令不能为空".into()),
        _ => Err(format!("未知 TUI 命令: {input}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_exposes_provision_deep_backup_and_prune() {
        assert_eq!(
            parse_command(":provision").unwrap(),
            PaletteAction::Provision
        );
        assert_eq!(
            parse_command("deep-backup").unwrap(),
            PaletteAction::BackupCreateDeep
        );
        assert_eq!(parse_command("prune").unwrap(), PaletteAction::BackupPrune);
        assert_eq!(
            parse_command("offline-convert").unwrap(),
            PaletteAction::OfflineConvert
        );
        assert_eq!(parse_command("list").unwrap(), PaletteAction::Devices);
        assert_eq!(parse_command("info").unwrap(), PaletteAction::Devices);
        assert_eq!(
            parse_command("convert").unwrap(),
            PaletteAction::OfflineConvert
        );
    }
}
