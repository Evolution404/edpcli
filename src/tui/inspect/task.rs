use super::*;

impl TaskHub {
    pub fn request_advanced_inspect(
        &mut self,
        source: crate::tui::state::AdvancedInspectSource,
        request: crate::application::inspect::AdvancedInspectRequest,
    ) -> Result<u64, &'static str> {
        if !self.advanced_inspect_single_flight.try_start() {
            return Err("已有高级检查正在执行");
        }
        let generation = self.advanced_inspect_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| match source {
                crate::tui::state::AdvancedInspectSource::Disk(disk) => {
                    let runner = SysRunner;
                    crate::application::inspect::load_disk_advanced_inspect(&runner, disk, &request)
                }
                crate::tui::state::AdvancedInspectSource::Backup(path) => {
                    crate::application::inspect::load_backup_advanced_inspect(&path, &request)
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "高级检查 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::AdvancedInspect { generation, result });
        });
        Ok(generation)
    }

    pub fn request_advanced_inspect_sector(
        &mut self,
        source: crate::tui::state::AdvancedInspectSource,
        lba: u64,
    ) -> Result<u64, &'static str> {
        self.request_advanced_inspect_sector_mode(
            source,
            lba,
            crate::application::inspect::AdvancedInspectMode::Decode,
        )
    }

    pub fn request_advanced_inspect_preview(
        &mut self,
        source: crate::tui::state::AdvancedInspectSource,
        lba: u64,
    ) -> Result<u64, &'static str> {
        self.request_advanced_inspect_sector_mode(
            source,
            lba,
            crate::application::inspect::AdvancedInspectMode::Meta,
        )
    }

    fn request_advanced_inspect_sector_mode(
        &mut self,
        source: crate::tui::state::AdvancedInspectSource,
        lba: u64,
        mode: crate::application::inspect::AdvancedInspectMode,
    ) -> Result<u64, &'static str> {
        if !self.advanced_inspect_sector_single_flight.try_start() {
            return Err("已有扇区读取正在执行");
        }
        let generation = self.advanced_inspect_sector_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let request = crate::application::inspect::AdvancedInspectRequest {
                mode,
                lbas: vec![lba],
                export_dir: None,
                device_id_override: None,
                fail_soft_decode: true,
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                let workspace = match source {
                    crate::tui::state::AdvancedInspectSource::Disk(disk) => {
                        let runner = SysRunner;
                        crate::application::inspect::load_disk_advanced_inspect(
                            &runner, disk, &request,
                        )
                    }
                    crate::tui::state::AdvancedInspectSource::Backup(path) => {
                        crate::application::inspect::load_backup_advanced_inspect(&path, &request)
                    }
                }?;
                workspace
                    .items
                    .into_iter()
                    .next()
                    .ok_or_else(|| format!("LBA{lba} Inspect 未返回扇区"))
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "扇区 Inspect worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::AdvancedInspectSector {
                generation,
                lba,
                result,
            });
        });
        Ok(generation)
    }
}
