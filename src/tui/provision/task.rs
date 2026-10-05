use super::*;

impl TaskHub {
    pub(crate) fn retain_provision_context(&mut self, state: &crate::tui::state::AppState) {
        self.retain_password_session(
            (state.workspace() == crate::tui::state::Workspace::Provision
                && state.provision().stage == crate::tui::state::ProvisionStage::Form)
                .then_some(state.provision().session_id),
        );
    }

    pub fn request_provision_key_probe(&mut self, disk: u32) -> Result<u64, &'static str> {
        let generation = self
            .provision
            .key_probe_slot
            .try_begin()
            .ok_or("已有来源密码域探测正在执行")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                crate::application::provision::probe_provision_key_domains_on_disk(&runner, disk)
                    .map_err(crate::application::error::OperationError::from)
            }))
            .unwrap_or_else(|_| Err("来源密码域探测后台任务异常终止".into()));
            let _ = tx.send(WorkerResult::Provision(ProvisionWorkerResult::KeyProbe {
                generation,
                result,
            }));
        });
        Ok(generation)
    }

    // Compatibility entry point; production supplies an explicit form session.
    pub fn request_provision_source_password_verify(
        &mut self,
        disk: u32,
        domain: crate::provision::KeyDomainRole,
        password: String,
        revision: u64,
    ) -> Result<u64, &'static str> {
        self.request_source_password_verify_session(disk, domain, password.into(), revision, 0)
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
                    &SysRunner,
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
                let runner = SysRunner;
                crate::application::provision::prepare_provision_on_disk(&runner, disk, &request)
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
        prepared: crate::tui::state::ProvisionPrepared,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        let progress = ProgressPublisher::new(self, operation_id, ProgressKind::Provision);
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                struct ProvisionWritePrompter;
                impl crate::application::write::Prompter for ProvisionWritePrompter {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
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
            .unwrap_or_else(|_| Err("制盘后台任务异常终止".into()));
            progress.flush();
            let _ = tx.send(WorkerResult::Provision(ProvisionWorkerResult::Write {
                operation_id,
                result,
            }));
        }));
        Ok(operation_id)
    }
}
