//! Typed, read-only EDP protocol API. Parsing never performs device I/O.
//! Profile axes stay orthogonal; unidentified states remain Unknown.
pub mod edpf;
pub mod iir;
pub mod image;
pub mod layout;
pub mod lba0;
pub mod lba1;
pub mod lba10;
pub mod lba11;
pub mod lba12;
pub mod lba2;
pub mod lba3;
pub mod lba4;
pub mod lba5;
pub mod lba7_compat;
pub mod profile;
pub mod types;

pub mod lba6;
pub mod lba7;
pub mod lba8;
pub mod lba9;
