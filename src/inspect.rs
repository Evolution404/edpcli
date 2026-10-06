//! Read-only Inspect protocol model facade. Decoding lives in `inspect_adapter`;
//! current text export and TUI rendering are owned by their respective layers.

pub use crate::inspect_adapter::{
    analyze_sector_with_context, DecodeRange, FieldChild, FieldStyle, FieldTransform,
    InspectDiagnostic, InspectDiagnosticCode, InspectFieldKey, InspectMeta, InspectParseState,
    SectorField, SectorFieldStatus, SectorView,
};
pub use crate::inspect_target::{InspectDiskContext, PhysicalDataState, SectorRegion};
