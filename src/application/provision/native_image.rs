//! Offline-only native-sector Plain image authoring. No USB discovery, raw
//! device handle, mount, provision commit, or physical write authority.
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::{PlainPartitionRequest, PlainPartitionSize};
use crate::diskio::{execute_native_transaction, NativeBlockDevice};
use crate::filesystem::{
    FilesystemGeometry, FilesystemKind, FormatRequest, NativeFormatPlan, NativeVirtualDiskPlan,
    EXFAT_DRIVER, FAT12_DRIVER, FAT16_DRIVER, FAT32_DRIVER,
};

fn checked_size(size: PlainPartitionSize, sector_bytes: u32) -> Result<Option<u64>, String> {
    let bytes = u64::from(sector_bytes);
    match size {
        PlainPartitionSize::Sectors(n) => Ok(Some(n)),
        PlainPartitionSize::MiB(n) | PlainPartitionSize::GiB(n) => {
            let unit = if matches!(size, PlainPartitionSize::MiB(_)) {
                1024u64 * 1024
            } else {
                1024u64 * 1024 * 1024
            };
            n.checked_mul(unit)
                .ok_or_else(|| "容量换算溢出".to_string())
                .and_then(|v| {
                    (v % bytes == 0)
                        .then_some(Some(v / bytes))
                        .ok_or_else(|| "容量不是完整原生逻辑扇区的整数倍".to_string())
                })
        }
        PlainPartitionSize::Fill => Ok(None),
    }
}

pub fn plan_native_plain_image(
    total_sectors: u64,
    sector_bytes: u32,
    partitions: &[PlainPartitionRequest],
) -> Result<NativeVirtualDiskPlan, String> {
    // 512B and 4Kn are the currently independently verified filesystem sizes.
    // Geometry on a given device is immutable for its entire transaction.
    if !matches!(sector_bytes, 512 | 4096) || total_sectors <= 2048 {
        return Err("仅支持已验证的512B/4096B原生整盘几何".into());
    }
    let default = [PlainPartitionRequest {
        start_lba: 2048,
        size: PlainPartitionSize::Fill,
        filesystem: FilesystemKind::ExFat,
        volume_label: "EDP-PLAIN".into(),
    }];
    let partitions = if partitions.is_empty() {
        &default[..]
    } else {
        partitions
    };
    if partitions.len() > 4 {
        return Err("MBR最多只能包含4个主分区".into());
    }
    let mut plans: Vec<NativeFormatPlan> = Vec::with_capacity(partitions.len());
    for partition in partitions {
        let start = partition.start_lba;
        let remaining = total_sectors
            .checked_sub(start)
            .filter(|v| *v > 0)
            .ok_or("原生分区起点超出整盘范围")?;
        let count = checked_size(partition.size, sector_bytes)?.unwrap_or(remaining);
        if count == 0 || count > remaining {
            return Err("原生分区容量超出整盘边界".into());
        }
        let geometry = FilesystemGeometry::new(start, count, sector_bytes);
        let request = FormatRequest {
            filesystem: partition.filesystem,
            volume_label: Some(partition.volume_label.clone()),
            volume_serial: Some(0xED50_0000 + plans.len() as u32),
        };
        let plan = match partition.filesystem {
            FilesystemKind::Fat12 => FAT12_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::Fat16 => FAT16_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::Fat32 => FAT32_DRIVER.build_native_format_plan(geometry, &request),
            FilesystemKind::ExFat => EXFAT_DRIVER.build_native_format_plan(geometry, &request),
            _ => return Err("离线原生镜像暂不支持格式化此文件系统".into()),
        }
        .map_err(|error| {
            format!(
                "{}原生格式化计划失败: {error}",
                partition.filesystem.label()
            )
        })?;
        plans.push(plan);
    }
    NativeVirtualDiskPlan::assemble(total_sectors, sector_bytes, &plans)
        .map_err(|error| format!("原生整盘镜像规划失败: {error}"))
}

