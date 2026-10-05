use super::*;

/// Publishes ordinary progress through a latest-value slot.
struct FormatPrompter {
    progress: ProgressPublisher,
}
impl crate::application::Prompter for FormatPrompter {
    fn prompt_line(&mut self, _msg: &str) -> String {
        String::new()
    }
    fn confirm_yes(&mut self, _msg: &str) -> bool {
        true
    }
    fn operation_progress(&mut self, event: crate::application::progress::ProgressEvent) {
        self.progress.publish(event);
    }
}

impl TaskHub {
    pub fn request_write(
        &mut self,
        intent: crate::tui::state::WriteIntent,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        if intent.kind != crate::tui::state::WriteKind::Restore {
            return Err("内部写入请求仅接受恢复操作；备份创建使用独立只读后台任务");
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
                let expected = intent.expected_identity.as_ref().ok_or_else(|| {
                    crate::application::error::OperationError::from("写操作缺少介质身份校验信息")
                        .in_phase("恢复")
                })?;
                let backup = intent.backup.as_ref().ok_or_else(|| {
                    crate::application::error::OperationError::from("恢复操作缺少固定备份路径")
                        .in_phase("恢复")
                })?;
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
                .map_err(crate::application::error::OperationError::from)
                .map_err(|error| error.in_phase("恢复"))
            }))
            .unwrap_or_else(|payload| {
                Err(crate::application::error::OperationError::from(format!(
                    "写盘后台任务异常终止: {}",
                    panic_message(payload)
                ))
                .in_phase("恢复后台任务"))
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
        let progress = ProgressPublisher::new(self, operation_id, ProgressKind::Format);
        self.critical_worker = Some(std::thread::spawn(move || {
            let runner = SysRunner;
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut prompt = FormatPrompter {
                    progress: progress.clone(),
                };
                crate::application::post_restore::format_partition_after_restore_on_disk_assessed(
                    &runner,
                    intent.disk,
                    &mut prompt,
                    &intent.outcome,
                    &intent.request,
                    &intent.volume_label,
                )
            }))
            .unwrap_or_else(|payload| {
                crate::application::post_restore::AssessedPostRestoreFormatResult {
                    assessment: None,
                    format: crate::application::post_restore::PostRestoreFormatResult {
                        partition_index: intent.request.partition_index,
                        filesystem: intent.request.filesystem,
                        result: Err(format!(
                            "格式化后台任务异常终止: {}",
                            panic_message(payload)
                        )),
                    },
                }
            });
            progress.flush();
            let _ = tx.send(WorkerResult::PostRestoreFormat {
                operation_id,
                assessment: result.assessment,
                result: result.format,
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
        let progress = ProgressPublisher::new(self, operation_id, ProgressKind::Format);
        self.critical_worker =
            Some(std::thread::spawn(move || {
                let runner = SysRunner;
                let result = catch_unwind(AssertUnwindSafe(|| {
                let mut prompt = FormatPrompter { progress: progress.clone() };
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
                            format!("加密格式化后台任务异常终止: {}", panic_message(payload)),
                        ),
                    ),
                }
            });
                progress.flush();
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
                        "密钥域重建后台任务异常终止: {}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::progress::{
        FormatStep, LogPolicy, OperationKind, Phase, ProgressEvent, Step, Unit, WorkProgress,
    };
    use crate::application::Prompter;

    #[test]
    fn format_forwarder_throttles_work_without_losing_phase_boundaries() {
        let hub = TaskHub::new();
        let mut prompt = FormatPrompter {
            progress: ProgressPublisher::new(&hub, OperationId(1), ProgressKind::Format),
        };
        let now = std::time::Instant::now();
        for current in 0..=1000 {
            let mut event = ProgressEvent::new(
                Phase::Format,
                Step::PostRestoreFormat(FormatStep::Write),
                5,
                10,
            )
            .with_work(WorkProgress::new(current, 1000, Unit::Sectors));
            event.operation = OperationKind::PostRestoreFormat;
            event.log_policy = LogPolicy::SnapshotOnly;
            event.emitted_at = now;
            prompt.operation_progress(event);
        }
        prompt.operation_progress(ProgressEvent::started(
            OperationKind::PostRestoreFormat,
            Phase::Format,
            Step::PostRestoreFormat(FormatStep::Sync),
            "sync",
        ));
        let events: Vec<_> = hub
            .rx
            .try_iter()
            .map(|message| match message {
                WorkerResult::PostRestoreFormatProgress { event, .. } => event,
                _ => panic!("unexpected message"),
            })
            .collect();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].work.unwrap().current, 0);
        assert_eq!(events[1].work.unwrap().current, 999);
        assert_eq!(events[2].work.unwrap().current, 1000);
        assert_eq!(events[3].step, Step::PostRestoreFormat(FormatStep::Sync));
        assert!(events[3].work.is_none());
    }
}
