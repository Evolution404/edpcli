//! Offline assessment of existing Metadata evidence; does not create a Deep
//! capture or claim that a prefix contains a complete filesystem inventory.
use edpcli::{backup_deep::assess_partition, backup_metadata::parse_partition_geometry, edpb};
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: deep_assess_metadata BACKUP.edpb")?,
    );
    if args.next().is_some() {
        return Err("expected one EDPB path".into());
    }
    let verified = edpb::verify_file(&path)?;
    let raw = edpb::read_raw_protocol(&path)?;
    let partitions = parse_partition_geometry(
        &raw,
        &verified.manifest.device.device_id,
        verified
            .manifest
            .geometry
            .total_sectors
            .ok_or("missing disk geometry")?,
    )?;
    let mut reports = Vec::new();
    for p in partitions
        .iter()
        .filter(|p| matches!(p.partition_type, 2 | 4))
    {
        let id = format!("raw.partition.{}.prefix", p.index);
        let prefix = if verified.manifest.artifacts.iter().any(|a| a.id == id) {
            Some(edpb::read_artifact(&path, &id)?)
        } else {
            None
        };
        reports.push(assess_partition(p, prefix.as_deref()));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "source_sha256": verified.file_sha256,
            "assessment_only": true,
            "partitions": reports,
        }))?
    );
    Ok(())
}
