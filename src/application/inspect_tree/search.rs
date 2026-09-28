use super::*;

pub fn find_sector_structured_path(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    query: &str,
) -> Option<Vec<String>> {
    find_sector_structured_paths(lba, decoder, status, fields, query)
        .into_iter()
        .next()
}

pub fn find_sector_structured_paths(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    query: &str,
) -> Vec<Vec<String>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let sector = sector_node_with_fields(lba, decoder, status, fields);
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
