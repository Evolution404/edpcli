//! `edpcli inspect` 的来源解析、展示与导出层。
//!
//! 扇区结构解析仍由 `inspect.rs` 负责；这里只处理显式备份文件或当前物理盘来源、
//! 展示模式和导出，避免 `cli.rs` 直接承担 inspect 业务流程。

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli::StdPrompter;
use crate::cli_args::{InspectMode, InspectOpts};
use crate::common::{EXIT_BACKUP, EXIT_IO, EXIT_OK, EXIT_TARGET, EXIT_USAGE, SECTOR};
use crate::diskio::{self, raw_path, FileDev};
use crate::elevate;
use crate::identify::identify;
use crate::inspect::{self, InspectMeta};
use crate::inspect_target::{InspectDiskContext, SectorRegion};
use crate::selectors::DeviceSelector;
use crate::sha256::sha256_hex;
use crate::sysinfo::{self, CmdRunner};

pub(crate) fn resolve_inspect_file(backup_dir: &Path, target: &str) -> Result<PathBuf, String> {
    let raw = Path::new(target);
    let candidate = if raw.components().count() == 1 {
        backup_dir.join(raw)
    } else if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(raw)
    };
    let path = fs::canonicalize(&candidate)
        .map_err(|e| format!("文件不存在或不可访问 {}: {}", candidate.display(), e))?;
    if !path.is_file() {
        return Err(format!("不是普通文件: {}", path.display()));
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("edpb") {
        return Err(format!("只接受 .edpb 备份: {}", path.display()));
    }
    Ok(path)
}

fn plain_hex(data: &[u8]) -> String {
    let mut out = String::new();
    for (line_no, line) in data.chunks(16).enumerate() {
        let base = line_no * 16;
        out.push_str(&format!("+0x{base:03X}: "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(&b) = line.get(i) {
                out.push_str(&format!("{b:02X} "));
            } else {
                out.push_str("   ");
            }
        }
        out.push(' ');
        for &b in line {
            out.push(if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            });
        }
        out.push('\n');
    }
    out
}

fn export_bytes(dir: &Path, lba: u64, suffix: &str, data: &[u8]) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let base = format!("LBA{lba}_{suffix}");
    fs::write(dir.join(format!("{base}.bin")), data)?;
    fs::write(dir.join(format!("{base}.hex")), plain_hex(data))?;
    Ok(())
}

fn export_meta(dir: &Path, lba: u64, text: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(format!("LBA{lba}_meta.txt")), text)
}

fn selected_lbas(opts: &InspectOpts) -> Vec<u64> {
    if opts.lbas.is_empty() {
        (0..crate::common::METADATA_SECTOR_COUNT as u64).collect()
    } else {
        opts.lbas.clone()
    }
}

fn region_labels(context: &InspectDiskContext, lba: u64) -> String {
    context
        .regions(lba)
        .iter()
        .map(SectorRegion::label)
        .collect::<Vec<_>>()
        .join("；")
}

fn protocol_view(
    context: &InspectDiskContext,
    lba: u64,
    raw: &[u8],
    meta: &InspectMeta,
) -> Result<inspect::SectorView, String> {
    let lba32 = u32::try_from(lba).map_err(|_| format!("LBA{lba} 超出协议解析器范围"))?;
    Ok(inspect::analyze_sector_with_context(
        lba32,
        raw,
        meta,
        Some(&context.protocol_image),
    ))
}

