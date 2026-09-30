use super::*;

impl AppState {
    pub(super) fn provision_mark_source_password_unverified(
        &mut self,
        id: Option<ProvisionFieldId>,
    ) {
        match id {
            Some(ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Share)) => {
                self.provision.form.share_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.share_source_password_revision = self
                    .provision
                    .share_source_password_revision
                    .wrapping_add(1);
                self.provision.share_source_verification = ProvisionPasswordVerificationState::Idle;
                self.provision.source_password_edit_dirty = true;
            }
            Some(ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt)) => {
                self.provision.form.encrypt_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.encrypt_source_password_revision = self
                    .provision
                    .encrypt_source_password_revision
                    .wrapping_add(1);
                self.provision.encrypt_source_verification =
                    ProvisionPasswordVerificationState::Idle;
                self.provision.source_password_edit_dirty = true;
            }
            _ => {}
        }
    }
}
