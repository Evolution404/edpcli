use super::*;

impl TaskHub {
    pub(crate) fn retain_provision_context(&mut self, state: &crate::tui::state::AppState) {
        let in_form = state.workspace() == crate::tui::state::Workspace::Provision
            && state.provision().stage == crate::tui::state::ProvisionStage::Form;
        self.retain_key_probe_context(in_form.then(|| state.selected_device_disk()).flatten().map(
            |disk| KeyProbeContext {
                disk,
                session_id: state.provision().session_id,
            },
        ));
        self.retain_password_session(in_form.then_some(state.provision().session_id));
    }

    pub(super) fn retain_key_probe_context(&mut self, context: Option<KeyProbeContext>) {
        if self.provision.key_probe_context != context {
            self.provision.key_probe_context = context;
            self.provision.key_probe_slot.invalidate_pending();
        }
    }

    pub(crate) fn request_key_probe_session(
        &mut self,
        context: KeyProbeContext,
    ) -> Result<u64, &'static str> {
        self.retain_key_probe_context(Some(context));
        let generation = match self.provision.key_probe_slot.request_latest(context) {
            LatestRequest::Started {
                generation,
                request,
            } => {
                self.start_key_probe(generation, request);
                generation
            }
            LatestRequest::Queued { generation } => generation,
        };
        Ok(generation)
    }

    pub(super) fn start_key_probe(&mut self, generation: u64, context: KeyProbeContext) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = system_runner();
                crate::application::provision::probe_provision_key_domains_on_disk(
                    &runner,
                    context.disk,
                )
                .map_err(crate::application::error::OperationError::from)
            }))
            .unwrap_or_else(|_| Err("来源密码域探测后台任务异常终止".into()));
            let _ = tx.send(WorkerResult::Provision(ProvisionWorkerResult::KeyProbe {
                generation,
                context,
                result,
            }));
        });
    }

    pub(crate) fn request_source_password_verify_session(
        &mut self,
        disk: u32,
        domain: crate::provision::KeyDomainRole,
        password: crate::domain::secret::SecretText,
        revision: u64,
        session_id: u64,
    ) -> Result<u64, &'static str> {
        self.retain_password_session(Some(session_id));
        let request = PasswordVerifyRequest {
            disk,
            domain,
            password,
            revision,
            session_id,
        };
        match self.provision.password_verify_slots[password_domain_index(domain)]
            .request_latest(request)
        {
            LatestRequest::Started {
                generation,
                request,
            } => self.start_source_password_verify(generation, request),
            LatestRequest::Queued { .. } => {}
        }
        Ok(revision)
    }

    pub(super) fn start_source_password_verify(
        &mut self,
        generation: u64,
        request: PasswordVerifyRequest,
    ) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let PasswordVerifyRequest {
                disk,
                domain,
                password,
                revision,
                session_id,
            } = request;
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::provision::verify_provision_source_password_on_disk(
                    &system_runner(),
                    disk,
                    domain,
                    password.as_bytes(),
                )
                .map_err(crate::application::error::OperationError::from)
            }))
            .unwrap_or_else(|_| Err("来源密码验证后台任务异常终止".into()));
            drop(password);
            let _ = tx.send(WorkerResult::Provision(ProvisionWorkerResult::KeyVerify {
                generation,
                session_id,
                revision,
                domain,
                result,
            }));
        });
    }

    pub fn request_provision_plan(
        &mut self,
        disk: u32,
        request: crate::application::provision::ProvisionRequest,
    ) -> Result<u64, &'static str> {
        let generation = self
            .provision
            .plan_slot
            .try_begin()
            .ok_or("已有制盘计划正在生成")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = system_runner();
                crate::application::provision::native_flow::prepare_native_provision_on_disk(
                    &runner, disk, &request,
                )
                .map(|native| {
                    crate::application::provision::PreparedProvision::Native(Box::new(native))
                })
                .map_err(crate::application::error::OperationError::from)
            }))
            .unwrap_or_else(|payload| {
                Err(format!("制盘计划后台任务异常终止: {}", panic_message(payload)).into())
            });
            let _ = tx.send(WorkerResult::Provision(ProvisionWorkerResult::Plan {
                generation,
                result,
            }));
        });
        Ok(generation)
    }

    pub fn request_native_mode1_readonly_plan(
        &mut self,
        disk: u32,
        backup: PathBuf,
    ) -> Result<u64, &'static str> {
        let generation = self
            .provision
            .plan_slot
            .try_begin()
            .ok_or("已有制盘计划正在生成")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = system_runner();
                crate::application::provision::native_preflight::preflight_native_mode1_on_disk(
                    &runner, disk, &backup,
                )
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "4Kn只读计划任务异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::Provision(
                ProvisionWorkerResult::NativeReadOnlyPlan { generation, result },
            ));
        });
        Ok(generation)
    }

    pub fn request_provision_export(
        &mut self,
        prepared: crate::application::provision::PreparedProvision,
        path: PathBuf,
    ) -> Result<u64, &'static str> {
        let generation = self
            .provision
            .export_slot
            .try_begin()
            .ok_or("已有制盘镜像正在导出")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::provision::export_provision_image(&path, &prepared)
                    .map_err(crate::application::error::OperationError::from)?;
                Ok(path)
            }))
            .unwrap_or_else(|payload| {
                Err(format!("制盘镜像导出后台任务异常终止: {}", panic_message(payload)).into())
            });
            let _ = tx.send(WorkerResult::Provision(ProvisionWorkerResult::Export {
                generation,
                result,
            }));
        });
        Ok(generation)
    }

    pub fn request_provision_write(
        &mut self,
        prepared: crate::application::provision::PreparedProvision,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        let progress = ProgressPublisher::new(self, operation_id, ProgressKind::Provision);
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = system_runner();
                struct ProvisionWritePrompter;
                impl crate::application::write::Prompter for ProvisionWritePrompter {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
                    }
                    fn prompt_secret(&mut self, _msg: &str) -> crate::provision::SecretBytes {
                        // Secrets arrive through the prepared UI intent; workers cannot prompt.
                        crate::provision::SecretBytes::default()
                    }

                    fn confirm_yes(&mut self, _msg: &str) -> bool {
                        true
                    }
                }
                let mut prompt = ProvisionWritePrompter;
                crate::application::provision::commit_provision_with_backup_on_disk_with_progress(
                    &runner,
                    &prepared,
                    backup_dir,
                    &mut prompt,
                    &mut |event| {
                        progress.publish(event);
                    },
                )
                .map_err(crate::application::error::OperationError::from)
            }))
            .unwrap_or_else(|_| {
                Err(
                    crate::application::error::OperationError::from("制盘后台任务异常终止")
                        .with_media_state(crate::application::error::MediaState::Unknown),
                )
            });
            progress.flush();
            let _ = tx.send(WorkerResult::Provision(ProvisionWorkerResult::Write {
                operation_id,
                result,
            }));
        }));
        Ok(operation_id)
    }
}
