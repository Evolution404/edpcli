//! CLI v2 的统一目标选择器。
//!
//! DeviceSelector 只负责把用户意图收敛为一个安全的外接 USB 整盘，并在提权重执行前
//! 固定为平台原生 selector。BackupSelector 只负责把稳定展示编号/文件名解析为
//! BackupCatalog 中的条目；业务命令不再自行解释 onlyid + index 组合。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::application::write::{guard_usb_disk, Prompter};
use crate::backup_catalog::{self, BackupCatalog};
use crate::common::{EdpCliError, EdpCliResult, EXIT_CANCELLED, EXIT_TARGET};
use crate::diskio::{self, BackupEntry};
use crate::sysinfo::{self, CmdRunner, ExtDisk};
use crate::ui::disk_menu_str;

#[derive(Debug, Clone, Copy)]
pub struct DeviceSelector {
    explicit: Option<u32>,
}

impl DeviceSelector {
    pub const fn new(explicit: Option<u32>) -> Self {
        Self { explicit }
    }

    pub fn resolve(&self, runner: &dyn CmdRunner, prompt: &mut dyn Prompter) -> EdpCliResult<u32> {
        if let Some(disk) = self.explicit {
            guard_usb_disk(runner, disk)?;
            return Ok(disk);
        }
        let disks: Vec<_> = sysinfo::list_usb_disks(runner)
            .into_iter()
            .filter(|disk| !crate::platform::is_system_disk(runner, disk.n))
            .collect();
        let disk = self.choose_from(&disks, prompt)?;
        guard_usb_disk(runner, disk)?;
        Ok(disk)
    }

    pub fn choose_from(&self, disks: &[ExtDisk], prompt: &mut dyn Prompter) -> EdpCliResult<u32> {
        if let Some(explicit) = self.explicit {
            if disks.iter().any(|disk| disk.n == explicit) {
                return Ok(explicit);
            }
            return Err(EdpCliError::new(
                EXIT_TARGET,
                format!("错误: disk{explicit} 当前不是可操作的外接 USB 整盘"),
            ));
        }
        match disks {
            [] => Err(EdpCliError::new(
                EXIT_TARGET,
                "错误: 未检测到外部 USB 盘。插入后重试, 或 --disk N 手动指定。",
            )),
            [only] => Ok(only.n),
            _ => {
                println!("检测到多个 USB 盘:");
                print!("{}", disk_menu_str(disks));
                loop {
                    let input = prompt.prompt_line(&crate::ui::bold(&format!(
                        "选择 [1-{}] (回车取消): ",
                        disks.len()
                    )));
                    let input = input.trim();
                    if input.is_empty() {
                        return Err(EdpCliError::new(EXIT_CANCELLED, "已取消"));
                    }
                    if let Ok(index) = input.parse::<usize>() {
                        if (1..=disks.len()).contains(&index) {
                            return Ok(disks[index - 1].n);
                        }
                    }
                    println!("{}", crate::ui::yellow("无效输入"));
                }
            }
        }
    }

    pub fn pin_argv(&self, argv: &mut Vec<String>, disk: u32) {
        let native = crate::platform::disk_selector_value(disk);
        for i in 0..argv.len() {
            if argv[i] == "--disk" {
                if let Some(value) = argv.get_mut(i + 1) {
                    *value = native;
                    return;
                }
                break;
            }
            if argv[i].starts_with("--disk=") {
                argv[i] = format!("--disk={native}");
                return;
            }
        }
        argv.push("--disk".into());
        argv.push(native);
    }
}

pub struct BackupSelector {
    catalog: BackupCatalog,
}

impl BackupSelector {
    pub fn load(root: &Path) -> Self {
        Self {
            catalog: BackupCatalog::load(root),
        }
    }

    pub fn catalog(&self) -> &BackupCatalog {
        &self.catalog
    }

    pub fn numbered(&self) -> Vec<&BackupEntry> {
        numbered_entries(self.catalog.entries(), None)
    }

    pub fn numbered_with_indices(&self) -> Vec<(usize, &BackupEntry)> {
        self.numbered()
            .into_iter()
            .enumerate()
            .map(|(index, entry)| (index + 1, entry))
            .collect()
    }

    pub fn resolve_one(&self, target: &str) -> Result<&BackupEntry, String> {
        resolve_one(&self.catalog, target, None)
    }

    /// 恢复命令的选择语义：
    /// - 数字目标是 backup list 的全局编号，必须先按当前盘 onlyid 过滤；
    /// - 显式文件/路径只做备份根目录约束，介质归属由 restore 随后的 LBA4 16B
    ///   身份终验决定，不能因用户重命名过备份文件而提前误拒绝。
    pub fn resolve_restore_target(
        &self,
        target: &str,
        onlyid: &str,
    ) -> Result<&BackupEntry, String> {
        if !target.is_empty() && target.bytes().all(|byte| byte.is_ascii_digit()) {
            return resolve_one(&self.catalog, target, Some(onlyid));
        }
        resolve_one(&self.catalog, target, None)
    }

    pub fn resolve_many(&self, targets: &[String]) -> Result<Vec<&BackupEntry>, String> {
        resolve_many(&self.catalog, targets, None)
    }

    pub fn for_onlyid<'a>(&'a self, onlyid: &'a str) -> BackupSelectorView<'a> {
        BackupSelectorView {
            selector: self,
            onlyid,
        }
    }
}

