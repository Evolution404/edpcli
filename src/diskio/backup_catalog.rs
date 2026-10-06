//! Compatibility facade; backup storage is owned by infrastructure.
pub use crate::infrastructure::backup_store::catalog::*;

pub use crate::infrastructure::backup_store::display_catalog::{
    scan_backup_dir_display, DisplayCatalogScan,
};
