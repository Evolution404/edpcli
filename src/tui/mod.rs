//! Interactive terminal frontend.
//!
//! The concrete event/state/render implementation is added in Phase 1. This module is already
//! routed through the public CLI surface so architecture contracts can prevent any later frontend
//! from bypassing the shared application/service layer.

use crate::common::EXIT_USAGE;

/// Enter the interactive frontend.
///
/// Phase 0 intentionally keeps terminal I/O out of this shim. Phase 1 replaces this body with the
/// ratatui/crossterm lifecycle while preserving this stable CLI entrypoint.
pub fn run() -> i32 {
    eprintln!("TUI 尚未初始化；当前开发分支正在实施交互层。");
    EXIT_USAGE
}
