use super::*;

impl TaskHub {
    pub fn request_write(
        &mut self,
        intent: crate::tui::state::WriteIntent,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                struct ConfirmedPrompter {
                    tx: Sender<WorkerResult>,
                    operation_id: OperationId,
                }
                impl crate::application::write::Prompter for ConfirmedPrompter {
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

                let result = (|| -> Result<(), String> {
                    let runner = SysRunner;
                    let expected = intent
                        .expected_identity
                        .as_ref()
                        .ok_or_else(|| "错误: TUI 写操作缺少 typed 介质身份 pin".to_string())?;
                    let mut prompt = ConfirmedPrompter {
                        tx: tx.clone(),
                        operation_id,
                    };
                    match intent.kind {
                        crate::tui::state::WriteKind::Restore => {
                            let backup = intent
                                .backup
                                .as_ref()
                                .ok_or_else(|| "错误: restore 缺少固定备份路径".to_string())?;
                            crate::application::write::restore_on_disk_with_pin(
                                &runner,
                                Some(backup.to_string_lossy().into_owned()),
                                intent.disk,
                                backup_dir,
                                &mut prompt,
                                expected,
                            )
                            .map(|_| ())
                            .map_err(|error| error.msg)
                        }
                        crate::tui::state::WriteKind::BackupCreate => {
                            Err("错误: backup create 必须走只读备份 worker".to_string())
                        }
                        crate::tui::state::WriteKind::BackupCreateDeep => {
                            Err("错误: deep backup 必须走只读备份 worker".to_string())
                        }
                    }
                })();
                result
            }))
            .unwrap_or_else(|payload| {
                Err(format!("写盘 worker 异常终止: {}", panic_message(payload)))
            });
            let _ = tx.send(WorkerResult::Write {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }
}