pub struct BackupSelectorView<'a> {
    selector: &'a BackupSelector,
    onlyid: &'a str,
}

impl BackupSelectorView<'_> {
    pub fn numbered(&self) -> Vec<&BackupEntry> {
        numbered_entries(self.selector.catalog.entries(), Some(self.onlyid))
    }

    pub fn numbered_with_indices(&self) -> Vec<(usize, &BackupEntry)> {
        self.selector
            .numbered_with_indices()
            .into_iter()
            .filter(|(_, entry)| matches_onlyid(entry, Some(self.onlyid)))
            .collect()
    }

    pub fn resolve_one(&self, target: &str) -> Result<&BackupEntry, String> {
        resolve_one(&self.selector.catalog, target, Some(self.onlyid))
    }

    pub fn resolve_many(&self, targets: &[String]) -> Result<Vec<&BackupEntry>, String> {
        resolve_many(&self.selector.catalog, targets, Some(self.onlyid))
    }
}

fn matches_onlyid(entry: &BackupEntry, onlyid: Option<&str>) -> bool {
    onlyid.is_none_or(|expected| {
        entry.meta.as_ref().and_then(|meta| meta.onlyid.as_deref()) == Some(expected)
    })
}

fn numbered_entries<'a>(entries: &'a [BackupEntry], onlyid: Option<&str>) -> Vec<&'a BackupEntry> {
    let mut visible: Vec<_> = entries
        .iter()
        .filter(|entry| entry.meta.is_some() && matches_onlyid(entry, onlyid))
        .collect();
    visible.sort_by(|a, b| {
        diskio::cmp_backup_newest_first(a, b)
            .then_with(|| backup_catalog::file_name(a).cmp(backup_catalog::file_name(b)))
    });
    visible
}

fn numeric_selection(target: &str) -> bool {
    !target.is_empty()
        && target.bytes().all(|byte| {
            byte.is_ascii_digit() || byte == b',' || byte == b'-' || byte.is_ascii_whitespace()
        })
        && target.bytes().any(|byte| byte.is_ascii_digit())
}

fn parse_indices(tokens: &[String], max: usize) -> Result<Vec<usize>, String> {
    let mut selected = BTreeSet::new();
    for token in tokens {
        for raw in token.split(',') {
            let part = raw.trim();
            if part.is_empty() {
                return Err("备份编号不能为空".into());
            }
            if let Some((left, right)) = part.split_once('-') {
                if right.contains('-') {
                    return Err(format!("无法解析备份范围: {part}"));
                }
                let start = left
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {left}"))?;
                let end = right
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {right}"))?;
                if start == 0 || end == 0 || start > end || end > max {
                    return Err(format!("备份范围超出 1-{max}: {part}"));
                }
                selected.extend(start..=end);
            } else {
                let index = part
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {part}"))?;
                if index == 0 || index > max {
                    return Err(format!("备份编号超出 1-{max}: {part}"));
                }
                selected.insert(index);
            }
        }
    }
    if selected.is_empty() {
        return Err("至少选择一份备份".into());
    }
    Ok(selected.into_iter().collect())
}

fn resolve_one<'a>(
    catalog: &'a BackupCatalog,
    target: &str,
    onlyid: Option<&str>,
) -> Result<&'a BackupEntry, String> {
    if !target.contains(',') && !target.contains('-') && target.bytes().all(|b| b.is_ascii_digit())
    {
        let entries = numbered_entries(catalog.entries(), None);
        let index = target
            .parse::<usize>()
            .map_err(|_| format!("无法解析备份编号: {target}"))?;
        if index == 0 || index > entries.len() {
            return Err(format!("备份编号超出 1-{}: {target}", entries.len()));
        }
        let entry = entries[index - 1];
        if !matches_onlyid(entry, onlyid) {
            return Err(format!("备份编号 [{index}] 不属于当前目标盘，拒绝选择"));
        }
        return Ok(entry);
    }
    let entry = catalog.resolve_target(target)?;
    if !matches_onlyid(entry, onlyid) {
        return Err(format!(
            "备份 {} 不属于当前目标盘，拒绝选择",
            entry.path.display()
        ));
    }
    Ok(entry)
}

fn resolve_many<'a>(
    catalog: &'a BackupCatalog,
    targets: &[String],
    onlyid: Option<&str>,
) -> Result<Vec<&'a BackupEntry>, String> {
    let numbered = numbered_entries(catalog.entries(), None);
    let mut paths = BTreeSet::<PathBuf>::new();
    let mut out = Vec::new();
    for target in targets {
        if numeric_selection(target) {
            for index in parse_indices(std::slice::from_ref(target), numbered.len())? {
                let entry = numbered[index - 1];
                if !matches_onlyid(entry, onlyid) {
                    return Err(format!("备份编号 [{index}] 不属于当前目标盘，拒绝选择"));
                }
                let canonical = backup_catalog::canonical_entry_path(&entry.path);
                if paths.insert(canonical) {
                    out.push(entry);
                }
            }
        } else {
            let entry = resolve_one(catalog, target, onlyid)?;
            let canonical = backup_catalog::canonical_entry_path(&entry.path);
            if paths.insert(canonical) {
                out.push(entry);
            }
        }
    }
    Ok(out)
}