/// Creates a brand-new sparse *ordinary file* image, verifies each complete
/// native block, and commits MBR LBA0 last. Refuses existing paths, symlinks,
/// device paths, nonregular files and unsupported geometry. On error removes
/// only the newly created file. This API cannot provision an existing USB disk.
/// A regular file created by this call, with explicit complete native blocks.
/// No raw-device path or pre-existing image can enter this adapter.
struct NewImageBlockDevice<'a> {
    file: &'a mut fs::File,
    total_sectors: u64,
    sector_bytes: u32,
}

impl NewImageBlockDevice<'_> {
    fn byte_offset(&self, lba: u64) -> std::io::Result<u64> {
        if lba >= self.total_sectors {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "LBA超出新镜像边界",
            ));
        }
        lba.checked_mul(u64::from(self.sector_bytes))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "LBA字节偏移溢出"))
    }
}

impl NativeBlockDevice for NewImageBlockDevice<'_> {
    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
    fn sector_bytes(&self) -> u32 {
        self.sector_bytes
    }
    fn read_block(&mut self, lba: u64) -> std::io::Result<Vec<u8>> {
        let offset = self.byte_offset(lba)?;
        let mut block = vec![0u8; self.sector_bytes as usize];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut block)?;
        Ok(block)
    }
    fn write_block(&mut self, lba: u64, full_block: &[u8]) -> std::io::Result<()> {
        if full_block.len() != self.sector_bytes as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "非完整原生块写入",
            ));
        }
        let offset = self.byte_offset(lba)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(full_block)
    }
    fn sync_blocks(&mut self) -> std::io::Result<()> {
        self.file.sync_all()
    }
}

pub fn export_native_plain_image(path: &Path, plan: &NativeVirtualDiskPlan) -> Result<(), String> {
    let byte_len = plan
        .total_sectors
        .checked_mul(u64::from(plan.sector_bytes))
        .ok_or("虚拟整盘字节长度溢出")?;
    if !matches!(plan.sector_bytes, 512 | 4096) || byte_len == 0 {
        return Err("不支持的原生镜像逻辑扇区几何".into());
    }
    if path.starts_with("/dev") || crate::platform::is_raw_device_path(&path.to_string_lossy()) {
        return Err("拒绝将虚拟镜像输出到设备路径".into());
    }
    if plan.writes.is_empty() || plan.writes.last().is_none_or(|w| w.relative_lba != 0) {
        return Err("原生镜像缺少最后提交的MBR".into());
    }
    // The plan is a public value; revalidate every write even if its creator
    // already checked geometry. This matters for callers that mutate writes.
    let mut seen = std::collections::BTreeSet::new();
    for write in &plan.writes {
        if write.relative_lba >= plan.total_sectors
            || write.data.len() != plan.sector_bytes as usize
            || !seen.insert(write.relative_lba)
        {
            return Err("原生镜像写计划含越界、截断或重复的LBA".into());
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("无法排他创建新的普通文件镜像: {error}"))?;
    let result: Result<(), String> = (|| {
        if !file.metadata().is_ok_and(|m| m.file_type().is_file()) {
            return Err("目标不是普通文件".into());
        }
        file.set_len(byte_len)
            .map_err(|error| format!("无法设置镜像容量: {error}"))?;
        // Both 512B and 4Kn virtual images reuse the same verified staged
        // transaction core; only a brand-new regular-file adapter exists.
        let mut device = NewImageBlockDevice {
            file: &mut file,
            total_sectors: plan.total_sectors,
            sector_bytes: plan.sector_bytes,
        };
        execute_native_transaction(&mut device, plan)
            .map_err(|error| format!("镜像原生事务失败: {error}"))
    })();
    drop(file);
    if result.is_err() {
        // Only a freshly create_new-owned path is ever removed.
        let _ = fs::remove_file(path);
    }
    result
}
