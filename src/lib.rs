//! edpcli — EDP/cems U 盘管理 CLI（识别、元信息、备份、恢复与免密转换）。
//!
//! 分层: common → platform / crypto → sectors / diskio → identify → cli。
//! 操作系统差异统一收敛在 platform；业务核心不得直接依赖 macOS/Linux/Windows API。

pub mod common;
pub mod md5;
pub mod platform;
#[cfg(target_os = "macos")]
pub(crate) mod native_probe;
pub mod plist;
pub mod crypto;
pub mod sectors;
pub mod identify;
pub mod sysinfo;
pub mod diskio;
pub mod elevate;
pub mod ui;
pub mod cli;
pub mod inspect;
pub mod metainfo;
pub mod completion;
pub mod backup_catalog;
pub mod backup_cli;
pub mod disk_scan;
pub mod cli_args;
pub(crate) mod inspect_cli;
pub(crate) mod metainfo_cli;
