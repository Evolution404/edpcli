//! Internal TUI command palette.
//!
//! Commands map only to typed application intents; no shell parsing or process execution is allowed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    Devices,
    Backups,
    Inspect,
    Apply,
    Restore,
    Refresh,
    Help,
    Quit,
}

pub fn parse_command(input: &str) -> Result<PaletteAction, String> {
    let command = input.trim().trim_start_matches(':').trim().to_ascii_lowercase();
    match command.as_str() {
        "devices" | "device" | "d" => Ok(PaletteAction::Devices),
        "backups" | "backup" | "b" => Ok(PaletteAction::Backups),
        "inspect" | "i" => Ok(PaletteAction::Inspect),
        "apply" | "a" => Ok(PaletteAction::Apply),
        "restore" | "r" => Ok(PaletteAction::Restore),
        "refresh" | "reload" => Ok(PaletteAction::Refresh),
        "help" | "h" | "?" => Ok(PaletteAction::Help),
        "quit" | "q" => Ok(PaletteAction::Quit),
        "" => Err("命令不能为空".into()),
        _ => Err(format!("未知 TUI 命令: {input}")),
    }
}
