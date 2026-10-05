use super::*;

impl AppState {
    pub(super) fn provision_initialize_password_candidates(&mut self, kind: ProvisionKind) {
        use crate::provision::{KeyDomainRole, SourcePasswordKnowledge};
        use password_verification::{TargetPasswordMode, TargetPasswordModeState};

        self.provision.session_id = next_form_session();
        self.provision.source_password_edit_dirty = false;
        self.provision.share_source_password_revision = 0;
        self.provision.encrypt_source_password_revision = 0;
        self.provision.target_password_modes = TargetPasswordModeState::default();
        self.provision.form.share_source_password.clear();
        self.provision.form.encrypt_source_password.clear();
        self.provision.form.share_target_password.clear();
        self.provision.form.encrypt_target_password.clear();
        self.provision.form.share_source_knowledge = SourcePasswordKnowledge::Unknown;
        self.provision.form.encrypt_source_knowledge = SourcePasswordKnowledge::Unknown;
        self.provision.share_source_verification = ProvisionPasswordVerificationState::Idle;
        self.provision.encrypt_source_verification = ProvisionPasswordVerificationState::Idle;

        let mode = kind.mode();
        let password = crate::domain::secret::SecretText::from(
            crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT,
        );
        let share_active = matches!(mode, Some(0 | 1 | 3));
        let encrypt_active = matches!(mode, Some(0..=2));
        let share_from_source =
            share_active && self.provision_source_has_password_domain(KeyDomainRole::Share);
        let encrypt_from_source =
            encrypt_active && self.provision_source_has_password_domain(KeyDomainRole::Encrypt);

        if share_active {
            if share_from_source {
                self.provision.form.share_source_password = password.clone();
                self.provision.share_source_verification =
                    ProvisionPasswordVerificationState::Verifying;
            } else {
                self.provision.form.share_target_password = password.clone();
                self.provision.target_password_modes.share = TargetPasswordMode::Explicit;
            }
        }
        if encrypt_active {
            if encrypt_from_source {
                self.provision.form.encrypt_source_password = password.clone();
                self.provision.encrypt_source_verification =
                    ProvisionPasswordVerificationState::Verifying;
            } else {
                self.provision.form.encrypt_target_password = password;
                self.provision.target_password_modes.encrypt = TargetPasswordMode::Explicit;
            }
        }
    }
}
