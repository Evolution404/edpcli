use super::*;

impl AppState {
    pub(crate) fn provision_source_is_plain(&self) -> bool {
        self.selected_device()
            .map(|row| {
                row.confirmed_provision_kind().unwrap_or(row.provision_kind)
                    == crate::provision::DiskProvisionKind::Plain
            })
            .unwrap_or(false)
    }

    pub(crate) fn provision_source_has_password_domain(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> bool {
        self.selected_device()
            .and_then(|row| row.confirmed_provision_kind())
            .is_some_and(|kind| kind.has_key_domain(domain))
    }

    pub(crate) fn provision_source_password_not_applicable(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> bool {
        self.provision_source_password_state(domain)
            == password_verification::SourcePasswordState::NotApplicable
    }

    pub(crate) fn provision_target_password_mode(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> password_verification::TargetPasswordMode {
        match domain {
            crate::provision::KeyDomainRole::Share => self.provision.target_password_modes.share,
            crate::provision::KeyDomainRole::Encrypt => {
                self.provision.target_password_modes.encrypt
            }
        }
    }

    /// A password-only passthrough is valid only if its underlying file and
    /// data cipher mode is identical to the source region's LBA12 EncryptMode.
    pub(crate) fn provision_source_algorithm_matches_target(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> bool {
        let source = match domain {
            crate::provision::KeyDomainRole::Share => self.provision.form.share_source_algorithm,
            crate::provision::KeyDomainRole::Encrypt => {
                self.provision.form.encrypt_source_algorithm
            }
        };
        source == Some(self.provision.form.encryption_algorithm)
    }

    pub(crate) fn provision_target_password_is_passthrough(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> bool {
        self.provision_target_password_mode(domain)
            == password_verification::TargetPasswordMode::Passthrough
            && self.provision_source_algorithm_matches_target(domain)
    }

    pub(crate) fn provision_source_password_state(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> password_verification::SourcePasswordState {
        if !self.provision_source_has_password_domain(domain) {
            return password_verification::SourcePasswordState::NotApplicable;
        }
        use crate::provision::SourcePasswordKnowledge;
        use password_verification::SourcePasswordState;

        let (verification, knowledge) = match domain {
            crate::provision::KeyDomainRole::Share => (
                self.provision.share_source_verification,
                self.provision.form.share_source_knowledge,
            ),
            crate::provision::KeyDomainRole::Encrypt => (
                self.provision.encrypt_source_verification,
                self.provision.form.encrypt_source_knowledge,
            ),
        };
        match verification {
            ProvisionPasswordVerificationState::Verifying => SourcePasswordState::Verifying,
            ProvisionPasswordVerificationState::Failed => SourcePasswordState::Failed,
            ProvisionPasswordVerificationState::Idle => match knowledge {
                SourcePasswordKnowledge::DefaultVerified => SourcePasswordState::VerifiedDefault,
                SourcePasswordKnowledge::UserVerified => SourcePasswordState::VerifiedUser,
                SourcePasswordKnowledge::Unknown => SourcePasswordState::Unknown,
            },
        }
    }

    fn provision_set_target_password_mode(
        &mut self,
        domain: crate::provision::KeyDomainRole,
        mode: password_verification::TargetPasswordMode,
    ) {
        match domain {
            crate::provision::KeyDomainRole::Share => {
                self.provision.target_password_modes.share = mode;
            }
            crate::provision::KeyDomainRole::Encrypt => {
                self.provision.target_password_modes.encrypt = mode;
            }
        }
    }

    fn provision_target_password_draft_user_edited(
        &self,
        domain: crate::provision::KeyDomainRole,
    ) -> bool {
        match domain {
            crate::provision::KeyDomainRole::Share => {
                self.provision.target_password_modes.share_draft_user_edited
            }
            crate::provision::KeyDomainRole::Encrypt => {
                self.provision
                    .target_password_modes
                    .encrypt_draft_user_edited
            }
        }
    }

    pub(super) fn provision_note_target_password_user_edit(
        &mut self,
        domain: crate::provision::KeyDomainRole,
    ) {
        match domain {
            crate::provision::KeyDomainRole::Share => {
                self.provision.target_password_modes.share_draft_user_edited = true;
            }
            crate::provision::KeyDomainRole::Encrypt => {
                self.provision
                    .target_password_modes
                    .encrypt_draft_user_edited = true;
            }
        }
        self.provision_set_target_password_mode(
            domain,
            password_verification::TargetPasswordMode::Explicit,
        );
    }

    pub(super) fn provision_password_intent(
        &self,
        domain: crate::provision::KeyDomainRole,
        format_selected: bool,
    ) -> password_verification::PasswordIntent {
        let (source, target) = match domain {
            crate::provision::KeyDomainRole::Share => (
                self.provision.form.share_source_password.as_str(),
                self.provision.form.share_target_password.as_str(),
            ),
            crate::provision::KeyDomainRole::Encrypt => (
                self.provision.form.encrypt_source_password.as_str(),
                self.provision.form.encrypt_target_password.as_str(),
            ),
        };
        let intent = password_model::decide_password_intent(
            self.provision_source_password_state(domain),
            self.provision_target_password_mode(domain),
            source,
            target,
            format_selected,
        );
        if self.provision_source_has_password_domain(domain)
            && !self.provision_source_algorithm_matches_target(domain)
            && !format_selected
            && intent != password_verification::PasswordIntent::Waiting
        {
            return password_verification::PasswordIntent::BlockedNeedsFormat;
        }
        intent
    }

    pub(crate) fn provision_toggle_target_password_mode(
        &mut self,
        domain: crate::provision::KeyDomainRole,
    ) {
        if self.provision_source_password_state(domain)
            == password_verification::SourcePasswordState::NotApplicable
        {
            self.provision_prepare_target_password_edit(domain);
            return;
        }
        use password_verification::TargetPasswordMode;
        match self.provision_target_password_mode(domain) {
            TargetPasswordMode::Passthrough => self.provision_prepare_target_password_edit(domain),
            TargetPasswordMode::Explicit => {
                self.provision_set_target_password_mode(domain, TargetPasswordMode::Passthrough)
            }
        }
    }

    pub(super) fn provision_prepare_target_password_edit(
        &mut self,
        domain: crate::provision::KeyDomainRole,
    ) {
        use password_verification::TargetPasswordMode;

        if self.provision_target_password_mode(domain) == TargetPasswordMode::Explicit {
            return;
        }
        let source_verified = self.provision_source_password_state(domain).is_verified();
        let draft_user_edited = self.provision_target_password_draft_user_edited(domain);
        let (source, target) = match domain {
            crate::provision::KeyDomainRole::Share => (
                self.provision.form.share_source_password.clone(),
                &mut self.provision.form.share_target_password,
            ),
            crate::provision::KeyDomainRole::Encrypt => (
                self.provision.form.encrypt_source_password.clone(),
                &mut self.provision.form.encrypt_target_password,
            ),
        };
        if !draft_user_edited && source_verified {
            *target = source;
        } else if target.is_empty() {
            *target = crate::domain::secret::SecretText::from(
                crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT,
            );
        }
        self.provision_set_target_password_mode(domain, TargetPasswordMode::Explicit);
        self.provision_sync_cursor_to_end();
    }

    pub(super) fn provision_normalize_target_password_mode(
        &mut self,
        domain: crate::provision::KeyDomainRole,
    ) {
        use password_verification::TargetPasswordMode;

        if self.provision_source_password_state(domain)
            == password_verification::SourcePasswordState::NotApplicable
        {
            self.provision_set_target_password_mode(domain, TargetPasswordMode::Explicit);
            return;
        }
        if self.provision_target_password_mode(domain) != TargetPasswordMode::Explicit {
            return;
        }
        let (source, target) = match domain {
            crate::provision::KeyDomainRole::Share => (
                self.provision.form.share_source_password.as_str(),
                self.provision.form.share_target_password.as_str(),
            ),
            crate::provision::KeyDomainRole::Encrypt => (
                self.provision.form.encrypt_source_password.as_str(),
                self.provision.form.encrypt_target_password.as_str(),
            ),
        };
        if self.provision_source_password_state(domain).is_verified() && source == target {
            self.provision_set_target_password_mode(domain, TargetPasswordMode::Passthrough);
        }
    }
}
