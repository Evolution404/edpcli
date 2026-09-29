use super::*;

impl TaskHub {
    pub fn request_write(
        &mut self,
        intent: crate::tui::state::WriteIntent,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        if intent.kind != crate::tui::state::WriteKind::Restore {
            return Err("request_write 仅接受 Restore；备份创建使用独立只读 worker");
        }
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

                let runner = SysRunner;
                let expected = intent
                    .expected_identity
                    .as_ref()
                    .ok_or_else(|| "错误: TUI 写操作缺少 typed 介质身份 pin".to_string())?;
                let backup = intent
                    .backup
                    .as_ref()
                    .ok_or_else(|| "错误: restore 缺少固定备份路径".to_string())?;
                let mut prompt = ConfirmedPrompter {
                    tx: tx.clone(),
                    operation_id,
                };
                crate::application::write::restore_on_disk_typed_with_pin(
                    &runner,
                    Some(backup.to_string_lossy().into_owned()),
                    intent.disk,
                    backup_dir,
                    &mut prompt,
                    expected,
                )
                .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|payload| {
                Err(format!("写盘 worker 异常终止: {}", panic_message(payload)))
            });
            let _ = tx.send(WorkerResult::Restore {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_post_restore_format(
        &mut self,
        intent: crate::tui::state::PostRestoreFormatIntent,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            struct ConfirmedPrompter;

            impl crate::application::Prompter for ConfirmedPrompter {
                fn prompt_line(&mut self, _msg: &str) -> String {
                    String::new()
                }

                fn confirm_yes(&mut self, _msg: &str) -> bool {
                    true
                }
            }

            let runner = SysRunner;
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut prompt = ConfirmedPrompter;
                crate::application::post_restore::format_partition_after_restore_on_disk(
                    &runner,
                    intent.disk,
                    &mut prompt,
                    &intent.outcome,
                    &intent.request,
                    &intent.volume_label,
                )
            }))
            .unwrap_or_else(|payload| {
                crate::application::post_restore::PostRestoreFormatResult {
                    partition_index: intent.request.partition_index,
                    filesystem: intent.request.filesystem,
                    result: Err(format!(
                        "格式化 worker 异常终止: {}",
                        panic_message(payload)
                    )),
                }
            });
            let _ = tx.send(WorkerResult::PostRestoreFormat {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }
    pub fn request_post_restore_encrypted_format(
        &mut self,
        intent: crate::tui::state::EncryptedPostRestoreFormatIntent,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            struct ConfirmedPrompter;

            impl crate::application::Prompter for ConfirmedPrompter {
                fn prompt_line(&mut self, _msg: &str) -> String {
                    String::new()
                }

                fn confirm_yes(&mut self, _msg: &str) -> bool {
                    true
                }
            }

            let runner = SysRunner;
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut prompt = ConfirmedPrompter;
                crate::application::post_restore::format_encrypted_partition_after_restore_on_disk(
                    &runner,
                    intent.disk,
                    &mut prompt,
                    &intent.outcome,
                    &intent.request,
                    intent.password.as_ref().map(|password| password.as_bytes()),
                    &intent.volume_label,
                )
            }))
            .unwrap_or_else(|payload| {
                crate::application::post_restore::EncryptedPostRestoreFormatResult {
                    partition_index: intent.request.partition_index,
                    filesystem: intent.request.filesystem,
                    result: Err(
                        crate::application::post_restore::EncryptedPostRestoreError::Operation(
                            format!("加密格式化 worker 异常终止: {}", panic_message(payload)),
                        ),
                    ),
                }
            });
            let _ = tx.send(WorkerResult::PostRestoreEncryptedFormat {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_post_restore_reinitialize(
        &mut self,
        intent: crate::tui::state::PostRestoreReinitializeIntent,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            struct ConfirmedPrompter;

            impl crate::application::Prompter for ConfirmedPrompter {
                fn prompt_line(&mut self, _msg: &str) -> String {
                    String::new()
                }

                fn confirm_yes(&mut self, _msg: &str) -> bool {
                    true
                }
            }

            let runner = SysRunner;
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut prompt = ConfirmedPrompter;
                crate::application::post_restore::reinitialize_encrypted_partition_after_restore_on_disk(
                    &runner,
                    intent.disk,
                    &mut prompt,
                    &intent.outcome,
                    &intent.request,
                    intent.filesystem,
                    &intent.volume_label,
                )
            }))
            .unwrap_or_else(|payload| {
                crate::application::post_restore::EncryptedPartitionReinitializeResult {
                    partition_index: intent.request.partition_index,
                    filesystem: intent.filesystem,
                    result: Err(format!(
                        "密钥域重建 worker 异常终止: {}",
                        panic_message(payload)
                    )),
                }
            });
            let _ = tx.send(WorkerResult::PostRestoreReinitialize {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }
}
