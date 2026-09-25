use super::*;

pub fn export_sparse_provision_image(
    path: &Path,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    if prepared.target_plan.as_ref().is_some_and(|plan| {
        plan.partitions
            .iter()
            .any(|part| part.action == PartitionAction::PreserveExact)
    }) {
        return Err(err(
            EXIT_TARGET,
            "错误: 含保留数据的制盘计划不能导出为稀疏镜像；镜像不包含来源盘用户数据",
        ));
    }
    let byte_len = prepared
        .write_image
        .total_sectors
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| err(EXIT_IO, "错误: 镜像长度溢出"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 无法创建 {}: {error}", path.display()),
            )
        })?;
    file.set_len(byte_len)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法设置镜像长度: {error}")))?;
    for (&lba, sector) in &prepared.write_image.patch {
        file.seek(SeekFrom::Start(u64::from(lba) * SECTOR as u64))
            .and_then(|_| file.write_all(sector))
            .map_err(|error| err(EXIT_IO, format!("错误: 写入镜像 LBA{lba} 失败: {error}")))?;
    }
    for choice in prepared
        .format_targets
        .iter()
        .filter(|choice| choice.selected)
    {
        let built = choice
            .prepared_image
            .as_ref()
            .ok_or_else(|| err(EXIT_TARGET, "错误: 导出计划缺少预生成格式化镜像"))?;
        for (&relative_lba, sector) in built.image.sectors() {
            let absolute_lba = choice
                .target
                .geometry
                .start_sector
                .checked_add(relative_lba)
                .ok_or_else(|| err(EXIT_TARGET, "错误: 导出格式化 LBA 溢出"))?;
            file.seek(SeekFrom::Start(absolute_lba * SECTOR as u64))
                .and_then(|_| file.write_all(sector))
                .map_err(|error| {
                    err(
                        EXIT_IO,
                        format!("错误: 写入格式化镜像 LBA{absolute_lba} 失败: {error}"),
                    )
                })?;
        }
    }
    file.sync_all()
        .map_err(|error| err(EXIT_IO, format!("错误: 镜像同步失败: {error}")))?;
    Ok(())
}

pub fn export_sparse_plain_provision_image(
    path: &Path,
    prepared: &PreparedPlainProvision,
) -> EdpCliResult<()> {
    let byte_len = prepared
        .plan
        .total_sectors
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| err(EXIT_IO, "错误: Plain 镜像长度溢出"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 无法创建 {}: {error}", path.display()),
            )
        })?;
    file.set_len(byte_len)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法设置 Plain 镜像长度: {error}")))?;

    for (&lba, sector) in &prepared.write_plan.writes {
        file.seek(SeekFrom::Start(u64::from(lba) * SECTOR as u64))
            .and_then(|_| file.write_all(&sector.bytes))
            .map_err(|error| {
                err(
                    EXIT_IO,
                    format!("错误: 写入 Plain 镜像 LBA{lba} 失败: {error}"),
                )
            })?;
    }
    let lba3 = &prepared.source_metadata[3 * SECTOR..4 * SECTOR];
    file.seek(SeekFrom::Start(3 * SECTOR as u64))
        .and_then(|_| file.write_all(lba3))
        .map_err(|error| err(EXIT_IO, format!("错误: 写入 Plain 镜像 LBA3 失败: {error}")))?;
    file.sync_all()
        .map_err(|error| err(EXIT_IO, format!("错误: Plain 镜像同步失败: {error}")))?;
    Ok(())
}

pub fn export_provision_image(path: &Path, prepared: &PreparedProvision) -> EdpCliResult<()> {
    match prepared {
        PreparedProvision::Official(prepared) => export_sparse_provision_image(path, prepared),
        PreparedProvision::Plain(prepared) => export_sparse_plain_provision_image(path, prepared),
    }
}
