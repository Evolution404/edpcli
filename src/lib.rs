//! edpcli — EDP/cems U 盘管理 CLI（识别、元信息、备份、恢复与免密转换）。
//!
//! 分层: common → platform / crypto → sectors / diskio → identify → cli。
//! 操作系统差异统一收敛在 platform；业务核心不得直接依赖 macOS/Linux/Windows API。

pub mod application;
pub mod backup_catalog;
pub mod backup_cli;
pub mod build_info;
pub mod cli;
pub mod cli_args;
pub mod common;
pub mod completion;
pub mod crypto;
pub mod disk_scan;
pub mod diskio;
pub mod elevate;
pub mod identify;
pub mod inspect;
pub(crate) mod inspect_cli;
pub mod md5;
pub mod metainfo;
pub(crate) mod metainfo_cli;
pub mod platform;
pub mod plist;
pub mod provision;
pub mod sectors;
pub mod selectors;
pub mod sysinfo;
pub mod tui;
pub mod ui;
