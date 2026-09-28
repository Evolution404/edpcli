use super::*;

fn advanced_plain_hex(data: &[u8]) -> String {
    let mut out = String::new();
    for (line_no, line) in data.chunks(16).enumerate() {
        let base = line_no * 16;
        out.push_str(&format!("+0x{base:03X}: "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(&byte) = line.get(i) {
                out.push_str(&format!("{byte:02X} "));
            } else {
                out.push_str("   ");
            }
        }
        out.push(' ');
        for &byte in line {
            out.push(if (0x20..=0x7e).contains(&byte) {
                byte as char
            } else {
                '.'
            });
        }
        out.push('\n');
    }
    out
}

pub(super) fn export_advanced_bytes(
    dir: &Path,
    lba: u64,
    suffix: &str,
    data: &[u8],
) -> Result<(), InspectError> {
    std::fs::create_dir_all(dir).map_err(|error| {
        InspectError::io(format!(
            "创建 Inspect 导出目录 {} 失败: {error}",
            dir.display()
        ))
    })?;
    let base = format!("LBA{lba}_{suffix}");
    std::fs::write(dir.join(format!("{base}.bin")), data)
        .map_err(|error| InspectError::io(format!("导出 {base}.bin 失败: {error}")))?;
    std::fs::write(dir.join(format!("{base}.hex")), advanced_plain_hex(data))
        .map_err(|error| InspectError::io(format!("导出 {base}.hex 失败: {error}")))?;
    Ok(())
}

pub(super) fn export_advanced_meta(dir: &Path, lba: u64, text: &str) -> Result<(), InspectError> {
    std::fs::create_dir_all(dir).map_err(|error| {
        InspectError::io(format!(
            "创建 Inspect 导出目录 {} 失败: {error}",
            dir.display()
        ))
    })?;
    std::fs::write(dir.join(format!("LBA{lba}_meta.txt")), text)
        .map_err(|error| InspectError::io(format!("导出 LBA{lba}_meta.txt 失败: {error}")))
}
