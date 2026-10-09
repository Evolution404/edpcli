//! Pure in-memory native-block assembly for **virtual** full-disk images.
//! This module does not open devices, perform physical I/O, or grant 4Kn
//! provisioning authorization. Physical transactions require separate gates.

use std::collections::BTreeMap;

use super::{
    FilesystemError, FilesystemErrorKind, FilesystemKind, NativeFilesystemWrite, NativeFormatPlan,
};

/// An assembled full-disk plan. The protective data after the 512B MBR
/// header is still one fully-owned native logical block, initialized to zero.
#[derive(Clone, Debug)]
pub struct NativeVirtualDiskPlan {
    pub total_sectors: u64,
    pub sector_bytes: u32,
    /// Contains unique, complete native LBA blocks. LBA0 is last so that
    /// consumers of the virtual image see a populated table only at the end.
    pub writes: Vec<NativeFilesystemWrite>,
}

fn invalid(message: &str) -> FilesystemError {
    FilesystemError::new(FilesystemErrorKind::InvalidGeometry, message)
}

impl NativeVirtualDiskPlan {
    /// Assemble a traditional 4-entry MBR and 1..=4 native filesystem
    /// metadata plans. All partitions and LBAs are expressed in **native**
    /// blocks; no legacy 512B sector equivalence is permitted.
    pub fn assemble(
        total_sectors: u64,
        sector_bytes: u32,
        partitions: &[NativeFormatPlan],
    ) -> Result<Self, FilesystemError> {
        if !matches!(sector_bytes, 512 | 4096)
            || total_sectors < 2
            || total_sectors > u32::MAX as u64
            || !(1..=4).contains(&partitions.len())
            || total_sectors.checked_mul(u64::from(sector_bytes)).is_none()
        {
            return Err(invalid("虚拟MBR整盘逻辑块大小、数量或分区数量不合法"));
        }

        let mut intervals = Vec::with_capacity(partitions.len());
        let mut blocks = BTreeMap::new();
        let mut mbr = vec![0u8; sector_bytes as usize];

        for (index, plan) in partitions.iter().enumerate() {
            let start = plan.geometry.partition_offset;
            let count = plan.geometry.sector_count;
            let end = start
                .checked_add(count)
                .ok_or_else(|| invalid("原生分区末端LBA溢出"))?;
            if plan.geometry.sector_size != sector_bytes
                || count == 0
                || start == 0
                || end > total_sectors
                || end > u32::MAX as u64
            {
                return Err(invalid("文件系统原生几何与虚拟整盘边界不一致"));
            }
            if plan.expected_metadata.kind != plan.filesystem
                || !plan.writes.iter().any(|write| write.relative_lba == 0)
            {
                return Err(invalid("虚拟整盘文件系统类型不一致或缺少引导块"));
            }
            let partition_type = match plan.filesystem {
                FilesystemKind::Fat12 => 0x01,
                FilesystemKind::Fat16 => 0x06,
                FilesystemKind::Fat32 => 0x0c,
                FilesystemKind::ExFat => 0x07,
                _ => return Err(invalid("虚拟MBR整盘不支持该文件系统类型")),
            };
            let entry = 446 + index * 16;
            mbr[entry + 4] = partition_type;
            mbr[entry + 8..entry + 12].copy_from_slice(&(start as u32).to_le_bytes());
            mbr[entry + 12..entry + 16].copy_from_slice(&(count as u32).to_le_bytes());
            intervals.push((start, end));

            for write in &plan.writes {
                if write.relative_lba >= count || write.data.len() != sector_bytes as usize {
                    return Err(invalid("文件系统元数据块尺寸或分区相对LBA非法"));
                }
                let lba = start
                    .checked_add(write.relative_lba)
                    .ok_or_else(|| invalid("文件系统元数据绝对LBA溢出"))?;
                if blocks.insert(lba, write.data.clone()).is_some() {
                    return Err(invalid("虚拟整盘存在重复原生LBA写入"));
                }
            }
        }

        intervals.sort_unstable();
        for pair in intervals.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(invalid("虚拟整盘存在重叠分区"));
            }
        }

        mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
        let mut writes = blocks
            .into_iter()
            .map(|(relative_lba, data)| NativeFilesystemWrite { relative_lba, data })
            .collect::<Vec<_>>();
        writes.push(NativeFilesystemWrite {
            relative_lba: 0,
            data: mbr,
        });
        Ok(Self {
            total_sectors,
            sector_bytes,
            writes,
        })
    }
}
