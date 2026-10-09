use super::*;

pub fn find_sector_structured_paths(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    query: &str,
) -> Vec<Vec<String>> {
    find_sector_structured_paths_with_sector_bytes(
        lba,
        decoder,
        status,
        fields,
        query,
        crate::common::SECTOR as u32,
    )
}

pub fn find_sector_structured_paths_with_sector_bytes(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    query: &str,
    native_bytes: u32,
) -> Vec<Vec<String>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let sector = if native_bytes == crate::common::SECTOR as u32 {
        standalone_sector_node_with_fields(lba, decoder, status, fields)
    } else {
        standalone_sector_node_with_fields_and_sector_bytes(
            lba,
            decoder,
            status,
            fields,
            native_bytes,
        )
    };
    let mut out = Vec::new();
    node_paths_match(
        &sector,
        &mut Vec::new(),
        &|node| {
            if node.label.to_lowercase().contains(&query) {
                return true;
            }
            node.range
                .byte_range
                .and_then(|range| fields.iter().find(|field| field.range == range))
                .is_some_and(|field| field.value.to_lowercase().contains(&query))
        },
        &mut out,
    );
    out
}
