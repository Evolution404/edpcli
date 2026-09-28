//! Shared sparse-filesystem write/readback executor.
//!
//! Filesystem construction remains in the pure provision domain. This
//! application helper owns the common physical write + sync + readback path so
//! provisioning and post-restore formatting cannot drift into separate writers.

use crate::common::{EdpCliError, EdpCliResult, EXIT_IO, EXIT_TARGET};
use crate::diskio::{self, SectorDev};
use crate::provision::SparseFilesystemImage;

pub(crate) fn write_sparse_filesystem_image(
    dev: &mut dyn SectorDev,
    start_lba: u64,
    image: &SparseFilesystemImage,
    observer: &mut dyn FnMut(diskio::TransactionActivity),
) -> EdpCliResult<()> {
    let total = image.sectors().len() as u64;
    for (index, (&relative, sector)) in image.sectors().iter().enumerate() {
        let absolute = start_lba
            .checked_add(relative)
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| EdpCliError::new(EXIT_TARGET, "错误: 格式化写入 LBA 溢出"))?;
        dev.write_sector(absolute, sector).map_err(|error| {
            EdpCliError::new(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 写入失败: {error}"),
            )
        })?;
        let event = diskio::TransactionActivity {
            phase: diskio::TransactionActivityPhase::FormatWrite,
            current: index as u64 + 1,
            total,
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(event)));
    }
    dev.sync()
        .map_err(|error| EdpCliError::new(EXIT_IO, format!("错误: 格式化同步失败: {error}")))?;
    for (index, (&relative, expected)) in image.sectors().iter().enumerate() {
        let absolute = start_lba
            .checked_add(relative)
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| EdpCliError::new(EXIT_TARGET, "错误: 格式化读回 LBA 溢出"))?;
        let actual = dev.read_sector(absolute).map_err(|error| {
            EdpCliError::new(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 读回失败: {error}"),
            )
        })?;
        if actual.as_slice() != expected {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 读回不一致"),
            ));
        }
        let event = diskio::TransactionActivity {
            phase: diskio::TransactionActivityPhase::FormatReadback,
            current: index as u64 + 1,
            total,
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(event)));
    }
    Ok(())
}
