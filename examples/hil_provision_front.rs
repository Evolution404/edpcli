//! Build a disposable mode0 front-partition image for native OS filesystem HIL.
//! This example writes only the explicitly supplied regular file, never a device.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

use edpcli::provision::{build_empty_exfat, build_empty_fat16};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: hil_provision_front OUTPUT.img fat16|exfat")?;
    let format = args.next().ok_or("missing filesystem")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let image = match format.as_str() {
        "fat16" => build_empty_fat16(63, 20_417, 0x1234_5678, "启动区")?,
        "exfat" => build_empty_exfat(63, 20_417, 0x1234_5678, "启动区")?,
        _ => return Err("filesystem must be fat16 or exfat".into()),
    };
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.set_len(20_480 * 512)?;
    let mut mbr = [0u8; 512];
    mbr[0x1c2] = if format == "fat16" { 0x0e } else { 0x07 };
    mbr[0x1c6..0x1ca].copy_from_slice(&63u32.to_le_bytes());
    mbr[0x1ca..0x1ce].copy_from_slice(&20_417u32.to_le_bytes());
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    file.write_all(&mbr)?;
    for (&relative, sector) in image.sectors() {
        file.seek(SeekFrom::Start((63 + relative) * 512))?;
        file.write_all(sector)?;
    }
    file.sync_all()?;
    Ok(())
}
