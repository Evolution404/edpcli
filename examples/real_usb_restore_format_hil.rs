//! Explicitly enabled destructive restore/format HIL on a serial-pinned external USB disk.
use edpcli::application::device::guard_usb_disk;
use edpcli::application::filesystem::FilesystemKind;
use edpcli::application::post_restore::{self, PartitionFormatRequest};
use edpcli::application::progress::{FormatStep, ProgressEvent, Step};
use edpcli::application::write::restore_on_disk_typed;
use edpcli::application::Prompter;
use edpcli::platform::system::SysRunner;
use edpcli::ports::CmdRunner;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Default)]
struct Progress {
    stages: BTreeSet<String>,
    events: usize,
    completed: bool,
    saw_write: bool,
    saw_readback: bool,
    work_complete: bool,
}
impl Prompter for Progress {
    fn prompt_line(&mut self, _: &str) -> String {
        String::new()
    }
    fn confirm_yes(&mut self, _: &str) -> bool {
        true
    }
    fn operation_progress(&mut self, event: ProgressEvent) {
        self.events += 1;
        let stage = format!("{:?}", event.step);
        if self.stages.insert(stage.clone()) {
            println!("[PROGRESS] {stage}");
        }
        self.completed |= event.step == Step::Completed;
        self.saw_write |= event.step == Step::PostRestoreFormat(FormatStep::Write);
        self.saw_readback |= event.step == Step::PostRestoreFormat(FormatStep::Readback);
        if let Some(work) = event.work {
            assert!(work.current <= work.total);
            self.work_complete |= work.total > 0 && work.current == work.total;
        }
    }
}

fn pin(runner: &SysRunner, disk: u32, expected: &str) -> Result<(), String> {
    guard_usb_disk(runner, disk).map_err(|error| error.msg)?;
    let serial = runner
        .hardware_serial(disk)
        .ok_or("Hardware serial missing; HIL refused")?;
    let digest = edpcli::edpb::sha256_hex(serial.as_bytes());
    if !digest.eq_ignore_ascii_case(expected) {
        return Err("Hardware serial changed; HIL refused".into());
    }
    Ok(())
}

