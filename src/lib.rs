//! edpcli — EDP/cems U 盘管理 CLI（识别、元信息、备份、恢复与安全制盘）。
//!
//! 分层: frontend (CLI/TUI) → application → domain + stable facades → infrastructure/platform.
//! 领域层不得依赖前端或平台副作用；写盘安全状态转换与授权统一收敛在 application。

pub mod application;
#[doc(hidden)]
pub mod backup_catalog;
pub(crate) mod backup_cli;
pub(crate) mod backup_coverage;
#[doc(hidden)]
pub mod backup_metadata;
pub(crate) mod backup_restore_preview;
pub(crate) mod build_info;
pub mod cli;
pub mod cli_args;
pub mod command_spec;
#[doc(hidden)]
pub mod common;
pub mod completion;
#[doc(hidden)]
pub mod crypto;
pub(crate) mod disk_layout;
#[doc(hidden)]
pub mod disk_scan;
mod disk_scan_render;
pub mod diskio;
pub(crate) mod domain;
pub mod edpb;
pub(crate) mod elevate;
#[doc(hidden)]
pub mod filesystem;
#[doc(hidden)]
pub mod filesystem_capability;
#[doc(hidden)]
pub mod identify;
mod infrastructure;
pub mod inspect;
pub(crate) mod inspect_adapter;
pub(crate) mod inspect_cli;
#[doc(hidden)]
pub mod inspect_target;
pub(crate) mod media_identity;
pub(crate) mod media_identity_observer;
#[doc(hidden)]
pub mod metainfo;
pub(crate) mod metainfo_cli;
pub(crate) mod partition_table;
#[doc(hidden)]
pub mod partition_transform;
pub mod platform;
#[cfg(target_os = "macos")]
pub(crate) mod plist;
pub mod protocol;
pub mod provision;
#[doc(hidden)]
pub mod sectors;
#[doc(hidden)]
pub mod selectors;
#[doc(hidden)]
pub mod sha256;
#[doc(hidden)]
pub mod sysinfo;
pub(crate) mod text_width;
pub mod tui;
#[doc(hidden)]
pub mod ui;
