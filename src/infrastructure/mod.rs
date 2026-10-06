//! Side-effect implementations. Each implementation has a single canonical module path.
pub mod backup_store;
pub(crate) mod process;

pub mod clock;

pub(crate) mod atomic_file;
