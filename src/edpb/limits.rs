// Metadata-only containers have finite parsing and allocation budgets.
pub(crate) const MAX_CONTAINER_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_PAYLOAD_BYTES: u64 = 128 * 1024 * 1024;
pub(crate) const MAX_ARTIFACTS: usize = 1024;

pub(super) fn bounded_len(length: u64, maximum: u64, what: &str) -> Result<usize, String> {
    if length > maximum {
        return Err(format!("EDPB {what} exceeds read budget"));
    }
    usize::try_from(length).map_err(|_| format!("EDPB {what} length cannot fit this platform"))
}

/// The same payload budget is enforced before writer I/O and reader allocation.
pub(super) fn validate_payload_lengths(
    lengths: impl IntoIterator<Item = u64>,
) -> Result<(), String> {
    let mut count = 0;
    let mut total = 0u64;
    for length in lengths {
        count += 1;
        if count > MAX_ARTIFACTS {
            return Err("EDPB artifact count exceeds read budget".into());
        }
        bounded_len(length, MAX_ARTIFACT_BYTES, "artifact")?;
        total = total
            .checked_add(length)
            .filter(|v| *v <= MAX_PAYLOAD_BYTES)
            .ok_or("EDPB total payload exceeds read budget")?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writer_reader_payload_boundaries() {
        assert!(validate_payload_lengths([MAX_ARTIFACT_BYTES, MAX_ARTIFACT_BYTES]).is_ok());
        assert!(validate_payload_lengths([MAX_ARTIFACT_BYTES + 1]).is_err());
        assert!(validate_payload_lengths([MAX_ARTIFACT_BYTES, MAX_ARTIFACT_BYTES, 1]).is_err());
        assert!(validate_payload_lengths(std::iter::repeat_n(1, MAX_ARTIFACTS)).is_ok());
        assert!(validate_payload_lengths(std::iter::repeat_n(1, MAX_ARTIFACTS + 1)).is_err());
    }
}
