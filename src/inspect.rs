//! Read-only Inspect presentation facade. Protocol adaptation lives in
//! `inspect_adapter`; UI text and hex rendering stay downstream of application.

pub fn render_fields(view: &SectorView) -> String {
    crate::application::inspect_text::render_fields_with(
        view,
        crate::application::inspect_text::InspectTextTheme {
            paint: render::paint,
            emphasize: crate::ui::bold_cyan,
            dim: crate::ui::dim,
        },
    )
}
pub use crate::inspect_adapter::{
    analyze_sector, analyze_sector_with_context, DecodeRange, FieldChild, FieldStyle,
    FieldTransform, InspectDiagnostic, InspectDiagnosticCode, InspectFieldKey, InspectMeta,
    InspectParseState, SectorField, SectorFieldStatus, SectorView,
};

mod render;
pub use render::{overview_line, render_hex};
