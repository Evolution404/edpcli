use super::*;

pub(super) fn overview_lines(manifest: &crate::edpb::Manifest) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "EDPB Manifest",
            accent().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "Schema  {} · Container {}.{}",
            safe(&manifest.schema),
            manifest.container_version.major,
            manifest.container_version.minor
        )),
        Line::from(format!(
            "Region {} · Extent {} · Artifact {}",
            manifest.regions.len(),
            manifest.extents.len(),
            manifest.artifacts.len()
        )),
    ];
    if let Some(contract) = manifest.restore_contract.as_ref() {
        lines.push(Line::from(format!(
            "RestoreContract  structure={} protocol={} filesystem={} user_data={} assessment={}",
            contract.restores_partition_structure,
            contract.restores_edp_protocol,
            contract.restores_filesystem,
            contract.restores_user_data,
            contract.post_restore_assessment_required
        )));
    }
    lines
}

pub(super) fn detail_lines(manifest: &crate::edpb::Manifest) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "EDPB Manifest 技术证据",
            secondary().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled("Regions", accent())),
    ];
    for region in &manifest.regions {
        lines.push(Line::from(format!(
            "{} · role={} · start={} · sectors={} · status={:?}",
            safe(&region.id),
            safe(&region.role),
            region
                .start_lba
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".into()),
            region
                .sector_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".into()),
            region.semantic_status
        )));
    }

    lines.push(Line::from(Span::styled("Extents", accent())));
    for extent in &manifest.extents {
        lines.push(Line::from(format!(
            "{} · region={} · LBA{} +{} · {}",
            safe(&extent.id),
            safe(&extent.region_id),
            extent.start_lba,
            extent.sector_count,
            safe(&extent.purpose)
        )));
    }

    lines.push(Line::from(Span::styled("Artifacts", accent())));
    for artifact in &manifest.artifacts {
        lines.push(Line::from(format!(
            "{} · kind={} · policy={:?} · completeness={:?}",
            safe(&artifact.id),
            safe(&artifact.kind),
            artifact.restore_policy,
            artifact.completeness
        )));
        lines.push(Line::from(format!(
            "  source_extent_ids: {}",
            artifact
                .source_extent_ids
                .iter()
                .map(|value| safe(value))
                .collect::<Vec<_>>()
                .join(", ")
        )));
        lines.push(Line::from(format!(
            "  SHA-256: {}",
            safe(&artifact.storage.sha256)
        )));
    }
    lines
}