fn render_meta(
    context: &InspectDiskContext,
    meta: &InspectMeta,
    lba: u64,
    raw: &[u8],
    partition_boot_raw: Option<&[u8]>,
    partition_boot_issue: Option<&str>,
) -> Result<String, String> {
    let offset = lba
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| "LBA 字节偏移溢出".to_string())?;
    let mut out = format!(
        "LBA: {lba}\n物理字节偏移: {offset} (0x{offset:X})\nRAW SHA-256: {}\nRAW 非零字节: {}/512\n",
        sha256_hex(raw),
        raw.iter().filter(|&&byte| byte != 0).count()
    );
    let regions = context.regions(lba);
    out.push_str("区域:\n");
    for region in &regions {
        out.push_str(&format!("  - {}\n", region.label()));
    }
    if let Some(primary) = regions.first() {
        out.push_str(&format!("主区域: {}\n", primary.label()));
    }
    if regions.len() > 1 {
        out.push_str(&format!(
            "重叠区域: {}\n",
            regions[1..]
                .iter()
                .map(SectorRegion::label)
                .collect::<Vec<_>>()
                .join("；")
        ));
    } else {
        out.push_str("重叠区域: 无\n");
    }

    if lba <= u64::from(crate::common::METADATA_LAST_LBA) {
        let view = protocol_view(context, lba, raw, meta)?;
        out.push_str(&format!("协议解码: {}\n", view.method));
        out.push_str(&inspect::render_fields(&view));
        for note in &view.notes {
            out.push_str(&format!("  └─ {note}\n"));
        }
    }

    if let Some(partition) = context.partitions.iter().find(|partition| {
        lba >= partition.start_sector
            && lba
                < partition
                    .start_sector
                    .saturating_add(partition.sector_count)
    }) {
        out.push_str(&format!(
            "分区: index={} type={} relative_lba={} start={} sectors={}\n",
            partition.index,
            partition.partition_type,
            lba - partition.start_sector,
            partition.start_sector,
            partition.sector_count,
        ));
        out.push_str(&format!(
            "加密配置: NeedEncrypt={} EncryptMode={}\n",
            partition.need_encrypt, partition.encrypt_mode
        ));
        out.push_str(&format!(
            "MBR 直接暴露: {}\n",
            context.partition_mbr_exposure(partition)
        ));
        out.push_str(&format!(
            "密钥: FileKeyCRC=0x{:08X} 状态={}\n",
            partition.file_key_crc,
            context.partition_file_key_crc_status(partition)
        ));
        let state = match partition_boot_raw {
            Some(boot) => context.partition_physical_state(partition, boot),
            None => crate::inspect_target::PhysicalDataState::Unknown {
                reason: partition_boot_issue
                    .unwrap_or("缺少分区起始扇区证据")
                    .to_string(),
            },
        };
        out.push_str(&format!("物理数据状态: {}\n", state.label()));
        out.push_str(&format!("decode 策略: {}\n", state.decode_strategy()));
        out.push_str(&format!(
            "文件系统识别: {}\n",
            state
                .filesystem()
                .map(|filesystem| filesystem.label())
                .unwrap_or("未确认")
        ));
    }

    if let Some(lce) = &context.lce {
        if lba >= lce.start_lba && lba < lce.start_lba + lce.sector_count {
            out.push_str(&format!(
                "LCE: start={} sectors={} pointers={:?} mode={:?} chs_crosscheck={:?}\n",
                lce.start_lba,
                lce.sector_count,
                lce.lba7_pointer_entries
                    .iter()
                    .map(|pointer| (pointer.entry_index, pointer.partition_type))
                    .collect::<Vec<_>>(),
                lce.official_partition_mode,
                lce.chs_expected_start_lba
            ));
            out.push_str("LCE 解码: EDPSECDISK200709/A6B0，zero8，64 位物理字节偏移 tweak\n");
        }
    }
    for issue in &context.context_issues {
        out.push_str(&format!("上下文提示: {issue}\n"));
    }
    Ok(out)
}

fn print_inspect_meta(meta: &InspectMeta) {
    let mut parts = Vec::new();
    if let Some(id) = &meta.onlyid {
        parts.push(format!("onlyid={id}"));
    }
    if let (Some(v), Some(p)) = (&meta.vid, &meta.pid) {
        parts.push(format!("USB {v}:{p}"));
    }
    if let Some(size) = meta.size_bytes {
        parts.push(crate::common::fmt_gb(size));
    }
    if !parts.is_empty() {
        println!("{}  {}", crate::ui::bold("设备"), parts.join(" · "));
    }
    if let Some(did) = &meta.device_id {
        println!("{}  {}", crate::ui::bold("device_id"), did);
    } else {
        println!(
            "{}",
            crate::ui::yellow("device_id 未识别：LBA7/8/9/12 只能显示 RAW；可用 --id 手动指定")
        );
    }
}

