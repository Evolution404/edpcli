use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TargetPasswordEditState {
    pub(super) share: bool,
    pub(super) encrypt: bool,
}

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

    pub fn provision_source_password_verify_request(
        &mut self,
    ) -> Result<Option<(crate::provision::KeyDomainRole, String, u64)>, String> {
        let Some(id) = self.provision_field_id(self.provision.field_selected) else {
            return Ok(None);
        };
        let (domain, password, revision) = match id {
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Share) => (
                crate::provision::KeyDomainRole::Share,
                self.provision.form.share_source_password.as_str(),
                self.provision.share_source_password_revision,
            ),
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt) => (
                crate::provision::KeyDomainRole::Encrypt,
                self.provision.form.encrypt_source_password.as_str(),
                self.provision.encrypt_source_password_revision,
            ),
            _ => return Ok(None),
        };
        if password.is_empty() {
            match domain {
                crate::provision::KeyDomainRole::Share => {
                    self.provision.share_source_verification =
                        ProvisionPasswordVerificationState::Failed;
                }
                crate::provision::KeyDomainRole::Encrypt => {
                    self.provision.encrypt_source_verification =
                        ProvisionPasswordVerificationState::Failed;
                }
            }
            return Err("原密码不能为空，无法执行只读验证。".into());
        }
        match domain {
            crate::provision::KeyDomainRole::Share => {
                self.provision.share_source_verification =
                    ProvisionPasswordVerificationState::Verifying;
            }
            crate::provision::KeyDomainRole::Encrypt => {
                self.provision.encrypt_source_verification =
                    ProvisionPasswordVerificationState::Verifying;
            }
        }
        Ok(Some((domain, password.to_string(), revision)))
    }

    pub fn provision_finish_key_probe(
        &mut self,
        result: Result<crate::application::provision::ProvisionKeyProbe, String>,
    ) {
        if self.provision.stage != ProvisionStage::Form {
            return;
        }
        match result {
            Ok(probe) => {
                self.provision.form.share_opaque_profile = probe.share_opaque_profile;
                self.provision.form.encrypt_opaque_profile = probe.encrypt_opaque_profile;
                if let Some(knowledge) = probe.share {
                    if self.provision.share_source_password_revision == 0
                        && self.provision.form.share_source_password.as_bytes()
                            == crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD
                    {
                        self.provision.form.share_source_knowledge = knowledge;
                        self.provision.share_source_verification =
                            if knowledge == crate::provision::SourcePasswordKnowledge::Unknown {
                                ProvisionPasswordVerificationState::Failed
                            } else {
                                ProvisionPasswordVerificationState::Idle
                            };
                    }
                } else if self.provision.share_source_password_revision == 0 {
                    self.provision.share_source_verification =
                        ProvisionPasswordVerificationState::Idle;
                }
                if let Some(knowledge) = probe.encrypt {
                    if self.provision.encrypt_source_password_revision == 0
                        && self.provision.form.encrypt_source_password.as_bytes()
                            == crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD
                    {
                        self.provision.form.encrypt_source_knowledge = knowledge;
                        self.provision.encrypt_source_verification =
                            if knowledge == crate::provision::SourcePasswordKnowledge::Unknown {
                                ProvisionPasswordVerificationState::Failed
                            } else {
                                ProvisionPasswordVerificationState::Idle
                            };
                    }
                } else if self.provision.encrypt_source_password_revision == 0 {
                    self.provision.encrypt_source_verification =
                        ProvisionPasswordVerificationState::Idle;
                }
                self.provision.message = None;
                self.provision_sync_cursor_to_end();
            }
            Err(message) => {
                if self.provision.share_source_verification
                    == ProvisionPasswordVerificationState::Verifying
                {
                    self.provision.share_source_verification =
                        ProvisionPasswordVerificationState::Idle;
                }
                if self.provision.encrypt_source_verification
                    == ProvisionPasswordVerificationState::Verifying
                {
                    self.provision.encrypt_source_verification =
                        ProvisionPasswordVerificationState::Idle;
                }
                self.set_notice(format!("来源密码域只读探测失败: {message}"));
            }
        }
    }

    pub fn provision_finish_source_password_verify(
        &mut self,
        domain: crate::provision::KeyDomainRole,
        revision: u64,
        result: Result<crate::provision::SourcePasswordKnowledge, String>,
    ) {
        if self.provision.stage != ProvisionStage::Form {
            return;
        }
        let current_revision = match domain {
            crate::provision::KeyDomainRole::Share => self.provision.share_source_password_revision,
            crate::provision::KeyDomainRole::Encrypt => {
                self.provision.encrypt_source_password_revision
            }
        };
        if revision != current_revision {
            return;
        }
        match (domain, result) {
            (crate::provision::KeyDomainRole::Share, Ok(knowledge)) => {
                self.provision.form.share_source_knowledge = knowledge;
                self.provision.share_source_verification = ProvisionPasswordVerificationState::Idle;
            }
            (crate::provision::KeyDomainRole::Encrypt, Ok(knowledge)) => {
                self.provision.form.encrypt_source_knowledge = knowledge;
                self.provision.encrypt_source_verification =
                    ProvisionPasswordVerificationState::Idle;
            }
            (crate::provision::KeyDomainRole::Share, Err(message)) => {
                self.provision.form.share_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.share_source_verification =
                    ProvisionPasswordVerificationState::Failed;
                self.set_notice(format!(
                    "{message}；未自动启用格式化。可保持兼容布局透传；如需改密请主动勾选交换区格式化"
                ));
            }
            (crate::provision::KeyDomainRole::Encrypt, Err(message)) => {
                self.provision.form.encrypt_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.encrypt_source_verification =
                    ProvisionPasswordVerificationState::Failed;
                self.set_notice(format!(
                    "{message}；未自动启用格式化。可保持兼容布局透传；如需改密请主动勾选保密区格式化"
                ));
            }
        }
    }
}
