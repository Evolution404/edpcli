use super::*;

impl AppState {
    pub fn provision_set_planning(&mut self) {
        self.provision.review_projection = None;
        self.provision_transition_begin_planning();
    }

    pub fn provision_finish_plan(&mut self, result: Result<ProvisionPrepared, String>) {
        match result {
            Ok(prepared) => {
                let projection = match ProvisionConfirmationViewModel::from_prepared(&prepared) {
                    Ok(projection) => projection,
                    Err(message) => {
                        self.provision_transition_plan_failed(format!(
                            "错误: 计划确认投影失败: {message}"
                        ));
                        return;
                    }
                };
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
                self.provision.review_projection = Some(projection);
                self.provision.prepared = Some(prepared);
                self.provision_transition_plan_succeeded();
            }
            Err(message) => {
                self.provision.review_projection = None;
                self.provision_transition_plan_failed(message);
            }
        }
    }
}