fn render_inspect_source<F>(
    source_label: &str,
    meta: &InspectMeta,
    context: &InspectDiskContext,
    opts: &InspectOpts,
    mut read: F,
) -> i32
where
    F: FnMut(u64) -> io::Result<Vec<u8>>,
{
    println!("{}  {}", crate::ui::bold("来源"), source_label);
    print_inspect_meta(meta);
    println!(
        "{}  {}",
        crate::ui::bold("模式"),
        match opts.mode {
            InspectMode::Raw => "raw",
            InspectMode::Decode => "decode",
            InspectMode::Meta => "meta",
        }
    );

    let export_dir = opts.export.as_deref().map(PathBuf::from);
    for lba in selected_lbas(opts) {
        if let Err(error) = context.validate_lba(lba) {
            eprintln!("{}", crate::ui::red(&format!("错误: {error}")));
            return EXIT_TARGET;
        }
        let raw = match read(lba) {
            Ok(raw) if raw.len() == SECTOR => raw,
            Ok(raw) => {
                eprintln!(
                    "{}",
                    crate::ui::red(&format!(
                        "错误: LBA{lba} 返回 {}B，预期 {SECTOR}B",
                        raw.len()
                    ))
                );
                return EXIT_IO;
            }
            Err(error) => {
                eprintln!(
                    "{}",
                    crate::ui::red(&format!("错误: 读取 LBA{lba} 失败: {error}"))
                );
                return EXIT_IO;
            }
        };

        let mut partition_boot = None;
        let mut partition_boot_issue = None;
        if opts.mode != InspectMode::Raw {
            if let Some(partition) = context.partition_for_lba(lba) {
                if partition.start_sector == lba {
                    partition_boot = Some(raw.clone());
                } else {
                    match read(partition.start_sector) {
                        Ok(boot) if boot.len() == SECTOR => partition_boot = Some(boot),
                        Ok(boot) => {
                            partition_boot_issue = Some(format!(
                                "分区起始 LBA{} 只读取到 {}B",
                                partition.start_sector,
                                boot.len()
                            ));
                        }
                        Err(error) => {
                            partition_boot_issue = Some(format!(
                                "无法读取分区起始 LBA{}: {error}",
                                partition.start_sector
                            ));
                        }
                    }
                }
            }
        }

        println!();
        match opts.mode {
            InspectMode::Raw => {
                let offset = match lba.checked_mul(SECTOR as u64) {
                    Some(offset) => offset,
                    None => {
                        eprintln!("{}", crate::ui::red("错误: LBA 字节偏移溢出"));
                        return EXIT_TARGET;
                    }
                };
                println!(
                    "{}",
                    crate::ui::bold(&format!(
                        "LBA{lba}  offset=0x{offset:X}  区域={}  SHA-256={}  非零={}/512",
                        region_labels(context, lba),
                        sha256_hex(&raw),
                        raw.iter().filter(|&&byte| byte != 0).count()
                    ))
                );
                print!("{}", plain_hex(&raw));
                if let Some(dir) = &export_dir {
                    if let Err(error) = export_bytes(dir, lba, "raw", &raw) {
                        eprintln!("{}", crate::ui::red(&format!("错误: 导出失败: {error}")));
                        return EXIT_IO;
                    }
                }
            }
            InspectMode::Decode => {
                let result = if lba <= u64::from(crate::common::METADATA_LAST_LBA) {
                    protocol_view(context, lba, &raw, meta).map(|view| (view.decoded, view.method))
                } else {
                    context.decode_non_protocol_with_boot(lba, &raw, partition_boot.as_deref())
                };
                let (decoded, method) = match result {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!("{}", crate::ui::red(&format!("错误: {error}")));
                        return EXIT_TARGET;
                    }
                };
                println!("{}  {}", crate::ui::bold(&format!("LBA{lba}")), method);
                println!(
                    "  区域: {}  decoded SHA-256={}",
                    region_labels(context, lba),
                    sha256_hex(&decoded)
                );
                print!("{}", plain_hex(&decoded));
                if let Some(dir) = &export_dir {
                    if let Err(error) = export_bytes(dir, lba, "decoded", &decoded) {
                        eprintln!("{}", crate::ui::red(&format!("错误: 导出失败: {error}")));
                        return EXIT_IO;
                    }
                }
            }
            InspectMode::Meta => {
                let text = match render_meta(
                    context,
                    meta,
                    lba,
                    &raw,
                    partition_boot.as_deref(),
                    partition_boot_issue.as_deref(),
                ) {
                    Ok(text) => text,
                    Err(error) => {
                        eprintln!("{}", crate::ui::red(&format!("错误: {error}")));
                        return EXIT_TARGET;
                    }
                };
                print!("{text}");
                if let Some(dir) = &export_dir {
                    if let Err(error) = export_meta(dir, lba, &text) {
                        eprintln!("{}", crate::ui::red(&format!("错误: 导出失败: {error}")));
                        return EXIT_IO;
                    }
                }
            }
        }
    }
    if let Some(dir) = &export_dir {
        println!();
        println!("{}  {}", crate::ui::green("已导出"), dir.display());
    }
    EXIT_OK
}

