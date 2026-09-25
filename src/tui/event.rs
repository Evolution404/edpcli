//! Crossterm event compatibility surface.
//!
//! Key bindings live in `keymap.rs`; this module remains a thin re-export for callers that
//! historically imported event translation from `tui::event`.

pub use super::keymap::{is_actionable_key, KeyMapper};
