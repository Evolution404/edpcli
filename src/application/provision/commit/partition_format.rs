//! Empty filesystem write, readback and deep verification.
use super::*;

/// The plain verification image may only be used after every *authored*
/// ciphertext/plain metadata sector has been verified directly from the
/// device. Byte identity then proves the planned plaintext represents those
/// physical sectors, without copying FileKeys into the format report.
/// Sparse, unauthored/free-space sectors remain outside this proof.
fn verify_authored_metadata_readback(
    dev: &mut dyn SectorDev,
    start_lba: u64,
    raw_image: &SparseFilesystemImage,
    plain_image: &SparseFilesystemImage,
) -> EdpCliResult<()> {
    if raw_image.volume_sectors() != plain_image.volume_sectors()
        || raw_image.sectors().keys().ne(plain_image.sectors().keys())
        || !raw_image.sectors().contains_key(&0)
    {
        return Err(err(EXIT_TARGET, "错误: 格式化写入与明文验证扇区清单不一致"));
    }
    dev.sync()
        .map_err(|error| err(EXIT_IO, format!("错误: 格式化二次回读前同步失败: {error}")))?;
    for (&relative_lba, expected_raw) in raw_image.sectors() {
        let absolute_lba = start_lba
            .checked_add(relative_lba)
            .filter(|_| relative_lba < raw_image.volume_sectors())
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化二次回读LBA越界"))?;
        let observed = dev.read_sector(absolute_lba).map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 格式化二次回读LBA{absolute_lba}失败: {error}"),
            )
        })?;
        if observed.len() != SECTOR || observed.as_slice() != expected_raw {
            return Err(err(
                EXIT_IO,
                format!("错误: 格式化二次回读LBA{absolute_lba}完整物理扇区不一致"),
            ));
        }
    }
    Ok(())
}

struct PreparedImageReader<'a> {
    image: &'a SparseFilesystemImage,
}

impl PartitionReader for PreparedImageReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        self.image
            .sector_or_zero(relative_lba)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| std::io::Error::other("format read outside partition"))
    }
}

impl crate::filesystem::FilesystemReader for PreparedImageReader<'_> {
    fn sector_size(&self) -> u32 {
        SECTOR as u32
    }

    fn sector_count(&self) -> u64 {
        self.image.volume_sectors()
    }

    fn read_sector(
        &mut self,
        relative_lba: u64,
    ) -> Result<[u8; SECTOR], crate::filesystem::FilesystemError> {
        self.image.sector_or_zero(relative_lba).ok_or_else(|| {
            crate::filesystem::FilesystemError::new(
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                "格式化验证读取超出分区范围",
            )
        })
    }
}

pub(super) fn format_partition_with_progress(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
    choice: &PlannedPartitionFormat,
    observer: &mut dyn FnMut(diskio::TransactionActivity),
) -> Result<(), crate::application::error::OperationError> {
    use crate::application::error::{MediaState, OperationError};
    verify_format_identity(runner, dev, prepared).map_err(|error| {
        OperationError::from(error)
            .in_phase("格式化前身份复核")
            .with_media_state(MediaState::Unchanged)
    })?;
    let mut writing_started = false;
    execute_partition_format_observed(dev, choice, &mut |activity| {
        if matches!(
            activity.phase,
            diskio::TransactionActivityPhase::Write | diskio::TransactionActivityPhase::FormatWrite
        ) {
            writing_started = true;
        }
        observer(activity);
    })
    .map_err(|error| {
        let mut error = OperationError::from(error).in_phase("分区格式化");
        if error.media_state.is_none() {
            error.media_state = Some(if writing_started {
                MediaState::Unknown
            } else {
                MediaState::Unchanged
            });
        }
        error
    })?;
    verify_format_identity(runner, dev, prepared).map_err(|error| {
        OperationError::from(error)
            .in_phase("格式化后身份复核")
            .with_media_state(MediaState::Unknown)
    })?;
    Ok(())
}

/// Format one verified official partition. The caller owns device identity and
/// protocol verification; this operation never writes the protocol region.
#[cfg(test)]
pub(in crate::application::provision) fn execute_partition_format(
    dev: &mut dyn SectorDev,
    choice: &PlannedPartitionFormat,
) -> EdpCliResult<()> {
    execute_partition_format_observed(dev, choice, &mut |_| {})
}

