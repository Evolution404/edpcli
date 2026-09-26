use super::*;

impl TaskHub {
    pub fn request_provision_key_probe(&mut self, disk: u32) -> Result<u64, &'static str> {
        let generation = self
            .provision_key_probe_slot
            .try_begin()
            .ok_or("已有来源密码域探测正在执行")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                crate::application::provision::probe_provision_key_domains_on_disk(&runner, disk)
                    .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|_| Err("来源密码域探测 worker 异常终止".into()));
            let _ = tx.send(WorkerResult::ProvisionKeyProbe { generation, result });
        });
        Ok(generation)
    }

    pub fn request_provision_source_password_verify(
        &mut self,
        disk: u32,
        domain: crate::provision::KeyDomainRole,
        password: String,
    ) -> Result<u64, &'static str> {
        let generation = self
            .provision_key_probe_slot
            .try_begin()
            .ok_or("已有来源密码域探测/验证正在执行")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                crate::application::provision::verify_provision_source_password_on_disk(
                    &runner,
                    disk,
                    domain,
                    password.as_bytes(),
                )
                .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|_| Err("来源密码验证 worker 异常终止".into()));
            let _ = tx.send(WorkerResult::ProvisionKeyVerify {
                generation,
                domain,
                result,
            });
        });
        Ok(generation)
    }

    pub fn request_provision_plan(
        &mut self,
        disk: u32,
        request: crate::application::provision::ProvisionRequest,
    ) -> Result<u64, &'static str> {
        let generation = self
            .provision_slot
            .try_begin()
            .ok_or("已有制盘计划正在生成")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                crate::application::provision::prepare_provision_on_disk(&runner, disk, &request)
                    .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "制盘计划 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::ProvisionPlan { generation, result });
        });
        Ok(generation)
    }

    pub fn request_provision_export(
        &mut self,
        prepared: crate::application::provision::PreparedProvision,
        path: PathBuf,
    ) -> Result<u64, &'static str> {
        let generation = self
            .provision_export_slot
            .try_begin()
            .ok_or("已有制盘镜像正在导出")?;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::provision::export_provision_image(&path, &prepared)
                    .map_err(|error| error.msg)?;
                Ok(path)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "制盘镜像导出 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::ProvisionExport { generation, result });
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
        self.critical_worker =
            Some(std::thread::spawn(move || {
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
                        let _ = tx.send(WorkerResult::ProvisionProgress { operation_id, event });
                    },
                )
                .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|_| Err("制盘 worker 异常终止".into()));
                let _ = tx.send(WorkerResult::ProvisionWrite {
                    operation_id,
                    result,
                });
            }));
        Ok(operation_id)
    }
}
