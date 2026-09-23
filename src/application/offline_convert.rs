//! UI-neutral offline conversion of an exported LBA snapshot directory.

use std::path::{Path, PathBuf};

use crate::common::{EdpCliError, EdpCliResult, EXIT_IO};
use crate::sectors::{ConvertReport, ConvertResult};

#[derive(Debug, Clone, PartialEq)]
pub struct OfflineConvertRequest {
    pub source_dir: PathBuf,
    pub device_id: String,
    pub size_gb: Option<f64>,
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct OfflineConvertOutput {
    pub reports: Vec<ConvertReport>,
    pub result: ConvertResult,
    pub output_dir: Option<PathBuf>,
}

fn write_outputs(dir: &Path, result: &ConvertResult) -> EdpCliResult<()> {
    std::fs::create_dir_all(dir).map_err(|error| {
        EdpCliError::new(
            EXIT_IO,
            format!("错误: 无法创建输出目录 {}: {error}", dir.display()),
        )
    })?;
    for (lba, data) in [
        (0u32, &result.lba0),
        (6, &result.lba6),
        (7, &result.lba7),
        (12, &result.lba12),
    ] {
        let path = dir.join(format!("LBA{lba:02}.bin"));
        std::fs::write(&path, data).map_err(|error| {
            EdpCliError::new(
                EXIT_IO,
                format!("错误: 无法写入 {}: {error}", path.display()),
            )
        })?;
    }
    if let Some(lba9) = &result.lba9 {
        let path = dir.join("LBA09.bin");
        std::fs::write(&path, lba9).map_err(|error| {
            EdpCliError::new(
                EXIT_IO,
                format!("错误: 无法写入 {}: {error}", path.display()),
            )
        })?;
    }
    Ok(())
}

pub fn run(request: &OfflineConvertRequest) -> EdpCliResult<OfflineConvertOutput> {
    let read = |lba: u32| -> EdpCliResult<Vec<u8>> {
        Ok(crate::diskio::read_lba_file(&request.source_dir, lba))
    };
    let mut reports = Vec::new();
    let result =
        crate::sectors::convert(&read, &request.device_id, request.size_gb, &mut |report| {
            reports.push(report)
        })?;
    if let Some(output_dir) = &request.output_dir {
        write_outputs(output_dir, &result)?;
    }
    Ok(OfflineConvertOutput {
        reports,
        result,
        output_dir: request.output_dir.clone(),
    })
}
