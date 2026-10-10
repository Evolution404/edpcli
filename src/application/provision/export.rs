use super::*;
use crate::filesystem::{NativeFilesystemWrite, NativeVirtualDiskPlan};

fn export_native_virtual_image(
    path: &Path,
    plan: NativeVirtualDiskPlan,
    label: &str,
) -> EdpCliResult<()> {
    // No virtual-image destination may be a device path or special file.
    // Atomic rename is not authorization to replace a mounted device node.
    if path.starts_with("/dev") || crate::platform::is_raw_device_path(&path.to_string_lossy()) {
        return Err(err(EXIT_TARGET, "错误: 虚拟镜像禁止写入真实设备路径"));
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(err(EXIT_TARGET, "错误: 镜像目标是目录、符号链接或设备节点"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(err(EXIT_IO, format!("错误: 无法验证镜像目标: {error}")));
        }
    }
    let mut candidate = crate::infrastructure::atomic_file::AtomicFile::new(path)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法创建镜像候选文件: {e}")))?;
    super::native_image::write_native_virtual_plan(&mut candidate.file, &plan)
        .map_err(|e| err(EXIT_IO, format!("错误: {label}镜像写入或回读失败: {e}")))?;
    // Verify a fresh independent descriptor against the entire 512B/4Kn
    // authored write-set BEFORE replacing a previously exported image.
    // Atomic rename alone cannot repair an already published corrupt file.
    candidate
        .file
        .sync_all()
        .map_err(|e| err(EXIT_IO, format!("错误: {label}候选镜像落盘失败: {e}")))?;
    super::native_image::verify_native_virtual_image(candidate.candidate_path(), &plan)
        .map_err(|e| err(EXIT_IO, format!("错误: {label}镜像独立重开校验失败: {e}")))?;
    candidate
        .publish(true)
        .map_err(|e| err(EXIT_IO, format!("错误: {label}镜像发布失败: {e}")))?;
    Ok(())
}

/// Pure offline plan assembly shared by hardware-prepared EDP image export
/// and independent virtual EDP provisioning. This stage never opens a device.
pub(super) fn assemble_official_virtual_plan(
    image: &OfficialProvisionWriteImage,
    choices: &[PlannedPartitionFormat],
) -> EdpCliResult<NativeVirtualDiskPlan> {
    let mut blocks = BTreeMap::<u64, Vec<u8>>::new();
    for (&lba, sector) in &image.patch {
        blocks.insert(u64::from(lba), sector.clone());
    }
    for choice in choices.iter().filter(|choice| choice.selected) {
        let built = choice
            .prepared_image
            .as_ref()
            .ok_or_else(|| err(EXIT_TARGET, "错误: 导出计划缺少预生成格式化镜像"))?;
        for (&relative_lba, sector) in built.image.sectors() {
            let lba = choice
                .target
                .geometry
                .start_sector
                .checked_add(relative_lba)
                .ok_or_else(|| err(EXIT_TARGET, "错误: 导出格式化 LBA 溢出"))?;
            if blocks.insert(lba, sector.to_vec()).is_some() {
                return Err(err(
                    EXIT_TARGET,
                    format!("错误: 协议/格式化块重叠 LBA{lba}"),
                ));
            }
        }
    }
    let mbr = blocks
        .remove(&0)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 导出计划缺少 LBA0 MBR"))?;
    let mut writes = blocks
        .into_iter()
        .map(|(relative_lba, data)| NativeFilesystemWrite { relative_lba, data })
        .collect::<Vec<_>>();
    writes.push(NativeFilesystemWrite {
        relative_lba: 0,
        data: mbr,
    });
    Ok(NativeVirtualDiskPlan {
        total_sectors: image.total_sectors,
        sector_bytes: SECTOR as u32,
        writes,
    })
}

pub fn export_sparse_provision_image(
    path: &Path,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    if prepared.target_plan.as_ref().is_some_and(|plan| {
        plan.partitions
            .iter()
            .any(|part| part.disposition.preserves_extent())
    }) {
        return Err(err(
            EXIT_TARGET,
            "错误: 含保留数据的制盘计划不能导出为稀疏镜像；镜像不包含来源盘用户数据",
        ));
    }
    // No replay-only source data or independently invented producer.
    // The exact same validated write-set builder serves the offline API.
    let plan = assemble_official_virtual_plan(&prepared.write_image, &prepared.format_targets)?;
    export_native_virtual_image(path, plan, "EDP")
}

pub fn export_sparse_plain_provision_image(
    path: &Path,
    prepared: &PreparedPlainProvision,
) -> EdpCliResult<()> {
    let mut blocks = prepared
        .write_plan
        .writes
        .iter()
        .map(|(&lba, sector)| (u64::from(lba), sector.bytes.to_vec()))
        .collect::<BTreeMap<_, _>>();
    let lba3 = prepared
        .source_metadata
        .get(3 * SECTOR..4 * SECTOR)
        .ok_or_else(|| err(EXIT_TARGET, "错误: Plain 来源 LBA3 快照不完整"))?;
    if blocks.insert(3, lba3.to_vec()).is_some() {
        return Err(err(EXIT_TARGET, "错误: Plain 写计划覆盖要求保留的 LBA3"));
    }
    let mbr = blocks
        .remove(&0)
        .ok_or_else(|| err(EXIT_TARGET, "错误: Plain 镜像写计划缺少 MBR"))?;
    if mbr != prepared.write_plan.mbr {
        return Err(err(EXIT_TARGET, "错误: Plain MBR 与写计划不一致"));
    }
    let mut writes = blocks
        .into_iter()
        .map(|(relative_lba, data)| NativeFilesystemWrite { relative_lba, data })
        .collect::<Vec<_>>();
    writes.push(NativeFilesystemWrite {
        relative_lba: 0,
        data: mbr,
    });
    export_native_virtual_image(
        path,
        NativeVirtualDiskPlan {
            total_sectors: prepared.plan.total_sectors,
            sector_bytes: SECTOR as u32,
            writes,
        },
        "Plain",
    )
}

pub fn export_provision_image(path: &Path, prepared: &PreparedProvision) -> EdpCliResult<()> {
    match prepared {
        PreparedProvision::Official(prepared) => export_sparse_provision_image(path, prepared),
        PreparedProvision::Plain(prepared) => export_sparse_plain_provision_image(path, prepared),
        PreparedProvision::Native(native) => {
            super::native_image::export_native_plain_image(path, &native.plan)
                .map_err(|message| err(EXIT_IO, message))
        }
    }
}
