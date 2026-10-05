//! Build a disposable mode0 front-partition image for native OS filesystem HIL.
//! This example writes only the explicitly supplied regular file, never a device.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

use edpcli::application::filesystem::{build_empty_exfat, build_empty_fat16, build_empty_fat32};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: hil_provision_front OUTPUT.img fat16|fat32|exfat")?;
    let format = args.next().ok_or("missing filesystem")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let (partition_start, partition_sectors, mbr_type, image) = match format.as_str() {
        "fat16" => (
            63u64,
            20_417u64,
            0x0e,
            build_empty_fat16(63, 20_417, 0x1234_5678, "启动区")?,
        ),
        "fat32" => (
            2_048u64,
            131_072u64,
            0x0c,
            build_empty_fat32(2_048, 131_072, 0x2345_6789, "FAT32HIL")?,
        ),
        "exfat" => (
            63u64,
            20_417u64,
            0x07,
            build_empty_exfat(63, 20_417, 0x1234_5678, "启动区")?,
        ),
        _ => return Err("filesystem must be fat16, fat32, or exfat".into()),
    };
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.set_len((partition_start + partition_sectors) * 512)?;
    let mut mbr = [0u8; 512];
    mbr[0x1c2] = mbr_type;
    mbr[0x1c6..0x1ca].copy_from_slice(&(partition_start as u32).to_le_bytes());
    mbr[0x1ca..0x1ce].copy_from_slice(&(partition_sectors as u32).to_le_bytes());
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    file.write_all(&mbr)?;
    for (&relative, sector) in image.sectors() {
        file.seek(SeekFrom::Start((partition_start + relative) * 512))?;
        file.write_all(sector)?;
    }
    file.sync_all()?;
    Ok(())
}
