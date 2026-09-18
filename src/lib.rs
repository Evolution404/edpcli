//! edpcli — EDP/cems U 盘管理 CLI（识别、元信息、备份、恢复与免密转换）。
//!
//! 分层(与原 Python 版一致, 无环): common → crypto → sectors / diskio → identify → cli。
//! macOS 耦合面收在 sysinfo(diskutil/ioreg) 与 diskio(/dev/rdiskN)。

pub mod common;
pub mod md5;
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
