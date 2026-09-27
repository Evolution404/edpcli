//! Terminal-host clipboard integration for TUI copy actions.

use std::io;

use crossterm::{clipboard::CopyToClipboard, execute};

pub fn copy_text(content: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, CopyToClipboard::to_clipboard_from(content))
}
