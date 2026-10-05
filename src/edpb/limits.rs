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
