use super::*;

impl TaskHub {
    pub fn request_backup_verify(&mut self, path: PathBuf, backup_dir: PathBuf) -> u64 {
        let generation = self.verify_generation.begin();
        if !self.verify_single_flight.try_start() {
            self.pending_verify = Some((generation, path, backup_dir));
            return generation;
        }
        self.start_backup_verify(generation, path, backup_dir);
        generation
    }

    pub(super) fn start_backup_verify(
        &mut self,
        generation: u64,
        path: PathBuf,
        backup_dir: PathBuf,
    ) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::verify_backup_exact(&backup_dir, &path)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份校验 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupVerify {
                generation,
                path,
                result,
            });
        });
    }

    pub fn request_backup_delete(
        &mut self,
        path: PathBuf,
        expected_sha256: String,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::delete_backup_exact(&backup_dir, &path, &expected_sha256)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份删除 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupDelete {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_backup_batch_delete_plan(
        &mut self,
        targets: Vec<(PathBuf, String)>,
        backup_dir: PathBuf,
    ) -> Result<u64, &'static str> {
        if !self.batch_delete_single_flight.try_start() {
            return Err("已有批量删除计划正在生成");
        }
        let generation = self.batch_delete_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                session
                    .plan_exact_many(&targets)
                    .map_err(|error| error.message())
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "批量删除计划 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupBatchDeletePlan { generation, result });
        });
        Ok(generation)
    }

    pub fn request_backup_batch_delete_execute(
        &mut self,
        plan: crate::application::backup::DeletePlan,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                let expected = plan.targets.len();
                let results = session.execute(&plan);
                let failures = results
                    .iter()
                    .filter_map(|(path, result)| {
                        result
                            .as_ref()
                            .err()
                            .map(|message| format!("{}: {message}", path.display()))
                    })
                    .collect::<Vec<_>>();
                if failures.is_empty() {
                    Ok(expected)
                } else {
                    Err(format!(
                        "批量删除未完全成功：{} / {} 项失败；未通过复核的文件未删除。{}",
                        failures.len(),
                        expected,
                        failures.join("；")
                    ))
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "批量删除 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupBatchDeleteExecute {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_backup_prune_plan(
        &mut self,
        backup_dir: PathBuf,
        keep: usize,
    ) -> Result<u64, &'static str> {
        if !self.prune_single_flight.try_start() {
            return Err("已有备份清理计划正在生成");
        }
        let generation = self.prune_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                let plan = session.plan_prune(keep).map_err(|error| error.message())?;
                let (originals, retained_snapshots) = plan
                    .prune_stats
                    .as_ref()
                    .map(|stats| (stats.originals, stats.retained_snapshots))
                    .unwrap_or((0, 0));
                Ok(crate::tui::state::BackupPrunePrepared {
                    plan,
                    keep,
                    originals,
                    retained_snapshots,
                })
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份清理计划 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupPrunePlan { generation, result });
        });
        Ok(generation)
    }

    pub fn request_backup_prune_execute(
        &mut self,
        prepared: crate::tui::state::BackupPrunePrepared,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                let expected = prepared.plan.targets.len();
                let results = session.execute(&prepared.plan);
                let failures = results
                    .iter()
                    .filter_map(|(path, result)| {
                        result
                            .as_ref()
                            .err()
                            .map(|message| format!("{}: {message}", path.display()))
                    })
                    .collect::<Vec<_>>();
                if failures.is_empty() {
                    Ok(expected)
                } else {
                    Err(format!(
                        "清理未完全成功：{} / {} 项失败；未通过摘要复核的文件未删除。{}",
                        failures.len(),
                        expected,
                        failures.join("；")
                    ))
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份清理 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupPruneExecute {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_backup_create(
        &mut self,
        intent: crate::tui::state::WriteIntent,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let disk = intent.disk;
                struct BackupPrompter {
                    tx: Sender<WorkerResult>,
                    operation_id: OperationId,
                }

                impl crate::application::write::Prompter for BackupPrompter {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
                    }

                    fn confirm_yes(&mut self, _msg: &str) -> bool {
                        true
                    }

                    fn write_event(&mut self, event: crate::application::WriteEvent) {
                        let _ = self.tx.send(WorkerResult::WriteProgress {
                            operation_id: self.operation_id,
                            event,
                        });
                    }
                }

                let runner = SysRunner;
                crate::application::write::guard_usb_disk(&runner, disk)
                    .map_err(|error| error.msg)?;
                let path = crate::diskio::raw_path(disk);
                let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                    .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                let expected = intent.expected_identity.as_ref();
                crate::application::write::verify_expected_identity(
                    &runner,
                    disk,
                    expected.and_then(|value| value.onlyid.as_deref()),
                    expected.and_then(|value| value.device_id.as_deref()),
                    &mut dev,
                )
                .map_err(|error| error.msg)?;
                let mut prompt = BackupPrompter {
                    tx: tx.clone(),
                    operation_id,
                };
                let mut ctx = crate::application::write::Ctx {
                    runner: &runner,
                    clock: &crate::diskio::SystemClock,
                    prompt: &mut prompt,
                    backup_dir,
                };
                let deep = intent.kind == crate::tui::state::WriteKind::BackupCreateDeep;
                if deep {
                    crate::application::write::backup_create_level_flow(
                        disk, &mut ctx, &mut dev, true,
                    )
                } else {
                    crate::application::write::backup_create_flow(disk, &mut ctx, &mut dev)
                }
                .map(|_| ())
                .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|payload| {
                Err(format!("备份 worker 异常终止: {}", panic_message(payload)))
            });
            let _ = tx.send(WorkerResult::Write {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }
}
