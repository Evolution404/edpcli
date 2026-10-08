//! Shared sparse-filesystem transaction executor.
//!
//! Filesystem construction remains in the pure filesystem domain. This
//! application helper owns the common physical mirror/write/readback/rollback path so
//! provisioning and post-restore formatting cannot drift into separate writers.

use crate::common::{EdpCliError, EdpCliResult, EXIT_TARGET};
use crate::diskio;
use crate::filesystem::SparseFilesystemImage;
use crate::ports::SectorDev;

pub(crate) fn write_sparse_filesystem_image(
    dev: &mut dyn SectorDev,
    start_lba: u64,
    image: &SparseFilesystemImage,
    layout: diskio::BorrowedFormatLayout,
    observer: &mut dyn FnMut(diskio::TransactionActivity),
) -> EdpCliResult<()> {
    let partition_end = start_lba
        .checked_add(image.volume_sectors())
        .ok_or_else(|| EdpCliError::new(EXIT_TARGET, "错误: 格式化分区末端 LBA 溢出"))?;
    // Borrow the sparse filesystem's metadata; only rollback snapshots own
    // additional 512-byte payloads. This preserves the shared transaction path.
    let mut writes = Vec::with_capacity(image.sectors().len());
    for (&relative, sector) in image.sectors() {
        if relative >= image.volume_sectors() {
            return Err(EdpCliError::new(
                EXIT_TARGET,
                "错误: 格式化镜像超出分区边界",
            ));
        }
        let absolute = start_lba
            .checked_add(relative)
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| EdpCliError::new(EXIT_TARGET, "错误: 格式化写入 LBA 溢出"))?;
        writes.push((absolute, sector));
    }
    diskio::execute_borrowed_data_transaction_scoped_observed(
        dev,
        partition_end,
        &writes,
        diskio::BorrowedFormatBounds {
            start_lba,
            end_exclusive: partition_end,
            layout,
        },
        &mut |mut activity| {
            activity.phase = match activity.phase {
                diskio::TransactionActivityPhase::Write => {
                    diskio::TransactionActivityPhase::FormatWrite
                }
                diskio::TransactionActivityPhase::Readback => {
                    diskio::TransactionActivityPhase::FormatReadback
                }
                phase => phase,
            };
            observer(activity);
        },
    )
}