fn execute_partition_format_observed(
    dev: &mut dyn SectorDev,
    choice: &PlannedPartitionFormat,
    observer: &mut dyn FnMut(diskio::TransactionActivity),
) -> EdpCliResult<()> {
    let filesystem = choice
        .filesystem
        .ok_or_else(|| err(EXIT_TARGET, "错误: 兼容保留区不可格式化"))?;
    let built = choice
        .prepared_image
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化计划缺少预生成物理镜像"))?;
    let verification_image = choice
        .verification_image
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化计划缺少验证镜像"))?;
    if built.geometry != choice.target.geometry
        || built.physically_encrypted != choice.target.physically_encrypted
        || built.image.volume_sectors() != choice.target.geometry.sector_count()
        || verification_image.volume_sectors() != choice.target.geometry.sector_count()
    {
        return Err(err(EXIT_TARGET, "错误: 预生成格式化镜像与目标几何不一致"));
    }
    crate::application::filesystem_format::write_sparse_filesystem_image(
        dev,
        choice.target.geometry.start_sector,
        &built.image,
        crate::diskio::BorrowedFormatLayout::Edp,
        observer,
    )?;
    // The transaction itself checks the first synchronized readback. Check
    // every authored sector again from the device before interpreting the
    // planned plaintext as the disk's filesystem metadata.
    verify_authored_metadata_readback(
        dev,
        choice.target.geometry.start_sector,
        &built.image,
        verification_image,
    )?;
    let raw_boot = dev
        .read_sector(
            u32::try_from(choice.target.geometry.start_sector)
                .map_err(|_| err(EXIT_TARGET, "错误: 分区起点 LBA 溢出"))?,
        )
        .map_err(|error| err(EXIT_IO, format!("错误: 读取文件系统引导扇区失败: {error}")))?;
    let raw_plain_filesystem = if choice.target.physically_encrypted {
        let registry = crate::filesystem::default_registry();
        let mut raw_reader = crate::filesystem::BootSectorReader::new(
            &raw_boot,
            choice.target.geometry.sector_count(),
        );
        registry
            .detect(&mut raw_reader)
            .map_err(|error| err(EXIT_IO, format!("错误: 文件系统首扇区检测失败: {error}")))?
            .is_some_and(|detected| {
                detected.result.confidence == crate::filesystem::DetectionConfidence::Exact
            })
    } else {
        false
    };
    if raw_plain_filesystem {
        return Err(err(EXIT_IO, "错误: 加密分区物理首扇区出现明文文件系统签名"));
    }
    let geometry = PartitionGeometry {
        index: 0,
        partition_type: choice.target.geometry.partition_type.raw(),
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: choice.target.geometry.start_sector,
        sector_size: SECTOR as u64,
        partition_size: choice.target.geometry.size_bytes,
        sector_count: choice.target.geometry.sector_count(),
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    let registry = crate::filesystem::default_registry();
    let driver = registry.driver(filesystem).ok_or_else(|| {
        err(
            EXIT_IO,
            format!("错误: {} 文件系统没有已注册驱动", filesystem.config_token()),
        )
    })?;
    if !driver.capabilities().verify_format {
        return Err(err(
            EXIT_IO,
            format!(
                "错误: 当前不支持 {} 格式化读回校验",
                filesystem.config_token()
            ),
        ));
    }
    let mut request = crate::filesystem::FormatRequest::new(filesystem);
    request.volume_label = (!choice.volume_label.is_empty()).then(|| choice.volume_label.clone());
    request.volume_serial = Some(choice.volume_serial);
    let expected = driver.expected_format_metadata(&request).map_err(|error| {
        err(
            EXIT_IO,
            format!(
                "错误: {} 格式化预期元数据无效: {error}",
                filesystem.config_token()
            ),
        )
    })?;
    let fs_geometry = crate::filesystem::FilesystemGeometry::new(
        choice.target.geometry.start_sector,
        choice.target.geometry.sector_count(),
        SECTOR as u32,
    );
    // This reader projects only metadata authenticated against device bytes
    // above. Never mistake its sparse zero fallback for wiped free space.
    let mut reader = PreparedImageReader {
        image: verification_image,
    };
    driver
        .verify_format(&mut reader, fs_geometry, &expected)
        .map_err(|error| {
            err(
                EXIT_IO,
                format!(
                    "错误: {} 格式化读回校验失败: {error}",
                    filesystem.config_token()
                ),
            )
        })?;

    let report = analyze_partition(&geometry, &mut reader);
    if report.status != AnalysisStatus::Parsed
        || report.filesystem.as_deref() != Some(filesystem.config_token())
        || report.file_count != Some(0)
    {
        return Err(err(
            EXIT_IO,
            format!(
                "错误: {} 深度解析失败: {}",
                filesystem.config_token(),
                report.reason
            ),
        ));
    }
    Ok(())
}
