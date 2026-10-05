//! Shared presentation primitives. They consume application-owned data and perform no I/O.

pub mod badge;
pub mod banner;
pub mod card;
pub mod confirmation;
pub mod message;
pub mod modal;
pub mod operation_result;
pub mod panel;
pub mod responsive;
mod result_supplement;
mod result_table;
pub mod table;
pub(crate) mod text;
pub mod workspace_overview;

pub use badge::{status_badge, BadgeTone};
pub use banner::{notice_banner, BannerTone};
pub use card::card;
pub use confirmation::{
    render_action_confirmation_modal, render_write_confirmation_modal, ActionConfirmationSpec,
    ConfirmationTone, MediaWriteConfirmationKind, WriteConfirmationSpec,
};
pub use message::{UiMessage, UiMessageTone};
pub use modal::{centered_modal_rect, render_modal};
pub(crate) use operation_result::tone_style;
pub use operation_result::{
    render_operation_result, OperationResultSpec, ResultCard, ResultField, ResultTone, ResultValue,
};
pub use panel::panel;
pub use responsive::ViewportClass;
pub use result_supplement::ResultSupplement;
pub use result_table::ResultTable;
pub use table::data_table;
pub use workspace_overview::{workspace_overview, OverviewMetric, OverviewSearch};
