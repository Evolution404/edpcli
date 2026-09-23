//! Offline check of default mode2 key unwrap and decoded exFAT boot checksums.
//! This reads an existing EDPB only; it never opens the source disk.
use edpcli::{backup_deep::keys, backup_metadata::parse_partition_geometry, edpb};
use std::path::PathBuf;

fn exfat_boot_checksum(boot: &[u8]) -> Option<bool> {
    if boot.len() < 12 * 512 || &boot[3..11] != b"EXFAT   " {
        return None;
    }
    let calculated = boot[..11 * 512]
        .iter()
        .enumerate()
        .filter(|(offset, _)| !matches!(offset, 106 | 107 | 112))
        .fold(0u32, |sum, (_, byte)| {
            sum.rotate_right(1).wrapping_add(*byte as u32)
        });
    Some(
        boot[11 * 512..12 * 512]
            .chunks_exact(4)
            .all(|word| u32::from_le_bytes(word.try_into().unwrap()) == calculated),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("usage: deep_default_probe BACKUP.edpb")?);
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
    let mut results = Vec::new();
    for partition in partitions
        .iter()
        .filter(|p| matches!(p.partition_type, 2 | 4) && p.need_encrypt != 0)
    {
        let prefix_id = format!("raw.partition.{}.prefix", partition.index);
        let key = match keys::default_file_key(
            &raw,
            &verified.manifest.device.device_id,
            partition.index,
        ) {
            Ok(key) => key,
            Err(reason) => {
                results.push(serde_json::json!({
                    "partition_index": partition.index,
                    "status": "locked",
                    "reason": reason,
                }));
                continue;
            }
        };
        if !verified
            .manifest
            .artifacts
            .iter()
            .any(|a| a.id == prefix_id)
        {
            results.push(serde_json::json!({
                "partition_index": partition.index,
                "status": "not_captured",
                "reason": "partition prefix is missing",
            }));
            continue;
        }
        let prefix = edpb::read_artifact(&path, &prefix_id)?;
        let decoded = keys::decrypt_mode2(&prefix, &key)?;
        results.push(serde_json::json!({
            "partition_index": partition.index,
            "status": "decoded_prefix",
            "filesystem_signature": if decoded.len() >= 11 {
                String::from_utf8_lossy(&decoded[3..11]).into_owned()
            } else {
                String::new()
            },
            "exfat_boot_checksum_valid": exfat_boot_checksum(&decoded),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
