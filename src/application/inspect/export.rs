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
    crate::infrastructure::atomic_file::write(&dir.join(format!("{base}.bin")), data, false)
        .map_err(|error| InspectError::io(format!("导出 {base}.bin 失败: {error}")))?;
    crate::infrastructure::atomic_file::write(
        &dir.join(format!("{base}.hex")),
        advanced_plain_hex(data).as_bytes(),
        false,
    )
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
    crate::infrastructure::atomic_file::write(
        &dir.join(format!("LBA{lba}_meta.txt")),
        text.as_bytes(),
        false,
    )
    .map_err(|error| InspectError::io(format!("导出 LBA{lba}_meta.txt 失败: {error}")))
}

/// Each export run is isolated; only a published completion marker declares it usable.
pub(super) struct ExportBundle {
    pub dir: std::path::PathBuf,
    complete: bool,
}
impl ExportBundle {
    pub fn new(parent: &Path) -> Result<Self, InspectError> {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|e| InspectError::io(e.to_string()))?;
        let dir = parent.join(format!(
            "inspect-{}",
            nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ));
        crate::infrastructure::atomic_file::private_directory(&dir)
            .map_err(|e| InspectError::io(e.to_string()))?;
        Ok(Self {
            dir,
            complete: false,
        })
    }
    pub fn finish(&mut self) -> Result<(), InspectError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(|e| InspectError::io(e.to_string()))? {
            let entry = entry.map_err(|e| InspectError::io(e.to_string()))?;
            let bytes = std::fs::read(entry.path()).map_err(|e| InspectError::io(e.to_string()))?;
            files.push(serde_json::json!({"name": entry.file_name().to_string_lossy(), "bytes": bytes.len(), "sha256": crate::sha256::sha256_hex(&bytes)}));
        }
        files.sort_by_key(|f| f["name"].as_str().unwrap_or_default().to_owned());
        let bytes = serde_json::to_vec_pretty(
            &serde_json::json!({"schema":"edpcli.inspect-export.v1", "files":files}),
        )
        .map_err(|e| InspectError::io(e.to_string()))?;
        crate::infrastructure::atomic_file::write(&self.dir.join("complete.json"), &bytes, false)
            .map_err(|e| InspectError::io(e.to_string()))?;
        self.complete = true;
        Ok(())
    }
}
impl Drop for ExportBundle {
    fn drop(&mut self) {
        if !self.complete {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
