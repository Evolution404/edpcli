#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("real_usb_rollback_hil 目前只支持 macOS");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::env;
    use std::io;
    use std::process::Command;
    use std::time::Duration;

    use edpcli::application::device::guard_usb_disk;
    use edpcli::backup_metadata::parse_partition_geometry;
    use edpcli::common::{EXIT_ROLLED_BACK, SECTOR};
    use edpcli::diskio::{
        execute_write_transaction, raw_path, FileDev, SectorDev, SectorWriteStage,
        WriteTransactionPlan,
    };
    use edpcli::identify::{generate_candidates, identify};
    use edpcli::sysinfo::{disk_total_sectors, CmdRunner, SysRunner};

    struct FailOnceDev {
        inner: FileDev,
        writes: usize,
        fail_on: usize,
        failed: bool,
    }

    impl SectorDev for FailOnceDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            self.inner.read_sector(lba)
        }

        fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
            self.writes += 1;
            if !self.failed && self.writes == self.fail_on {
                self.failed = true;
                return Err(io::Error::other(format!(
                    "real USB HIL injected failure before write #{}, LBA{}",
                    self.writes, lba
                )));
            }
            self.inner.write_sector(lba, data)
        }

        fn sync(&mut self) -> io::Result<()> {
            self.inner.sync()
        }

        fn reopen_rdwr(&mut self, wait: Duration) -> io::Result<()> {
            self.inner.reopen_rdwr(wait)
        }
    }

    fn parse_args() -> Result<(u32, u32), String> {
        let mut disk = None;
        let mut base_lba = 1024u32;
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--disk" => {
                    disk = Some(
                        args.next()
                            .ok_or_else(|| "--disk 缺少编号".to_string())?
                            .parse()
                            .map_err(|_| "--disk 必须是整数".to_string())?,
                    );
                }
                "--base-lba" => {
                    base_lba = args
                        .next()
                        .ok_or_else(|| "--base-lba 缺少数值".to_string())?
                        .parse()
                        .map_err(|_| "--base-lba 必须是整数".to_string())?;
                }
                other => return Err(format!("未知参数: {other}")),
            }
        }
        Ok((
            disk.ok_or_else(|| "必须显式指定 --disk N".to_string())?,
            base_lba,
        ))
    }

    fn read_metadata(dev: &mut dyn SectorDev) -> Result<Vec<u8>, String> {
        let mut image = Vec::with_capacity(13 * SECTOR);
        for lba in 0..13u32 {
            image.extend(
                dev.read_sector(lba)
                    .map_err(|error| format!("读取 LBA{lba} 失败: {error}"))?,
            );
        }
        Ok(image)
    }

    fn mbr_partition_ranges(metadata: &[u8]) -> Result<Vec<(u64, u64)>, String> {
        let mbr = metadata
            .get(..SECTOR)
            .ok_or_else(|| "LBA0 长度不足，无法检查 MBR 分区范围".to_string())?;
        if mbr[510..512] != [0x55, 0xaa] {
            return Err("LBA0 缺少 0x55AA，无法安全判定 MBR 未分配区".into());
        }
        let mut ranges = Vec::new();
        for index in 0..4usize {
            let offset = 0x1be + index * 16;
            let entry = &mbr[offset..offset + 16];
            let partition_type = entry[4];
            let start = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as u64;
            let count = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as u64;
            if partition_type == 0 || count == 0 {
                continue;
            }
            let end = start
                .checked_add(count)
                .ok_or_else(|| format!("MBR P{} 范围溢出", index + 1))?;
            ranges.push((start, end));
        }
        Ok(ranges)
    }

    pub fn run() -> Result<(), String> {
        if env::var("EDPCLI_REAL_USB_ROLLBACK_HIL").ok().as_deref() != Some("YES") {
            return Err("未启用真实盘 HIL；必须显式设置 EDPCLI_REAL_USB_ROLLBACK_HIL=YES".into());
        }

        let (disk, base_lba) = parse_args()?;
        let touched = [base_lba, base_lba + 1, base_lba + 2];
        if base_lba <= 12 {
            return Err("故障注入扇区必须位于协议 LBA0-12 之外".into());
        }

        let runner = SysRunner;
        guard_usb_disk(&runner, disk).map_err(|error| error.msg)?;
        let total_before = disk_total_sectors(&runner, disk)
            .ok_or_else(|| format!("无法读取 disk{disk} 总扇区数"))?;
        if touched[2] as u64 >= total_before.saturating_sub(2048) {
            return Err("故障注入扇区不得进入盘尾 2048-sector 保护区".into());
        }
        let serial_before = runner
            .hardware_serial(disk)
            .ok_or_else(|| format!("无法读取 disk{disk} 硬件序列号"))?;

        let raw = raw_path(disk);
        let mut readonly =
            FileDev::open_rdonly(&raw).map_err(|error| format!("只读打开 {raw} 失败: {error}"))?;
        let metadata = read_metadata(&mut readonly)?;
        let edp_device_id = identify(&runner, disk, &metadata[7 * SECTOR..8 * SECTOR]).device_id;
        let device_id = edp_device_id
            .clone()
            .or_else(|| generate_candidates(&runner, disk).into_iter().next())
            .ok_or_else(|| "无法从 EDPF 或 USB/SCSI 硬件信息取得 device_id".to_string())?;
        let lba4_tag = edpcli::diskio::lba4_tag16_from(&metadata[4 * SECTOR..5 * SECTOR])
            .ok_or_else(|| "LBA4 缺少身份标签范围".to_string())?;
        let partitions = match parse_partition_geometry(&metadata, &device_id, total_before) {
            Ok(partitions) => partitions,
            Err(error) if lba4_tag.iter().any(|&byte| byte != 0) => {
                return Err(format!(
                    "当前盘仍有 EDP LBA4 身份但协议分区几何解析失败，拒绝把未知区域当未分配区: {error}"
                ));
            }
            Err(_) => Vec::new(),
        };
        let mbr_ranges = mbr_partition_ranges(&metadata)?;
        for &lba in &touched {
            if let Some(partition) = partitions.iter().find(|partition| {
                let start = partition.start_sector;
                let end = start + partition.sector_count;
                (lba as u64) >= start && (lba as u64) < end
            }) {
                return Err(format!(
                    "LBA{lba} 落在 EDP 活动分区 index={} type{} {}..{} 内，拒绝故障注入",
                    partition.index,
                    partition.partition_type,
                    partition.start_sector,
                    partition.start_sector + partition.sector_count - 1
                ));
            }
            if let Some((start, end)) = mbr_ranges
                .iter()
                .find(|(start, end)| (lba as u64) >= *start && (lba as u64) < *end)
            {
                return Err(format!(
                    "LBA{lba} 落在 MBR 活动分区 {}..{} 内，拒绝故障注入",
                    start,
                    end - 1
                ));
            }
        }
        let before: Vec<Vec<u8>> = touched
            .iter()
            .map(|&lba| {
                readonly
                    .read_sector(lba)
                    .map_err(|error| format!("读取故障注入 LBA{lba} 失败: {error}"))
            })
            .collect::<Result<_, _>>()?;
        if before
            .iter()
            .any(|sector| sector.iter().any(|&byte| byte != 0))
        {
            return Err("故障注入仅允许使用全零未分配扇区；目标扇区存在原始数据，拒绝执行".into());
        }
        drop(readonly);

        let disk_name = format!("disk{disk}");
        let status = Command::new("/usr/sbin/diskutil")
            .args(["unmountDisk", "force", &disk_name])
            .status()
            .map_err(|error| format!("无法启动 diskutil unmountDisk: {error}"))?;
        if !status.success() {
            return Err(format!("无法卸载 {disk_name}，拒绝真实写入 HIL"));
        }

        guard_usb_disk(&runner, disk).map_err(|error| error.msg)?;
        let total_after = disk_total_sectors(&runner, disk)
            .ok_or_else(|| format!("卸载后无法读取 disk{disk} 总扇区数"))?;
        let serial_after = runner
            .hardware_serial(disk)
            .ok_or_else(|| format!("卸载后无法读取 disk{disk} 硬件序列号"))?;
        if total_after != total_before || serial_after != serial_before {
            return Err("卸载后硬件身份/容量发生变化，拒绝继续".into());
        }

        let mut inner = FileDev::open_rdwr(&raw, Duration::from_secs(10))
            .map_err(|error| format!("读写打开 {raw} 失败: {error}"))?;
        let reopened_lba7 = inner
            .read_sector(7)
            .map_err(|error| format!("重开后读取 LBA7 失败: {error}"))?;
        if edp_device_id.is_some() {
            let reopened_id = identify(&runner, disk, &reopened_lba7).device_id;
            if reopened_id.as_deref() != Some(device_id.as_str()) {
                return Err(format!(
                    "重开后 EDP device_id 不一致: before={device_id}, after={}",
                    reopened_id.as_deref().unwrap_or("<none>")
                ));
            }
        } else if !generate_candidates(&runner, disk)
            .iter()
            .any(|candidate| candidate == &device_id)
        {
            return Err(format!(
                "重开后 USB/SCSI 硬件候选不再包含 device_id={device_id}"
            ));
        }
        for (index, &lba) in touched.iter().enumerate() {
            let current = inner
                .read_sector(lba)
                .map_err(|error| format!("重开后读取 LBA{lba} 失败: {error}"))?;
            if current != before[index] {
                return Err(format!("LBA{lba} 在确认/卸载期间发生变化，拒绝 HIL"));
            }
        }

        let mut plan = WriteTransactionPlan::new(total_before);
        for (index, &lba) in touched.iter().enumerate() {
            let mut mutated = before[index].clone();
            for byte in &mut mutated {
                *byte ^= 0xA5 ^ (index as u8);
            }
            plan.insert(
                lba,
                mutated,
                SectorWriteStage::Data,
                format!("real-usb-rollback-hil-{index}"),
            )
            .map_err(|error| format!("构造 HIL transaction 失败: {error}"))?;
        }

        let mut dev = FailOnceDev {
            inner,
            writes: 0,
            fail_on: 2,
            failed: false,
        };
        let error = execute_write_transaction(&mut dev, &plan)
            .expect_err("真实 USB rollback HIL 必须由注入故障触发失败");
        if error.code != EXIT_ROLLED_BACK {
            return Err(format!(
                "期望 EXIT_ROLLED_BACK，实际 code={} msg={}",
                error.code, error.msg
            ));
        }
        drop(dev);

        let mut verify = FileDev::open_rdonly(&raw)
            .map_err(|error| format!("回滚后重开 {raw} 失败: {error}"))?;
        for (index, &lba) in touched.iter().enumerate() {
            let actual = verify
                .read_sector(lba)
                .map_err(|error| format!("回滚后读取 LBA{lba} 失败: {error}"))?;
            if actual != before[index] {
                return Err(format!("rollback 后 LBA{lba} 未恢复到写前字节"));
            }
        }

        println!(
            "PASS real USB rollback: disk{} device_id={} touched={:?} injected_write=2 result=EXIT_ROLLED_BACK independent_readback=PASS",
            disk, device_id, touched
        );
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() {
    if let Err(error) = macos::run() {
        eprintln!("FAIL real USB rollback HIL: {error}");
        std::process::exit(1);
    }
}
