use super::{inspect_field_status_style, sector_render::sector_byte_style};
use crate::application::inspect::{
    AbsoluteByteRange, AdvancedInspectItem, InspectField, InspectFieldStatus, InspectFieldType,
};
use crate::inspect::{DecodeRange, FieldStyle, InspectFieldKey, InspectParseState};

fn item(
    status: Option<InspectFieldStatus>,
    decoded: bool,
    unavailable: bool,
) -> AdvancedInspectItem {
    let fields = status
        .map(|status| InspectField {
            key: InspectFieldKey::Synthetic,
            range: AbsoluteByteRange {
                start: 0,
                end_exclusive: 1,
            },
            field_type: InspectFieldType::Identity,
            raw: vec![0],
            decoded: vec![0],
            field_logical: None,
            transform: None,
            status,
            label: "byte0".into(),
            value: "0".into(),
            style: FieldStyle::Identity,
            group: None,
            children: Vec::new(),
        })
        .into_iter()
        .collect();
    AdvancedInspectItem {
        lba: 0,
        regions: vec!["test".into()],
        raw: vec![0; crate::common::SECTOR],
        raw_sha256: "raw".into(),
        raw_nonzero: 0,
        decoded: Some(vec![0; crate::common::SECTOR]),
        decode_ranges: if decoded {
            vec![DecodeRange::new(0, 1)]
        } else {
            Vec::new()
        },
        decoded_sha256: Some("decoded".into()),
        method: Some("test".into()),
        decode_error: unavailable.then(|| "decoder unavailable".into()),
        parse_state: InspectParseState::Parsed,
        diagnostics: Vec::new(),
        fields,
        notes: Vec::new(),
        meta_text: None,
    }
}

#[test]
fn mixed_decode_provenance_does_not_depend_on_byte_equality() {
    let semantic = inspect_field_status_style(InspectFieldStatus::Known);
    let decoded = item(Some(InspectFieldStatus::Known), true, false);
    assert_eq!(decoded.raw[0], decoded.decoded.as_ref().unwrap()[0]);
    let style = sector_byte_style(
        &decoded,
        crate::tui::state::SectorInspectMode::Mixed,
        0,
        false,
    );
    let expected = crate::tui::theme::current().inspect_decode_overlay(semantic);
    assert_eq!(style.fg, expected.fg);
    assert_eq!(style.bg, expected.bg);

    let plain = item(Some(InspectFieldStatus::Known), false, false);
    let plain_style = sector_byte_style(
        &plain,
        crate::tui::state::SectorInspectMode::Mixed,
        0,
        false,
    );
    assert_eq!(plain_style.fg, semantic.fg);
    assert_eq!(plain_style.bg, semantic.bg);
}

#[test]
fn mixed_preserves_all_field_semantics_under_decode_overlay() {
    for status in [
        InspectFieldStatus::Known,
        InspectFieldStatus::Unknown,
        InspectFieldStatus::Reserved,
        InspectFieldStatus::Preserved,
    ] {
        let item = item(Some(status), true, false);
        let style = sector_byte_style(&item, crate::tui::state::SectorInspectMode::Mixed, 0, false);
        let semantic = inspect_field_status_style(status);
        assert_eq!(style.fg, semantic.fg, "{status:?}");
        assert_eq!(
            style.bg,
            crate::tui::theme::current()
                .inspect_decode_overlay(semantic)
                .bg,
            "{status:?}"
        );
    }
}

#[test]
fn unavailable_and_cursor_are_overlays_not_replacements() {
    let unavailable = item(Some(InspectFieldStatus::Reserved), false, true);
    let semantic = inspect_field_status_style(InspectFieldStatus::Reserved);
    let style = sector_byte_style(
        &unavailable,
        crate::tui::state::SectorInspectMode::Mixed,
        0,
        true,
    );
    let expected = crate::tui::theme::current().inspect_cursor_overlay(
        crate::tui::theme::current().inspect_decode_unavailable_overlay(semantic),
    );
    assert_eq!(style.fg, expected.fg);
    assert_eq!(style.bg, expected.bg);
    assert_eq!(style.add_modifier, expected.add_modifier);
}
