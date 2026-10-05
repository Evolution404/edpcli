//! Filesystem format planning for official Provision targets.
use super::*;

fn build_plain_format_image(
    target: &PartitionFormatTarget,
    filesystem: FilesystemKind,
    volume_label: &str,
    volume_serial: u32,
) -> Result<SparseFilesystemImage, crate::filesystem::FilesystemError> {
    crate::filesystem::build_empty_filesystem_typed(
        filesystem,
        target.geometry.start_sector,
        target.geometry.sector_count(),
        volume_serial,
        Some(volume_label),
    )
}

pub fn plan_format_targets(
    plan: &OfficialProvisionPlan,
    options: &FormatOptions,
    serials: &[u32],
    file_key: &[u8; 16],
) -> Result<Vec<PlannedPartitionFormat>, String> {
    plan_format_targets_typed(plan, options, serials, file_key).map_err(|error| error.to_string())
}

pub fn plan_format_targets_typed(
    plan: &OfficialProvisionPlan,
    options: &FormatOptions,
    serials: &[u32],
    file_key: &[u8; 16],
) -> Result<Vec<PlannedPartitionFormat>, ProvisionPlanningError> {
    let keys = vec![*file_key; serials.len()];
    plan_format_targets_with_keys(plan, options, serials, &keys)
}

pub(super) fn plan_format_targets_with_keys(
    plan: &OfficialProvisionPlan,
    options: &FormatOptions,
    serials: &[u32],
    file_keys: &[[u8; 16]],
) -> Result<Vec<PlannedPartitionFormat>, ProvisionPlanningError> {
    let targets = plan
        .format_targets()
        .map_err(ProvisionPlanningError::Geometry)?;
    if targets.len() != serials.len() || targets.len() != file_keys.len() {
        return Err(ProvisionPlanningError::FormatTargetCountMismatch);
    }
    if options.boot
        && !targets
            .iter()
            .any(|t| t.format_capable && t.role == PartitionRole::Boot)
        || options.share
            && !targets.iter().any(|t| {
                t.format_capable
                    && matches!(
                        t.role,
                        PartitionRole::Share | PartitionRole::BootShareCombined
                    )
            })
        || options.encrypt
            && !targets
                .iter()
                .any(|t| t.format_capable && t.role == PartitionRole::Encrypt)
    {
        return Err(ProvisionPlanningError::MissingFormatRole);
    }
    let mut planned = targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| {
            let (selected, label) = options.choice(target.role);
            PlannedPartitionFormat {
                target,
                selected,
                filesystem: target.filesystem,
                volume_label: label.to_string(),
                volume_serial: serials[index],
                prepared_image: None,
                verification_image: None,
            }
        })
        .collect::<Vec<_>>();
    for choice in planned.iter().filter(|choice| choice.target.format_capable) {
        let filesystem = choice
            .filesystem
            .ok_or(ProvisionPlanningError::MissingFilesystem {
                role: choice.target.role,
            })?;
        crate::filesystem::validate_writable_filesystem(filesystem).map_err(|source| {
            ProvisionPlanningError::Filesystem {
                partition: None,
                source,
            }
        })?;
    }
    for (index, choice) in planned
        .iter_mut()
        .enumerate()
        .filter(|(_, choice)| choice.selected)
    {
        let filesystem = choice
            .filesystem
            .ok_or(ProvisionPlanningError::MissingFilesystem {
                role: choice.target.role,
            })?;
        let verification_image = build_plain_format_image(
            &choice.target,
            filesystem,
            &choice.volume_label,
            choice.volume_serial,
        )
        .map_err(|source| ProvisionPlanningError::Filesystem {
            partition: None,
            source,
        })?;
        let prepared_image = build_official_partition_filesystem(
            plan,
            &choice.target,
            &file_keys[index],
            &choice.volume_label,
            choice.volume_serial,
        )
        .map_err(|message| ProvisionPlanningError::FormatImage {
            role: choice.target.role,
            message,
        })?;
        if !choice.target.physically_encrypted && prepared_image.image != verification_image {
            return Err(ProvisionPlanningError::ImageMismatch {
                role: choice.target.role,
            });
        }
        choice.prepared_image = Some(prepared_image);
        choice.verification_image = Some(verification_image);
    }
    Ok(planned)
}
