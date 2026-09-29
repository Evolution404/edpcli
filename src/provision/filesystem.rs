//! Provision-level adapter for file migration image construction.

mod migration;
pub use migration::build_migrated_filesystem;
