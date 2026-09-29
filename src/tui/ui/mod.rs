//! Shared presentation primitives. They consume application-owned data and perform no I/O.

pub mod badge;
pub mod banner;
pub mod card;
pub mod modal;
pub mod panel;
pub mod responsive;
pub mod table;

pub use badge::{status_badge, BadgeTone};
pub use banner::{notice_banner, BannerTone};
pub use card::card;
pub use modal::{centered_modal_rect, render_modal};
pub use panel::panel;
pub use responsive::ViewportClass;
pub use table::data_table;
