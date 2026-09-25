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

                    // 写流程只允许经 write_event 上报；兜底丢弃，防止文本直接写进备用屏。
                    fn output(&mut self, _msg: &str) {}
                }

                let result = (|| -> Result<(), String> {
                    let runner = SysRunner;
                    crate::application::write::guard_usb_disk(&runner, intent.disk)
                        .map_err(|error| error.msg)?;
                    let path = crate::diskio::raw_path(intent.disk);
                    let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                        .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                    let expected = intent.expected_identity.as_ref();
                    crate::application::write::verify_expected_identity(
                        &runner,
                        intent.disk,
                        expected.and_then(|value| value.onlyid.as_deref()),
                        expected.and_then(|value| value.device_id.as_deref()),
                        &mut dev,
                    )
                    .map_err(|error| error.msg)?;
                    let mut prompt = ConfirmedPrompter {
                        tx: tx.clone(),
                        operation_id,
                    };
                    let mut ctx = crate::application::write::Ctx {
                        runner: &runner,
                        clock: &crate::diskio::SystemClock,
                        prompt: &mut prompt,
                        backup_dir,
                    };
                    match intent.kind {
                        crate::tui::state::WriteKind::Restore => {
                            let backup = intent
                                .backup
                                .as_ref()
                                .ok_or_else(|| "错误: restore 缺少固定备份路径".to_string())?;
                            crate::application::write::restore_flow(
                                Some(backup.to_string_lossy().into_owned()),
                                intent.disk,
                                &mut ctx,
                                &mut dev,
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
