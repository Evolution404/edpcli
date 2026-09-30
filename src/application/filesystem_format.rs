//! Shared sparse-filesystem transaction executor.
//!
//! Filesystem construction remains in the pure filesystem domain. This
//! application helper owns the common physical mirror/write/readback/rollback path so
//! provisioning and post-restore formatting cannot drift into separate writers.

use crate::common::{EdpCliError, EdpCliResult, EXIT_TARGET};
use crate::diskio::{self, SectorDev, SectorWriteStage, WriteTransactionPlan};
use crate::filesystem::SparseFilesystemImage;

pub(crate) fn write_sparse_filesystem_image(
    dev: &mut dyn SectorDev,
    start_lba: u64,
    image: &SparseFilesystemImage,
    observer: &mut dyn FnMut(diskio::TransactionActivity),
) -> EdpCliResult<()> {
    let partition_end = start_lba
        .checked_add(image.volume_sectors())
        .ok_or_else(|| EdpCliError::new(EXIT_TARGET, "错误: 格式化分区末端 LBA 溢出"))?;
    let mut transaction = WriteTransactionPlan::new(partition_end);
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
        transaction
            .insert(
                absolute,
                sector.to_vec(),
                SectorWriteStage::Data,
                "partition filesystem",
            )
            .map_err(|message| EdpCliError::new(EXIT_TARGET, message))?;
    }
    diskio::execute_write_transaction_observed(dev, &transaction, &mut |mut activity| {
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
    })
}
