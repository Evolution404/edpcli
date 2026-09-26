//! Read-only Inspect presentation facade. Protocol adaptation lives in
//! `inspect_adapter`; UI text and hex rendering stay downstream of application.

pub use crate::application::inspect_text::render_fields;
pub use crate::inspect_adapter::{
    analyze_sector, analyze_sector_with_context, FieldChild, FieldStyle, FieldTransform,
    InspectDiagnostic, InspectDiagnosticCode, InspectFieldKey, InspectMeta, InspectParseState,
    SectorField, SectorFieldStatus, SectorView,
};

mod render;
pub use render::{overview_line, render_hex};
