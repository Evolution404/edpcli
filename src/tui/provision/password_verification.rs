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
            }
            Some(ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt)) => {
                self.provision.form.encrypt_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
            }
            _ => {}
        }
    }

    pub fn provision_source_password_verify_request(
        &self,
    ) -> Result<Option<(crate::provision::KeyDomainRole, String)>, String> {
        let Some(descriptor) = self.provision_field_descriptor(self.provision.field_selected)
        else {
            return Ok(None);
        };
        if !descriptor.capabilities.verify_source_password {
            return Ok(None);
        }
        let id = descriptor.id;
        let (domain, password) = match id {
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Share) => (
                crate::provision::KeyDomainRole::Share,
                self.provision.form.share_source_password.as_str(),
            ),
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt) => (
                crate::provision::KeyDomainRole::Encrypt,
                self.provision.form.encrypt_source_password.as_str(),
            ),
            _ => return Ok(None),
        };
        if password.is_empty() {
            return Err("请先输入当前域来源密码，再按 v 验证。".into());
        }
        Ok(Some((domain, password.to_string())))
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
                let mut status = Vec::new();
                self.provision.form.share_opaque_profile = probe.share_opaque_profile;
                self.provision.form.encrypt_opaque_profile = probe.encrypt_opaque_profile;
                if let Some(knowledge) = probe.share {
                    if self.provision.form.share_source_password.is_empty() {
                        self.provision.form.share_source_knowledge = knowledge;
                        if knowledge == crate::provision::SourcePasswordKnowledge::DefaultVerified {
                            self.provision.form.share_source_password = "0000aaaa".into();
                        }
                    }
                    status.push(format!(
                        "交换域:{}",
                        match self.provision.form.share_source_knowledge {
                            crate::provision::SourcePasswordKnowledge::DefaultVerified => {
                                "默认密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::UserVerified => {
                                "用户密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::Unknown => "Unknown",
                        }
                    ));
                }
                if let Some(knowledge) = probe.encrypt {
                    if self.provision.form.encrypt_source_password.is_empty() {
                        self.provision.form.encrypt_source_knowledge = knowledge;
                        if knowledge == crate::provision::SourcePasswordKnowledge::DefaultVerified {
                            self.provision.form.encrypt_source_password = "0000aaaa".into();
                        }
                    }
                    status.push(format!(
                        "保密域:{}",
                        match self.provision.form.encrypt_source_knowledge {
                            crate::provision::SourcePasswordKnowledge::DefaultVerified => {
                                "默认密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::UserVerified => {
                                "用户密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::Unknown => "Unknown",
                        }
                    ));
                }
                self.provision.message = Some(if status.is_empty() {
                    format!(
                        "来源状态: {} · 无 EDP 用户密码域",
                        probe.source_kind.short_name()
                    )
                } else {
                    format!(
                        "来源状态: {} · {}",
                        probe.source_kind.short_name(),
                        status.join(" · ")
                    )
                });
                self.provision_sync_cursor_to_end();
            }
            Err(message) => {
                self.provision.message = Some(format!("来源密码域只读探测失败: {message}"));
            }
        }
    }

    pub fn provision_finish_source_password_verify(
        &mut self,
        domain: crate::provision::KeyDomainRole,
        result: Result<crate::provision::SourcePasswordKnowledge, String>,
    ) {
        if self.provision.stage != ProvisionStage::Form {
            return;
        }
        match (domain, result) {
            (crate::provision::KeyDomainRole::Share, Ok(knowledge)) => {
                self.provision.form.share_source_knowledge = knowledge;
                self.provision.message = Some("交换域来源密码验证通过。".into());
            }
            (crate::provision::KeyDomainRole::Encrypt, Ok(knowledge)) => {
                self.provision.form.encrypt_source_knowledge = knowledge;
                self.provision.message = Some("保密域来源密码验证通过。".into());
            }
            (crate::provision::KeyDomainRole::Share, Err(message)) => {
                self.provision.form.share_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.message = Some(message);
            }
            (crate::provision::KeyDomainRole::Encrypt, Err(message)) => {
                self.provision.form.encrypt_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.message = Some(message);
            }
        }
    }
}
