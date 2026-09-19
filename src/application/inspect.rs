//! Read-only inspect application service shared by CLI/TUI frontends.

use std::path::Path;

use crate::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use crate::diskio::{self, FileDev, SectorReadCache};
use crate::identify::identify;
use crate::inspect::{self, InspectMeta, SectorView};
use crate::sysinfo::{self, CmdRunner};

#[derive(Debug, Clone)]
pub struct InspectWorkspace {
    pub source: String,
    pub meta: InspectMeta,
    pub views: Vec<SectorView>,
}

fn analyze_image(
    source: String,
    data: &[u8],
    meta: InspectMeta,
) -> Result<InspectWorkspace, String> {
    if data.len() != METADATA_IMAGE_LEN {
        return Err(format!(
            "错误: inspect 镜像长度 {}B，预期 {}B (LBA0-12)",
            data.len(),
            METADATA_IMAGE_LEN
        ));
    }
    let views = (0..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            let start = lba as usize * SECTOR;
            inspect::analyze_sector(lba, &data[start..start + SECTOR], &meta)
        })
        .collect();
    Ok(InspectWorkspace {
        source,
        meta,
        views,
    })
}

pub fn load_backup_inspect(path: &Path) -> Result<InspectWorkspace, String> {
    let data = std::fs::read(path)
        .map_err(|error| format!("错误: 无法读取备份 {}: {error}", path.display()))?;
    let meta = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(diskio::parse_backup_name)
        .as_ref()
        .map(InspectMeta::from_backup_meta)
        .unwrap_or_default();
    analyze_image(path.display().to_string(), &data, meta)
}

pub fn load_disk_inspect(runner: &dyn CmdRunner, disk: u32) -> Result<InspectWorkspace, String> {
    crate::application::write::guard_usb_disk(runner, disk).map_err(|error| error.msg)?;
    let path = diskio::raw_path(disk);
    let mut dev = FileDev::open_rdonly(&path)
        .map_err(|error| format!("错误: 无法只读打开 disk{disk}: {error}"))?;
    let mut reader = SectorReadCache::new(&mut dev);
    let mut sectors = Vec::with_capacity(METADATA_IMAGE_LEN);
    for lba in 0..METADATA_SECTOR_COUNT as u32 {
        let raw = reader
            .read_sector(lba)
            .map_err(|error| format!("错误: 读取 disk{disk} LBA{lba} 失败: {error}"))?;
        if raw.len() != SECTOR {
            return Err(format!(
                "错误: disk{disk} LBA{lba} 长度 {}B，预期 {SECTOR}B",
                raw.len()
            ));
        }
        sectors.extend_from_slice(&raw);
    }

    let raw7 = &sectors[7 * SECTOR..8 * SECTOR];
    let id = identify(runner, disk, raw7).device_id;
    let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
    let size_bytes = sysinfo::disk_total_sectors(runner, disk)
        .and_then(|value| value.checked_mul(SECTOR as u64));
    let onlyid = diskio::lba4_label_id_from(&sectors[4 * SECTOR..5 * SECTOR]);
    let meta = InspectMeta {
        device_id: id,
        vid: (vid != "xxxx").then_some(vid),
        pid: (pid != "xxxx").then_some(pid),
        size_bytes,
        onlyid,
    };
    analyze_image(format!("物理盘 disk{disk} ({path})"), &sectors, meta)
}
