//! Historical evidence keeps its original filename and bytes. Adapt only the
//! name passed to the runtime metadata parser; never load it as a runtime backup.
pub fn parse_gold_name(name: &str) -> Option<edpcli::diskio::BackupMeta> {
    let stem = name.strip_suffix(".bin")?;
    edpcli::diskio::parse_backup_name(&format!("{stem}.edpb"))
}
