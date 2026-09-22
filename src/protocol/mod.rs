//! Typed, read-only EDP protocol API. Parsing never performs device I/O.
//! Profile axes stay orthogonal; unidentified states remain Unknown.
pub mod layout;
pub mod lba0;
pub mod lba1;
pub mod lba2;
pub mod lba3;
pub mod lba4;
pub mod lba5;
pub mod profile;
pub mod types;
