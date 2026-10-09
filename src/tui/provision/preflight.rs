use super::*;
pub(crate) use crate::application::provision::preflight::{
    ProvisionFormatDisposition, ProvisionPreflight, ProvisionPreflightKind,
};

impl AppState {
    pub(crate) fn provision_preflight(&self) -> Result<ProvisionPreflight, String> {
        use crate::application::provision::preflight::{
            PasswordDomainPreflight, ProvisionPreflightInput,
        };
        use crate::provision::{KeyDomainRole, PartitionRole};
        let (resolved, source) = self.provision_resolved_prefill()?;
        let parts = resolved.draft_partitions(crate::common::SECTOR as u64)?;
        let extents = self
            .selected_device()
            .and_then(|row| row.partition_table.as_ref())
            .map(|table| {
                table
                    .partitions
                    .iter()
                    .map(|part| crate::provision::PlainSourceExtent {
                        start_lba: part.start_lba,
                        sector_count: part.sector_count,
                        filesystem: part
                            .filesystem
                            .as_deref()
                            .and_then(crate::filesystem::FilesystemKind::from_config_token),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let format_roles = [
            PartitionRole::Boot,
            PartitionRole::Share,
            PartitionRole::BootShareCombined,
            PartitionRole::Encrypt,
        ]
        .into_iter()
        .filter(|role| self.provision_explicit_format_selected(*role))
        .collect::<Vec<_>>();
        let password_domains =
            [KeyDomainRole::Share, KeyDomainRole::Encrypt].map(|domain| PasswordDomainPreflight {
                source_state: self.provision_source_password_state(domain),
                algorithm_compatible: self.provision_source_algorithm_matches_target(domain),
                preserve_intent: self.provision_password_intent(domain, false),
                format_intent: self.provision_password_intent(domain, true),
                opaque_profile: match domain {
                    KeyDomainRole::Share => self.provision.form.share_opaque_profile,
                    KeyDomainRole::Encrypt => self.provision.form.encrypt_opaque_profile,
                },
            });
        Ok(ProvisionPreflightInput {
            source: source.as_ref(),
            source_is_plain: self.provision_source_is_plain(),
            plain_extents: &extents,
            password_domains,
            format_roles: &format_roles,
        }
        .evaluate(&parts, resolved.usable_end_lba))
    }
    pub(super) fn provision_explicit_format_selected(
        &self,
        role: crate::provision::PartitionRole,
    ) -> bool {
        use crate::provision::PartitionRole;
        match role {
            PartitionRole::Boot => self.provision.form.format_boot,
            PartitionRole::Share | PartitionRole::BootShareCombined => {
                self.provision.form.format_share
            }
            PartitionRole::Encrypt => self.provision.form.format_encrypt,
            PartitionRole::CompatibilityReserve => false,
        }
    }
}
