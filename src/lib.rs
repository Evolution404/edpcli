//! edpcli — EDP/cems U 盘管理 CLI（识别、元信息、备份、恢复与安全制盘）。
//!
//! 分层: frontend (CLI/TUI) → application → domain + stable facades → infrastructure/platform.
//! 领域层不得依赖前端或平台副作用；写盘安全状态转换与授权统一收敛在 application。

pub mod application;
pub(crate) mod backup_catalog;
pub(crate) mod backup_cli;
pub(crate) mod backup_coverage;
pub(crate) mod backup_metadata;
pub(crate) mod backup_restore_preview;
pub(crate) mod build_info;
pub mod cli;
pub mod cli_args;
pub mod command_spec;
pub(crate) mod common;
pub mod completion;
pub(crate) mod crypto;
pub(crate) mod disk_layout;
pub(crate) mod disk_scan;
mod disk_scan_render;
pub mod diskio;
pub(crate) mod domain;
pub mod edpb;
pub(crate) mod elevate;
pub(crate) mod filesystem;
pub(crate) mod filesystem_capability;
pub(crate) mod identify;
mod infrastructure;
pub mod inspect;
pub(crate) mod inspect_adapter;
pub(crate) mod inspect_cli;
pub(crate) mod inspect_target;
pub(crate) mod media_identity;
pub(crate) mod media_identity_observer;
pub(crate) mod metainfo;
pub(crate) mod metainfo_cli;
pub(crate) mod partition_table;
pub(crate) mod partition_transform;
pub mod platform;
#[cfg(target_os = "macos")]
pub(crate) mod plist;
pub mod protocol;
pub mod provision;
pub(crate) mod sectors;
pub(crate) mod selectors;
pub(crate) mod sha256;
pub(crate) mod sysinfo;
pub(crate) mod text_width;
pub mod tui;
pub(crate) mod ui;
