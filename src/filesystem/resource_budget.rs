//! Payload budgets for the current materialized formatter. These are accounting
//! limits, not measurements of allocator overhead or process peak RSS.
use super::{FilesystemError, FilesystemErrorKind, FilesystemKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatResourceEstimate {
    pub metadata_sectors: u64,
    pub image_payload_bytes: u64,
    /// Eight payload copies cover retained images, construction intermediates,
    /// transaction copies and rollback snapshots in current application flows.
    pub working_payload_bytes: u64,
}
impl FormatResourceEstimate {
    pub fn from_sectors(metadata_sectors: u64) -> Result<Self, FilesystemError> {
        Self::from_native_sectors(metadata_sectors, 512)
    }

    /// Metadata count in complete native logical blocks, not 512B equivalents.
    pub fn from_native_sectors(
        metadata_sectors: u64,
        sector_bytes: u32,
    ) -> Result<Self, FilesystemError> {
        let image_payload_bytes = metadata_sectors
            .checked_mul(u64::from(sector_bytes))
            .ok_or_else(overflow)?;
        let working_payload_bytes = image_payload_bytes.checked_mul(8).ok_or_else(overflow)?;
        Ok(Self {
            metadata_sectors,
            image_payload_bytes,
            working_payload_bytes,
        })
    }
    pub fn checked_add(self, other: Self) -> Result<Self, FilesystemError> {
        Ok(Self {
            metadata_sectors: self
                .metadata_sectors
                .checked_add(other.metadata_sectors)
                .ok_or_else(overflow)?,
            image_payload_bytes: self
                .image_payload_bytes
                .checked_add(other.image_payload_bytes)
                .ok_or_else(overflow)?,
            working_payload_bytes: self
                .working_payload_bytes
                .checked_add(other.working_payload_bytes)
                .ok_or_else(overflow)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatResourceBudget {
    pub max_image_payload_bytes: u64,
    pub max_working_payload_bytes: u64,
}
impl Default for FormatResourceBudget {
    fn default() -> Self {
        Self {
            max_image_payload_bytes: 64 * 1024 * 1024,
            max_working_payload_bytes: 512 * 1024 * 1024,
        }
    }
}
impl FormatResourceBudget {
    pub fn check(self, estimate: FormatResourceEstimate) -> Result<(), FilesystemError> {
        if estimate.image_payload_bytes > self.max_image_payload_bytes
            || estimate.working_payload_bytes > self.max_working_payload_bytes
        {
            return Err(FilesystemError::new(
                FilesystemErrorKind::FormatBudgetExceeded,
                format!(
                    "格式化资源预算不足：镜像有效载荷 {} / {} B，工作有效载荷 {} / {} B",
                    estimate.image_payload_bytes,
                    self.max_image_payload_bytes,
                    estimate.working_payload_bytes,
                    self.max_working_payload_bytes,
                ),
            ));
        }
        Ok(())
    }
}
fn overflow() -> FilesystemError {
    FilesystemError::new(
        FilesystemErrorKind::FormatBudgetExceeded,
        "格式化资源估算溢出",
    )
}

/// Geometry-only estimate; runs before any volume-sized allocation.
pub fn estimate_format_resources(
    filesystem: FilesystemKind,
    volume_sectors: u64,
) -> Result<FormatResourceEstimate, FilesystemError> {
    let count = match filesystem {
        FilesystemKind::Fat16 => super::fat16::format_sector_count(volume_sectors)?,
        FilesystemKind::Fat32 => super::fat32::format_sector_count(volume_sectors)?,
        FilesystemKind::ExFat => super::exfat::format_sector_count(volume_sectors)?,
        _ => return Err(FilesystemError::format_unsupported(filesystem)),
    };
    FormatResourceEstimate::from_sectors(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn estimate_matches_materialized_write_sets_for_all_writable_filesystems() {
        for (filesystem, sectors) in [
            (FilesystemKind::Fat16, 32768),
            (FilesystemKind::Fat32, 262144),
            (FilesystemKind::ExFat, 32768),
        ] {
            let estimate = estimate_format_resources(filesystem, sectors).unwrap();
            let image =
                crate::filesystem::build_empty_filesystem_typed(filesystem, 2048, sectors, 1, None)
                    .unwrap();
            assert_eq!(estimate.metadata_sectors, image.sectors().len() as u64);
            assert_eq!(estimate.image_payload_bytes, image.metadata_bytes() as u64);
        }
    }
    #[test]
    fn builders_refuse_large_image_and_cumulative_plain_plan_before_materializing() {
        let error = crate::filesystem::build_empty_filesystem_typed(
            FilesystemKind::ExFat,
            2048,
            1u64 << 37,
            1,
            None,
        )
        .unwrap_err();
        assert_eq!(error.kind, FilesystemErrorKind::FormatBudgetExceeded);
        let count = 8 * 1024 * 1024 * 1024u64 / 512;
        let parts = (0..3)
            .map(|i| {
                crate::provision::PlainPartitionSpec::new(
                    2048 + i * count,
                    count,
                    FilesystemKind::Fat32,
                    "DATA",
                )
            })
            .collect();
        let plan = crate::provision::PlainProvisionPlan::new(2048 + 3 * count, parts).unwrap();
        let error = crate::provision::build_plain_provision_write_plan(&plan, None, &[1, 2, 3])
            .unwrap_err();
        assert!(error.contains("格式化资源预算不足"));
    }

    #[test]
    fn budget_rejects_overflow_and_cumulative_plans_at_exact_boundary() {
        assert!(FormatResourceEstimate::from_sectors(u64::MAX).is_err());
        let estimate = FormatResourceEstimate::from_sectors(10).unwrap();
        let budget = FormatResourceBudget {
            max_image_payload_bytes: 5120,
            max_working_payload_bytes: 40960,
        };
        budget.check(estimate).unwrap();
        assert!(budget
            .check(
                estimate
                    .checked_add(FormatResourceEstimate::from_sectors(1).unwrap())
                    .unwrap()
            )
            .is_err());
        assert!(estimate
            .checked_add(FormatResourceEstimate {
                metadata_sectors: u64::MAX,
                image_payload_bytes: 0,
                working_payload_bytes: 0
            })
            .is_err());
        let huge = estimate_format_resources(FilesystemKind::ExFat, 1u64 << 37).unwrap();
        assert!(FormatResourceBudget::default().check(huge).is_err());
    }
}
