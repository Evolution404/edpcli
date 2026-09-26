//! Validated producer/consumer filesystem limits owned by edpcli.

/// Maximum exFAT cluster count that the Deep parser will allocate and verify.
pub const EXFAT_MAX_VALIDATED_CLUSTERS: u32 = 4_194_304;

/// With 512-byte sectors, this caps clusters at 32 MiB, the largest that the
/// current directory-chain and exFAT geometry checks can safely consume.
pub const EXFAT_MAX_VALIDATED_CLUSTER_SHIFT: u8 = 16;