struct BackupSectorReader {
    path: PathBuf,
    manifest: crate::edpb::Manifest,
    protocol: Vec<u8>,
    cache: BTreeMap<String, Vec<u8>>,
}

impl BackupSectorReader {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        if lba < crate::common::METADATA_SECTOR_COUNT as u64 {
            let start = usize::try_from(lba).unwrap() * SECTOR;
            return Ok(self.protocol[start..start + SECTOR].to_vec());
        }

        let found = self.manifest.extents.iter().find_map(|extent| {
            let end = extent.start_lba.checked_add(extent.sector_count)?;
            if lba < extent.start_lba || lba >= end {
                return None;
            }
            let artifact = self.manifest.artifacts.iter().find(|artifact| {
                artifact.kind == "raw_sectors"
                    && artifact.source_extent_ids.iter().any(|id| id == &extent.id)
            })?;
            Some((extent.start_lba, artifact.id.clone()))
        });
        let Some((start_lba, artifact_id)) = found else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("EDPB 未采集 LBA{lba} 的原始扇区"),
            ));
        };

        if !self.cache.contains_key(&artifact_id) {
            let data =
                crate::edpb::read_artifact(&self.path, &artifact_id).map_err(io::Error::other)?;
            self.cache.insert(artifact_id.clone(), data);
        }
        let data = self
            .cache
            .get(&artifact_id)
            .expect("刚插入的 Artifact 必须存在");
        let offset = usize::try_from(lba - start_lba)
            .ok()
            .and_then(|sector| sector.checked_mul(SECTOR))
            .ok_or_else(|| io::Error::other("EDPB Artifact 扇区偏移溢出"))?;
        data.get(offset..offset + SECTOR)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "EDPB Artifact 截断"))
    }
}

fn inspect_backup_flow(opts: InspectOpts) -> i32 {
    let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
    let Some(target) = opts.backup.as_deref() else {
        eprintln!("{}", crate::ui::red("错误: inspect 缺少备份文件来源"));
        return EXIT_USAGE;
    };
    let path = match resolve_inspect_file(&bak, target) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {message}")));
            return EXIT_BACKUP;
        }
    };
    let verified = match crate::edpb::verify_file(&path) {
        Ok(container) => container,
        Err(message) => {
            eprintln!(
                "{}",
                crate::ui::red(&format!(
                    "错误: EDPB 校验失败 {}: {message}",
                    path.display()
                ))
            );
            return EXIT_BACKUP;
        }
    };
    let protocol = match crate::edpb::read_raw_protocol(&path) {
        Ok(data) => data,
        Err(message) => {
            eprintln!(
                "{}",
                crate::ui::red(&format!("错误: 读取 EDPB LBA0-LBA12 失败: {message}"))
            );
            return EXIT_BACKUP;
        }
    };
    let mut meta = InspectMeta {
        device_id: Some(verified.manifest.device.device_id.clone()),
        vid: Some(verified.manifest.device.vid.clone()),
        pid: Some(verified.manifest.device.pid.clone()),
        size_bytes: verified.manifest.geometry.capacity_bytes,
        onlyid: verified.manifest.device.onlyid.clone(),
    };
    if let Some(did) = &opts.device_id {
        meta.device_id = Some(did.clone());
    }
    let Some(total_sectors) = verified.manifest.geometry.total_sectors else {
        eprintln!(
            "{}",
            crate::ui::red("错误: EDPB 缺少 total_sectors，无法校验任意 LBA")
        );
        return EXIT_BACKUP;
    };
    let context = InspectDiskContext::new(protocol.clone(), meta.device_id.clone(), total_sectors);
    let mut reader = BackupSectorReader {
        path: path.clone(),
        manifest: verified.manifest,
        protocol,
        cache: BTreeMap::new(),
    };
    render_inspect_source(&path.display().to_string(), &meta, &context, &opts, |lba| {
        reader.read_sector(lba)
    })
}