fn invalidate_filesystems(
    runner: &SysRunner,
    disk: u32,
    expected: &str,
    backup: &std::path::Path,
    only_boot: bool,
) -> Result<(), String> {
    use edpcli::application::support::{EdpCliError, EXIT_TARGET, SECTOR};
    use edpcli::application::target_session::{ReadOnly, TargetSession};
    use edpcli::diskio::{raw_path, FileDev, SectorWriteStage, WriteTransactionPlan};
    use edpcli::ports::SectorDev;
    use std::time::Duration;
    pin(runner, disk, expected)?;
    let verified = edpcli::edpb::VerifiedBackupReader::open(backup)?;
    let manifest = &verified.verified().manifest;
    let total = edpcli::platform::system::disk_total_sectors(runner, disk)
        .ok_or("Target geometry missing")?;
    if manifest.geometry.total_sectors != Some(total) {
        return Err("Backup geometry differs from target".into());
    }
    let mut dev = FileDev::open_rdonly(&raw_path(disk)).map_err(|error| error.to_string())?;
    let protocol = (0..13)
        .map(|lba| dev.read_sector(lba))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .concat();
    let partitions = edpcli::application::backup::parse_partition_geometry(
        &protocol,
        &manifest.device.device_id,
        total,
    )?;
    let mut plan = WriteTransactionPlan::new(total);
    for partition in &manifest.partitions {
        if only_boot && partition.index != 1 {
            continue;
        }
        if partition.start_lba < 13
            || partition.sector_count < 24
            || !partitions.iter().any(|live| {
                live.index.checked_add(1) == usize::try_from(partition.index).ok()
                    && live.start_sector == partition.start_lba
                    && live.sector_count == partition.sector_count
            })
        {
            return Err("HIL filesystem range is not the current partition".into());
        }
        for offset in 0..24 {
            plan.insert(
                u32::try_from(partition.start_lba + offset).map_err(|_| "HIL range exceeds u32")?,
                vec![0; SECTOR],
                SectorWriteStage::Metadata,
                "explicit HIL filesystem damage",
            )?;
        }
    }
    if plan.writes().is_empty() {
        return Err("No filesystem headers to invalidate".into());
    }
    let session = TargetSession::<ReadOnly>::open_usb(runner, disk).map_err(|error| error.msg)?;
    let prepared = session.prepare_write().map_err(|error| error.to_string())?;
    let mut locked = prepared
        .reopen_and_verify(&mut dev, Duration::from_secs(10), |dev| {
            pin(runner, disk, expected)
                .map_err(|message| EdpCliError::new(EXIT_TARGET, message))?;
            for lba in 0..13 {
                let actual = dev
                    .read_sector(lba)
                    .map_err(|error| EdpCliError::new(EXIT_TARGET, error.to_string()))?;
                if actual != protocol[lba as usize * SECTOR..(lba as usize + 1) * SECTOR] {
                    return Err(EdpCliError::new(
                        EXIT_TARGET,
                        "Target metadata changed before HIL",
                    ));
                }
            }
            Ok(())
        })
        .map_err(|error| format!("HIL reopen guard: {error:?}"))?;
    locked
        .execute_transaction(&plan)
        .map_err(|error| error.msg)?;
    println!("[PASS] explicit filesystem-header damage written and readback verified");
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "--probe" {
        let disk = edpcli::platform::parse_disk_selector(&args[1])?;
        let runner = SysRunner;
        guard_usb_disk(&runner, disk).map_err(|error| error.msg)?;
        let serial = runner
            .hardware_serial(disk)
            .ok_or("Hardware serial missing")?;
        let probe = runner
            .hardware_probe(disk)
            .ok_or("Hardware probe missing")?;
        println!(
            "disk={disk} total_sectors={:?} vid={:?} pid={:?} inquiry={:?} serial_sha256={}",
            edpcli::platform::system::disk_total_sectors(&runner, disk),
            probe.vid,
            probe.pid,
            probe.inquiry,
            edpcli::edpb::sha256_hex(serial.as_bytes())
        );
        return Ok(());
    }
    if args.len() != 3
        && !(args.len() == 4
            && matches!(
                args[3].as_str(),
                "--invalidate-filesystems" | "--damage-boot-only"
            ))
    {
        return Err(
            "usage: --probe DISK | DISK SERIAL_SHA256 BACKUP.edpb [--invalidate-filesystems|--damage-boot-only]"
                .into(),
        );
    }
    if std::env::var("EDPCLI_REAL_USB_RESTORE_FORMAT_HIL").as_deref() != Ok("YES") {
        return Err("EDPCLI_REAL_USB_RESTORE_FORMAT_HIL=YES is required".into());
    }
    if args[1].len() != 64 || !args[1].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("A 64-digit expected serial SHA-256 is required".into());
    }
    let disk = edpcli::platform::parse_disk_selector(&args[0])?;
    let backup =
        std::fs::canonicalize(PathBuf::from(&args[2])).map_err(|error| error.to_string())?;
    let backup_dir = backup
        .parent()
        .ok_or("Backup parent missing")?
        .to_path_buf();
    let runner = SysRunner;
    pin(&runner, disk, &args[1])?;
    if args.len() == 4 {
        invalidate_filesystems(
            &runner,
            disk,
            &args[1],
            &backup,
            args[3] == "--damage-boot-only",
        )?;
        if args[3] == "--damage-boot-only" {
            return pin(&runner, disk, &args[1]);
        }
    }
    let mut progress = Progress::default();
    let outcome = restore_on_disk_typed(
        &runner,
        Some(backup.display().to_string()),
        disk,
        backup_dir,
        &mut progress,
        None,
        None,
    )
    .map_err(|error| error.msg)?;
    println!(
        "[PASS] metadata restore: {} partitions",
        outcome.partitions.len()
    );
    if outcome.partitions.is_empty() {
        return Err("No partitions available for formatting".into());
    }
    for partition in &outcome.assessment.partitions {
        pin(&runner, disk, &args[1])?;
        let filesystem = if partition.index == 1 {
            FilesystemKind::Fat16
        } else {
            FilesystemKind::ExFat
        };
        let request = PartitionFormatRequest {
            partition_index: partition.index,
            filesystem,
        };
        let mut progress = Progress::default();
        if partition.requires_original_key {
            post_restore::format_encrypted_partition_after_restore_on_disk(
                &runner,
                disk,
                &mut progress,
                &outcome,
                &request,
                None,
                "HILCRYPT",
            )
            .result
            .map_err(|error| format!("Encrypted format failed: {error:?}"))?;
        } else {
            post_restore::format_partition_after_restore_on_disk(
                &runner,
                disk,
                &mut progress,
                &outcome,
                &request,
                "HILBOOT",
            )
            .result
            .map_err(|error| error.to_string())?;
        }
        if !(progress.completed
            && progress.saw_write
            && progress.saw_readback
            && progress.work_complete)
        {
            return Err(format!(
                "Partition {} did not emit complete write/readback progress",
                partition.index
            ));
        }
        println!(
            "[PASS] partition {} {:?}: filesystem/readback verified, {} progress events",
            partition.index, filesystem, progress.events
        );
    }
    pin(&runner, disk, &args[1])?;
    println!("[PASS] hardware identity unchanged after restore/format");
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("HIL failed: {error}");
        std::process::exit(1);
    }
}
