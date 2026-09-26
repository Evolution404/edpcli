use super::*;

impl AppState {
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

    pub fn provision_set_planning(&mut self) {
        self.provision.stage = ProvisionStage::Planning;
        self.input_mode = InputMode::Normal;
        self.provision.message = Some("正在只读检查目标并生成精确制盘计划…".into());
    }

    pub fn provision_finish_plan(&mut self, result: Result<ProvisionPrepared, String>) {
        match result {
            Ok(prepared) => {
                if let ProvisionPrepared::Official(official) = &prepared {
                    if let Some(target_plan) = &official.target_plan {
                        for part in &target_plan.partitions {
                            match crate::provision::KeyDomainRole::from_partition_role(
                                part.geometry.role,
                            ) {
                                Some(crate::provision::KeyDomainRole::Share) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.share_source_knowledge = knowledge;
                                    }
                                }
                                Some(crate::provision::KeyDomainRole::Encrypt) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.encrypt_source_knowledge = knowledge;
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                }
                self.provision.prepared = Some(prepared);
                self.provision.stage = ProvisionStage::Review;
                self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_review();
                self.provision.message = None;
            }
            Err(message) => {
                self.provision.stage = ProvisionStage::Form;
                self.provision.message = Some(message);
            }
        }
    }
}