fn inspect_disk_flow(runner: &dyn CmdRunner, mut opts: InspectOpts) -> i32 {
    let selector = DeviceSelector::new(opts.disk);
    if !elevate::is_root() {
        let mut argv: Vec<String> = std::env::args().skip(1).collect();
        let mut prompt = StdPrompter;
        let n = match selector.resolve(runner, &mut prompt) {
            Ok(n) => n,
            Err(error) => {
                eprintln!("{}", crate::ui::red(&error.msg));
                return error.code;
            }
        };
        selector.pin_argv(&mut argv, n);
        elevate::ensure_elevated(&argv);
        unreachable!();
    }

    let mut prompt = StdPrompter;
    let n = match selector.resolve(runner, &mut prompt) {
        Ok(n) => n,
        Err(error) => {
            eprintln!("{}", crate::ui::red(&error.msg));
            return error.code;
        }
    };
    opts.disk = Some(n);
    let path = raw_path(n);
    let mut dev = match FileDev::open_rdonly(&path) {
        Ok(dev) => dev,
        Err(error) => {
            eprintln!(
                "{}",
                crate::ui::red(&format!("错误: 无法只读打开 disk{n}: {error}"))
            );
            return EXIT_IO;
        }
    };

    let Some(total_sectors) = sysinfo::disk_total_sectors(runner, n) else {
        eprintln!("{}", crate::ui::red("错误: 无法取得设备总扇区数"));
        return EXIT_TARGET;
    };

    let mut protocol = Vec::with_capacity(crate::common::METADATA_IMAGE_LEN);
    for lba in 0..crate::common::METADATA_SECTOR_COUNT as u64 {
        match dev.read_sector_u64(lba) {
            Ok(raw) if raw.len() == SECTOR => protocol.extend_from_slice(&raw),
            Ok(raw) => {
                eprintln!(
                    "{}",
                    crate::ui::red(&format!("错误: LBA{lba} 只读取到 {}B", raw.len()))
                );
                return EXIT_IO;
            }
            Err(error) => {
                eprintln!(
                    "{}",
                    crate::ui::red(&format!("错误: 读取协议上下文 LBA{lba} 失败: {error}"))
                );
                return EXIT_IO;
            }
        }
    }

    let raw7 = &protocol[7 * SECTOR..8 * SECTOR];
    let id = identify(runner, n, raw7).device_id;
    let (vid, pid) = sysinfo::usb_vid_pid(runner, n);
    let mut meta = InspectMeta {
        device_id: id,
        vid: (vid != "xxxx").then_some(vid),
        pid: (pid != "xxxx").then_some(pid),
        size_bytes: total_sectors.checked_mul(SECTOR as u64),
        onlyid: diskio::lba4_label_id_from(&protocol[4 * SECTOR..5 * SECTOR]),
    };
    if let Some(did) = &opts.device_id {
        meta.device_id = Some(did.clone());
    }

    let context = InspectDiskContext::new(protocol, meta.device_id.clone(), total_sectors);
    render_inspect_source(
        &format!("物理盘 disk{n} ({path})"),
        &meta,
        &context,
        &opts,
        |lba| dev.read_sector_u64(lba),
    )
}

pub(crate) fn inspect_flow(runner: &dyn CmdRunner, opts: InspectOpts) -> i32 {
    if opts.backup.is_some() {
        inspect_backup_flow(opts)
    } else {
        inspect_disk_flow(runner, opts)
    }
}
