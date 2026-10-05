//! A refresh-scoped read model. Never retained as authority for a later write/delete.
use crate::backup_catalog::BackupCatalog;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug)]
pub(crate) struct CatalogSnapshot {
    root: PathBuf,
    catalog: OnceLock<BackupCatalog>,
}
impl CatalogSnapshot {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            catalog: OnceLock::new(),
        }
    }
    pub(crate) fn catalog(&self) -> &BackupCatalog {
        self.catalog.get_or_init(|| BackupCatalog::load(&self.root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_refresh_consumers_load_one_catalog() {
        let snapshot = CatalogSnapshot::new(Path::new("missing-audit-catalog"));
        let loads = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    snapshot.catalog.get_or_init(|| {
                        loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        BackupCatalog::load(&snapshot.root)
                    });
                });
            }
        });
        assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(std::ptr::eq(snapshot.catalog(), snapshot.catalog()));
    }
}
