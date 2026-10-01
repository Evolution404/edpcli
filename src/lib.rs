//! edpcli — EDP/cems U 盘管理 CLI（识别、元信息、备份、恢复与安全制盘）。
//!
//! 分层: common → platform / crypto → sectors / diskio → identify → cli。
//! 操作系统差异统一收敛在 platform；业务核心不得直接依赖 macOS/Linux/Windows API。

pub mod application;
pub mod backup_catalog;
pub(crate) mod backup_cli;
pub(crate) mod backup_coverage;
pub mod backup_metadata;
pub(crate) mod build_info;
pub mod cli;
pub mod cli_args;
pub mod command_spec;
pub mod common;
pub mod completion;
pub mod crypto;
pub(crate) mod disk_layout;
pub mod disk_scan;
mod disk_scan_render;
pub mod diskio;
pub mod edpb;
pub(crate) mod elevate;
pub mod filesystem;
pub mod filesystem_capability;
pub mod identify;
pub mod inspect;
pub(crate) mod inspect_adapter;
pub(crate) mod inspect_cli;
pub mod inspect_target;
pub(crate) mod media_identity;
pub(crate) mod media_identity_observer;
pub mod metainfo;
pub(crate) mod metainfo_cli;
pub(crate) mod partition_table;
pub mod partition_transform;
pub mod platform;
#[cfg(target_os = "macos")]
pub(crate) mod plist;
pub mod protocol;
pub mod provision;
pub mod sectors;
pub mod selectors;
pub mod sha256;
pub mod sysinfo;
pub(crate) mod text_width;
pub mod tui;
pub mod ui;
