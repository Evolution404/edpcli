#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("real_usb_password_hil 目前只支持 macOS");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::env;
    use std::fs::OpenOptions;
    use std::io::{self, BufRead, BufReader, Write};
    use std::process::Command;

    use edpcli::application::device::guard_usb_disk;
    use edpcli::application::provision::{
        commit_provision_with_backup_on_disk, prepare_provision_on_disk, FormatOptions,
        OfficialProvisionRequest, ProvisionCommitOutcome, ProvisionRequest,
    };
    use edpcli::application::Prompter;
    use edpcli::common::SECTOR;
    use edpcli::diskio::{raw_path, FileDev, SectorDev};
    use edpcli::provision::{
        parse_existing_provision, KeyDomainRole, KeyDomainSecretPair, KeyDomainSecrets,
        OfficialFilesystemFormat, OfficialPartitionMode, PartitionAction, PartitionRole,
        ProvisionImage, ProvisionTarget, RegionDisposition, SourcePasswordKnowledge,
        TargetIdentity, DEFAULT_KEY_DOMAIN_PASSWORD, DEFAULT_SAFE6_LABEL,
    };
    use edpcli::sysinfo::{disk_total_sectors, CmdRunner, SysRunner};
    use sha2::{Digest, Sha256};

    const ENABLE_ENV: &str = "EDPCLI_REAL_USB_PASSWORD_HIL";
    const EXPECTED_VID: u16 = 0x3535;
    const EXPECTED_PID: u16 = 0x6300;
    const EXPECTED_TOTAL_SECTORS: u64 = 15_728_640;
    const EXPECTED_DEVICE_ID: &str = "disk&ven_aigo&prod_u335&rev_1100";

    const BOOT_START: u64 = 63;
    const BOOT_SECTORS: u64 = 20_417;
    const SHARE_START: u64 = 20_480;
    const SHARE_SECTORS: u64 = 13_606_912;
    const ENCRYPT_START: u64 = 13_627_392;
    const ENCRYPT_SECTORS: u64 = 2_097_152;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Stage {
        All,
        Build,
        Rewrap,
    }

    struct Args {
        disk: u32,
        probe: bool,
        stage: Stage,
        stdin_secrets: bool,
        generate_secrets: bool,
        expected_serial_sha256: Option<String>,
    }

    struct SecretBundle {
        share_v1: Vec<u8>,
        encrypt_v1: Vec<u8>,
        share_v2: Vec<u8>,
        encrypt_v2: Vec<u8>,
    }

    impl SecretBundle {
        fn validate(&self) -> Result<(), String> {
            for (name, value) in [
                ("第一阶段交换域", self.share_v1.as_slice()),
                ("第一阶段保密域", self.encrypt_v1.as_slice()),
                ("第二阶段交换域", self.share_v2.as_slice()),
                ("第二阶段保密域", self.encrypt_v2.as_slice()),
            ] {
                if value.is_empty() {
                    return Err(format!("{name}密码不能为空"));
                }
                if value == DEFAULT_KEY_DOMAIN_PASSWORD {
                    return Err(format!("{name}必须使用非默认测试密码"));
                }
            }
            if self.share_v1 == self.encrypt_v1 {
                return Err("第一阶段 Share/Encrypt 密码必须不同".into());
            }
            if self.share_v2 == self.encrypt_v2 {
                return Err("第二阶段 Share/Encrypt 密码必须不同".into());
            }
            if self.share_v1 == self.share_v2 {
                return Err("Share rewrap 的新旧密码必须不同".into());
            }
            if self.encrypt_v1 == self.encrypt_v2 {
                return Err("Encrypt rewrap 的新旧密码必须不同".into());
            }
            Ok(())
        }
    }

    impl Drop for SecretBundle {
        fn drop(&mut self) {
            self.share_v1.fill(0);
            self.encrypt_v1.fill(0);
            self.share_v2.fill(0);
            self.encrypt_v2.fill(0);
        }
    }

    struct TtyEchoGuard;

    impl TtyEchoGuard {
        fn disable() -> Result<Self, String> {
            let status = Command::new("/bin/stty")
                .args(["-f", "/dev/tty", "-echo"])
                .status()
                .map_err(|error| format!("无法关闭 /dev/tty 回显: {error}"))?;
            if !status.success() {
                return Err("无法关闭 /dev/tty 回显".into());
            }
            Ok(Self)
        }
    }

    impl Drop for TtyEchoGuard {
        fn drop(&mut self) {
            let _ = Command::new("/bin/stty")
                .args(["-f", "/dev/tty", "echo"])
                .status();
        }
    }

    struct HilPrompter;

    impl Prompter for HilPrompter {
        fn prompt_line(&mut self, _msg: &str) -> String {
            String::new()
        }

        fn confirm_yes(&mut self, _msg: &str) -> bool {
            true
        }
    }

    #[derive(Clone, Debug)]
    struct DomainEvidence {
        raw_file_key: [u8; 16],
        lba7_wrapper: Vec<u8>,
        lba12_wrapper: Vec<u8>,
        first_sector_sha256: String,
    }

    fn parse_args() -> Result<Args, String> {
        let mut disk = None;
        let mut probe = false;
        let mut stage = Stage::All;
        let mut stdin_secrets = false;
        let mut generate_secrets = false;
        let mut expected_serial_sha256 = None;
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--disk" => {
                    disk = Some(
                        args.next()
                            .ok_or_else(|| "--disk 缺少编号".to_string())?
                            .parse()
                            .map_err(|_| "--disk 必须是整数".to_string())?,
                    );
                }
                "--probe" => probe = true,
                "--stdin-secrets" => stdin_secrets = true,
                "--generate-secrets" => generate_secrets = true,
                "--stage" => {
                    stage = match args
                        .next()
                        .ok_or_else(|| "--stage 缺少 all/build/rewrap".to_string())?
                        .as_str()
                    {
                        "all" => Stage::All,
                        "build" => Stage::Build,
                        "rewrap" => Stage::Rewrap,
                        other => return Err(format!("未知 --stage: {other}")),
                    };
                }
                "--expected-serial-sha256" => {
                    expected_serial_sha256 = Some(
                        args.next()
                            .ok_or_else(|| "--expected-serial-sha256 缺少值".to_string())?,
                    );
                }
                other => return Err(format!("未知参数: {other}")),
            }
        }
        if stdin_secrets && generate_secrets {
            return Err("--stdin-secrets 与 --generate-secrets 不能同时使用".into());
        }
        Ok(Args {
            disk: disk.ok_or_else(|| "必须显式指定 --disk N".to_string())?,
            probe,
            stage,
            stdin_secrets,
            generate_secrets,
            expected_serial_sha256,
        })
    }

    fn sha256_hex(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }

    fn probe_target(
        runner: &SysRunner,
        disk: u32,
        expected_serial_sha256: Option<&str>,
    ) -> Result<(TargetIdentity, String), String> {
        guard_usb_disk(runner, disk).map_err(|error| error.msg)?;
        let total = disk_total_sectors(runner, disk)
            .ok_or_else(|| format!("无法读取 disk{disk} 总扇区数"))?;
        let probe = runner
            .hardware_probe(disk)
            .ok_or_else(|| format!("无法读取 disk{disk} USB/SCSI 硬件身份"))?;
        let identity = TargetIdentity::from_probe(&probe, total)
            .map_err(|error| format!("无法构造硬件身份: {error}"))?;
        if identity.vid() != EXPECTED_VID
            || identity.pid() != EXPECTED_PID
            || identity.total_sectors() != EXPECTED_TOTAL_SECTORS
            || identity.device_id() != EXPECTED_DEVICE_ID
        {
            return Err(format!(
                "真实盘 HIL 身份不匹配：VID:PID={:04x}:{:04x} sectors={} device_id={}",
                identity.vid(),
                identity.pid(),
                identity.total_sectors(),
                identity.device_id()
            ));
        }
        let serial = runner
            .hardware_serial(disk)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("无法读取 disk{disk} 硬件序列号"))?;
        let serial_sha256 = sha256_hex(serial.as_bytes());
        if let Some(expected) = expected_serial_sha256 {
            if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("--expected-serial-sha256 必须是 64 位十六进制".into());
            }
            if !serial_sha256.eq_ignore_ascii_case(expected) {
                return Err("硬件 serial SHA-256 与显式绑定值不一致，拒绝真实写盘".into());
            }
        }
        Ok((identity, serial_sha256))
    }

    fn strip_line_end(mut line: String) -> Vec<u8> {
        while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        line.into_bytes()
    }

    fn read_four_lines<R: BufRead>(reader: &mut R) -> Result<SecretBundle, String> {
        fn next<R: BufRead>(reader: &mut R, label: &str) -> Result<Vec<u8>, String> {
            let mut line = String::new();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| format!("读取{label}失败: {error}"))?;
            if read == 0 {
                return Err(format!("读取{label}时遇到 EOF"));
            }
            Ok(strip_line_end(line))
        }

        let secrets = SecretBundle {
            share_v1: next(reader, "第一阶段 Share 密码")?,
            encrypt_v1: next(reader, "第一阶段 Encrypt 密码")?,
            share_v2: next(reader, "第二阶段 Share 密码")?,
            encrypt_v2: next(reader, "第二阶段 Encrypt 密码")?,
        };
        secrets.validate()?;
        Ok(secrets)
    }

    fn generated_secret(marker: u8) -> Result<Vec<u8>, String> {
        const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
        let mut entropy = [0u8; 20];
        getrandom::fill(&mut entropy)
            .map_err(|error| format!("生成一次性 HIL 密码失败: {error}"))?;
        let mut secret = Vec::with_capacity(25);
        secret.extend_from_slice(&[b'A', b'a', b'7', b'!', marker]);
        for byte in entropy {
            secret.push(ALPHABET[byte as usize % ALPHABET.len()]);
        }
        Ok(secret)
    }

    fn generated_secret_bundle() -> Result<SecretBundle, String> {
        let secrets = SecretBundle {
            share_v1: generated_secret(b'Q')?,
            encrypt_v1: generated_secret(b'R')?,
            share_v2: generated_secret(b'S')?,
            encrypt_v2: generated_secret(b'T')?,
        };
        secrets.validate()?;
        Ok(secrets)
    }

    fn read_tty_secret<R: BufRead, W: Write>(
        reader: &mut R,
        writer: &mut W,
        prompt: &str,
    ) -> Result<Vec<u8>, String> {
        writer
            .write_all(prompt.as_bytes())
            .and_then(|_| writer.flush())
            .map_err(|error| format!("写入 /dev/tty 提示失败: {error}"))?;
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| format!("读取 /dev/tty 密码失败: {error}"))?;
        writer
            .write_all(b"\n")
            .and_then(|_| writer.flush())
            .map_err(|error| format!("写入 /dev/tty 换行失败: {error}"))?;
        if read == 0 {
            return Err("从 /dev/tty 读取密码时遇到 EOF".into());
        }
        Ok(strip_line_end(line))
    }

    fn read_secrets(stdin_secrets: bool, generate_secrets: bool) -> Result<SecretBundle, String> {
        if generate_secrets {
            return generated_secret_bundle();
        }
        if stdin_secrets {
            let stdin = io::stdin();
            return read_four_lines(&mut stdin.lock());
        }

        let mut tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|error| format!("无法打开 /dev/tty: {error}"))?;
        let mut reader = BufReader::new(
            tty.try_clone()
                .map_err(|error| format!("无法复制 /dev/tty: {error}"))?,
        );
        let _echo = TtyEchoGuard::disable()?;
        let secrets = SecretBundle {
            share_v1: read_tty_secret(&mut reader, &mut tty, "第一阶段 Share 测试密码: ")?,
            encrypt_v1: read_tty_secret(&mut reader, &mut tty, "第一阶段 Encrypt 测试密码: ")?,
            share_v2: read_tty_secret(&mut reader, &mut tty, "第二阶段 Share 新密码: ")?,
            encrypt_v2: read_tty_secret(&mut reader, &mut tty, "第二阶段 Encrypt 新密码: ")?,
        };
        secrets.validate()?;
        Ok(secrets)
    }

    fn format_options(selected: bool) -> FormatOptions {
        FormatOptions {
            boot: selected,
            share: selected,
            encrypt: selected,
            boot_label: "BOOT".into(),
            share_label: "SHARE".into(),
            encrypt_label: "ENCRYPT".into(),
            boot_fs: OfficialFilesystemFormat::Fat16,
            share_fs: OfficialFilesystemFormat::ExFat,
            encrypt_fs: OfficialFilesystemFormat::ExFat,
        }
    }

    fn mode0_request(
        share_source: Option<&[u8]>,
        share_target: &[u8],
        encrypt_source: Option<&[u8]>,
        encrypt_target: &[u8],
        rebuild_filesystems: bool,
    ) -> ProvisionRequest {
        ProvisionRequest::Official(Box::new(OfficialProvisionRequest {
            target: ProvisionTarget::Official(OfficialPartitionMode::DefaultThreePartition),
            boot_start_lba: Some(BOOT_START),
            share_start_lba: Some(SHARE_START),
            encrypt_start_lba: Some(ENCRYPT_START),
            boot_mib: None,
            boot_sectors: Some(BOOT_SECTORS),
            share_mib: None,
            share_sectors: Some(SHARE_SECTORS),
            encrypt_mib: None,
            encrypt_sectors: Some(ENCRYPT_SECTORS),
            label_id: String::new(),
            user: "TEST".into(),
            dept: "TEST".into(),
            label: DEFAULT_SAFE6_LABEL.into(),
            key_domains: KeyDomainSecrets::new(
                KeyDomainSecretPair::new(share_source, Some(share_target)),
                KeyDomainSecretPair::new(encrypt_source, Some(encrypt_target)),
            ),
            volume_label: "HIL".into(),
            format: format_options(rebuild_filesystems),
            force_change_password: Some(false),
            cancel_password_complexity_check: Some(false),
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
        }))
    }

    fn read_protocol_image(dev: &mut dyn SectorDev) -> Result<ProvisionImage, String> {
        let mut bytes = Vec::with_capacity(13 * SECTOR);
        for lba in 0..13u32 {
            bytes.extend(
                dev.read_sector(lba)
                    .map_err(|error| format!("读取 LBA{lba} 失败: {error}"))?,
            );
        }
        ProvisionImage::from_bytes(bytes).map_err(|error| format!("协议镜像无效: {error}"))
    }

    fn current_parsed_mode0(
        disk: u32,
        device_id: &str,
        total_sectors: u64,
    ) -> Result<edpcli::provision::ParsedExistingProvision, String> {
        let mut dev = FileDev::open_rdonly(&raw_path(disk))
            .map_err(|error| format!("只读打开 disk{disk} 失败: {error}"))?;
        let image = read_protocol_image(&mut dev)?;
        let parsed = parse_existing_provision(&image, device_id, total_sectors)
            .map_err(|error| format!("解析当前 EDP 协议失败: {error}"))?
            .ok_or_else(|| "当前盘不是可验证的 EDP 注册盘".to_string())?;
        if parsed.profile.source_mode != OfficialPartitionMode::DefaultThreePartition {
            return Err(format!(
                "当前盘不是 mode0，实际为 {:?}",
                parsed.profile.source_mode
            ));
        }
        for (role, start, sectors) in [
            (PartitionRole::Boot, BOOT_START, BOOT_SECTORS),
            (PartitionRole::Share, SHARE_START, SHARE_SECTORS),
            (PartitionRole::Encrypt, ENCRYPT_START, ENCRYPT_SECTORS),
        ] {
            let part = parsed
                .profile
                .partition(role)
                .ok_or_else(|| format!("mode0 缺少 {} 分区", role.label()))?;
            if part.start_lba != start || part.sector_count != sectors {
                return Err(format!(
                    "{} 几何不匹配：{}+{}，预期 {}+{}",
                    role.label(),
                    part.start_lba,
                    part.sector_count,
                    start,
                    sectors
                ));
            }
        }
        Ok(parsed)
    }

    fn require_plain_source(disk: u32, device_id: &str, total_sectors: u64) -> Result<(), String> {
        let mut dev = FileDev::open_rdonly(&raw_path(disk))
            .map_err(|error| format!("只读打开 disk{disk} 失败: {error}"))?;
        let image = read_protocol_image(&mut dev)?;
        if image.as_bytes()[4 * SECTOR..5 * SECTOR]
            .iter()
            .any(|&byte| byte != 0)
        {
            return Err("build 阶段要求 Plain，当前 LBA4 非零；拒绝覆盖疑似 EDP 身份".into());
        }
        if parse_existing_provision(&image, device_id, total_sectors)
            .map_err(|error| format!("Plain 前置解析失败: {error}"))?
            .is_some()
        {
            return Err("build 阶段要求 Plain，当前仍可解析为 EDP 注册盘".into());
        }
        Ok(())
    }

    fn strict_exfat_boot(boot: &[u8], start_lba: u64, sector_count: u64) -> bool {
        if boot.len() != SECTOR
            || boot[510..512] != [0x55, 0xaa]
            || !((boot[0] == 0xeb && boot[2] == 0x90) || boot[0] == 0xe9)
            || boot.get(3..11) != Some(b"EXFAT   ")
            || boot[11..64].iter().any(|&byte| byte != 0)
        {
            return false;
        }
        let u32le = |offset: usize| {
            u32::from_le_bytes(boot[offset..offset + 4].try_into().expect("bounded u32"))
        };
        let u64le = |offset: usize| {
            u64::from_le_bytes(boot[offset..offset + 8].try_into().expect("bounded u64"))
        };
        let partition_offset = u64le(64);
        let volume_length = u64le(72);
        let fat_offset = u32le(80) as u64;
        let fat_length = u32le(84) as u64;
        let heap_offset = u32le(88) as u64;
        let cluster_count = u32le(92) as u64;
        let root_cluster = u32le(96) as u64;
        let bps_shift = boot[108];
        let spc_shift = boot[109];
        let fats = boot[110] as u64;
        let percent_in_use = boot[112];
        let Some(spc) = 1u64.checked_shl(spc_shift as u32) else {
            return false;
        };
        let Some(fat_end) = fat_length
            .checked_mul(fats)
            .and_then(|length| fat_offset.checked_add(length))
        else {
            return false;
        };
        let Some(heap_end) = cluster_count
            .checked_mul(spc)
            .and_then(|length| heap_offset.checked_add(length))
        else {
            return false;
        };
        partition_offset == start_lba
            && volume_length != 0
            && volume_length <= sector_count
            && bps_shift == 9
            && spc_shift < 26
            && matches!(fats, 1 | 2)
            && (percent_in_use <= 100 || percent_in_use == 0xff)
            && fat_offset >= 24
            && fat_length > 0
            && heap_offset >= fat_end
            && cluster_count > 0
            && root_cluster >= 2
            && root_cluster < cluster_count + 2
            && heap_end <= volume_length
    }

    fn domain_evidence(
        dev: &mut dyn SectorDev,
        parsed: &edpcli::provision::ParsedExistingProvision,
        role: PartitionRole,
        password: &[u8],
    ) -> Result<DomainEvidence, String> {
        let part = parsed
            .profile
            .partition(role)
            .ok_or_else(|| format!("缺少 {} 分区", role.label()))?;
        let record = *parsed
            .record(role)
            .ok_or_else(|| format!("缺少 {} key record", role.label()))?;
        let raw_file_key = record
            .verified_sm4_file_key(password)
            .map_err(|error| format!("{} FileKeyCRC 验证失败: {error}", role.label()))?;
        let lba = u32::try_from(part.start_lba)
            .map_err(|_| format!("{} 起始 LBA 超出 u32", role.label()))?;
        let raw = dev
            .read_sector(lba)
            .map_err(|error| format!("读取 {} 首扇区失败: {error}", role.label()))?;
        let plain = edpcli::backup_deep::keys::decrypt_mode2(&raw, &raw_file_key)
            .map_err(|error| format!("解密 {} 首扇区失败: {error}", role.label()))?;
        if !strict_exfat_boot(&plain, part.start_lba, part.sector_count) {
            return Err(format!(
                "{} 解密后未通过严格 exFAT boot-sector 校验",
                role.label()
            ));
        }
        Ok(DomainEvidence {
            raw_file_key,
            lba7_wrapper: record.lba7.encrypted_file_key.to_vec(),
            lba12_wrapper: record.lba12.encrypted_file_key.to_vec(),
            first_sector_sha256: sha256_hex(&raw),
        })
    }

    fn verify_domain_passwords(
        parsed: &edpcli::provision::ParsedExistingProvision,
        share_password: &[u8],
        encrypt_password: &[u8],
    ) -> Result<(), String> {
        if parsed.source_password_knowledge(KeyDomainRole::Share, Some(share_password))
            != SourcePasswordKnowledge::UserVerified
        {
            return Err("Share 自有密码未验证为 UserVerified".into());
        }
        if parsed.source_password_knowledge(KeyDomainRole::Encrypt, Some(encrypt_password))
            != SourcePasswordKnowledge::UserVerified
        {
            return Err("Encrypt 自有密码未验证为 UserVerified".into());
        }
        if parsed.source_password_knowledge(KeyDomainRole::Share, Some(encrypt_password))
            != SourcePasswordKnowledge::Unknown
        {
            return Err("Encrypt 密码错误地验证了 Share 域".into());
        }
        if parsed.source_password_knowledge(KeyDomainRole::Encrypt, Some(share_password))
            != SourcePasswordKnowledge::Unknown
        {
            return Err("Share 密码错误地验证了 Encrypt 域".into());
        }
        Ok(())
    }

    fn snapshot_evidence(
        disk: u32,
        device_id: &str,
        total_sectors: u64,
        share_password: &[u8],
        encrypt_password: &[u8],
    ) -> Result<(DomainEvidence, DomainEvidence), String> {
        let parsed = current_parsed_mode0(disk, device_id, total_sectors)?;
        verify_domain_passwords(&parsed, share_password, encrypt_password)?;
        let mut dev = FileDev::open_rdonly(&raw_path(disk))
            .map_err(|error| format!("只读打开 disk{disk} 失败: {error}"))?;
        let share = domain_evidence(&mut dev, &parsed, PartitionRole::Share, share_password)?;
        let encrypt = domain_evidence(&mut dev, &parsed, PartitionRole::Encrypt, encrypt_password)?;
        Ok((share, encrypt))
    }

    fn require_commit_success(outcome: ProvisionCommitOutcome) -> Result<(), String> {
        match outcome {
            ProvisionCommitOutcome::Official(report) => {
                if !report.provision_succeeded {
                    return Err("protocol provision report 标记为失败".into());
                }
                if let Some(failed) = report.formats.iter().find(|item| item.result.is_err()) {
                    return Err(format!(
                        "{} 文件系统初始化失败: {}",
                        failed.role.label(),
                        failed.result.as_ref().err().expect("checked error")
                    ));
                }
                Ok(())
            }
            ProvisionCommitOutcome::Plain { .. } => {
                Err("HIL 预期官方 mode0，却得到 Plain commit".into())
            }
        }
    }

    fn build_distinct_password_mode0(
        runner: &SysRunner,
        disk: u32,
        identity: &TargetIdentity,
        serial_sha256: &str,
        secrets: &SecretBundle,
    ) -> Result<(DomainEvidence, DomainEvidence), String> {
        require_plain_source(disk, identity.device_id(), identity.total_sectors())?;
        let request = mode0_request(None, &secrets.share_v1, None, &secrets.encrypt_v1, true);
        let prepared =
            prepare_provision_on_disk(runner, disk, &request).map_err(|error| error.msg)?;
        let official = match &prepared {
            edpcli::application::provision::PreparedProvision::Official(value) => value,
            _ => return Err("build 阶段 planner 未生成官方制盘计划".into()),
        };
        let target_plan = official
            .target_plan
            .as_ref()
            .ok_or_else(|| "build 阶段缺少 TargetProvisionPlan".to_string())?;
        for role in [
            PartitionRole::Boot,
            PartitionRole::Share,
            PartitionRole::Encrypt,
        ] {
            let part = target_plan
                .partitions
                .iter()
                .find(|part| part.geometry.role == role)
                .ok_or_else(|| format!("build planner 缺少 {}", role.label()))?;
            if part.disposition != RegionDisposition::Rebuild
                || part.action != PartitionAction::Rebuild
            {
                return Err(format!(
                    "Plain→mode0 {} 必须 Rebuild，实际 {:?}/{:?}",
                    role.label(),
                    part.action,
                    part.disposition
                ));
            }
        }
        if official
            .format_targets
            .iter()
            .filter(|choice| choice.target.format_capable)
            .any(|choice| !choice.selected)
        {
            return Err("Plain→mode0 必须完整初始化全部可格式化分区".into());
        }

        let mut prompt = HilPrompter;
        let outcome = commit_provision_with_backup_on_disk(
            runner,
            &prepared,
            edpcli::application::resolve_backup_dir(None),
            &mut prompt,
        )
        .map_err(|error| error.msg)?;
        let backup_path = outcome.backup.path.display().to_string();
        require_commit_success(outcome.commit)?;
        let (_, serial_after) = probe_target(runner, disk, Some(serial_sha256))?;
        if serial_after != serial_sha256 {
            return Err("build 后硬件 serial SHA-256 发生变化".into());
        }

        let evidence = snapshot_evidence(
            disk,
            identity.device_id(),
            identity.total_sectors(),
            &secrets.share_v1,
            &secrets.encrypt_v1,
        )?;
        println!("双域异密码 build：PASS");
        println!("  自动备份: {backup_path}");
        println!("  Share FileKeyCRC/SM4/exFAT: PASS");
        println!("  Share RAW SHA-256: {}", evidence.0.first_sector_sha256);
        println!("  Encrypt FileKeyCRC/SM4/exFAT: PASS");
        println!("  Encrypt RAW SHA-256: {}", evidence.1.first_sector_sha256);
        println!("  互换密码验证: FAIL-CLOSED/PASS");
        Ok(evidence)
    }

    fn rewrap_passwords(
        runner: &SysRunner,
        disk: u32,
        identity: &TargetIdentity,
        serial_sha256: &str,
        secrets: &SecretBundle,
        before: Option<(DomainEvidence, DomainEvidence)>,
    ) -> Result<(), String> {
        let before = match before {
            Some(value) => value,
            None => snapshot_evidence(
                disk,
                identity.device_id(),
                identity.total_sectors(),
                &secrets.share_v1,
                &secrets.encrypt_v1,
            )?,
        };
        let request = mode0_request(
            Some(&secrets.share_v1),
            &secrets.share_v2,
            Some(&secrets.encrypt_v1),
            &secrets.encrypt_v2,
            false,
        );
        let prepared =
            prepare_provision_on_disk(runner, disk, &request).map_err(|error| error.msg)?;
        let official = match &prepared {
            edpcli::application::provision::PreparedProvision::Official(value) => value,
            _ => return Err("rewrap 阶段 planner 未生成官方制盘计划".into()),
        };
        let target_plan = official
            .target_plan
            .as_ref()
            .ok_or_else(|| "rewrap 阶段缺少 TargetProvisionPlan".to_string())?;
        for role in [PartitionRole::Share, PartitionRole::Encrypt] {
            let part = target_plan
                .partitions
                .iter()
                .find(|part| part.geometry.role == role)
                .ok_or_else(|| format!("rewrap planner 缺少 {}", role.label()))?;
            if part.disposition != RegionDisposition::RewrapVerified
                || part.action != PartitionAction::PreserveExact
            {
                return Err(format!(
                    "{} password-only 必须 RewrapVerified/PreserveExact，实际 {:?}/{:?}",
                    role.label(),
                    part.action,
                    part.disposition
                ));
            }
        }
        if official.format_targets.iter().any(|choice| choice.selected) {
            return Err("password-only rewrap 不得包含 filesystem format".into());
        }
        for &lba in official.write_image.patch.keys() {
            let lba = lba as u64;
            if (SHARE_START..SHARE_START + SHARE_SECTORS).contains(&lba)
                || (ENCRYPT_START..ENCRYPT_START + ENCRYPT_SECTORS).contains(&lba)
            {
                return Err(format!("rewrap write-set 错误触及数据区 LBA{lba}"));
            }
        }

        let mut prompt = HilPrompter;
        let outcome = commit_provision_with_backup_on_disk(
            runner,
            &prepared,
            edpcli::application::resolve_backup_dir(None),
            &mut prompt,
        )
        .map_err(|error| error.msg)?;
        let backup_path = outcome.backup.path.display().to_string();
        require_commit_success(outcome.commit)?;
        let (_, serial_after) = probe_target(runner, disk, Some(serial_sha256))?;
        if serial_after != serial_sha256 {
            return Err("rewrap 后硬件 serial SHA-256 发生变化".into());
        }

        let parsed = current_parsed_mode0(disk, identity.device_id(), identity.total_sectors())?;
        verify_domain_passwords(&parsed, &secrets.share_v2, &secrets.encrypt_v2)?;
        if parsed.source_password_knowledge(KeyDomainRole::Share, Some(&secrets.share_v1))
            != SourcePasswordKnowledge::Unknown
        {
            return Err("Share 旧密码在 rewrap 后仍然有效".into());
        }
        if parsed.source_password_knowledge(KeyDomainRole::Encrypt, Some(&secrets.encrypt_v1))
            != SourcePasswordKnowledge::Unknown
        {
            return Err("Encrypt 旧密码在 rewrap 后仍然有效".into());
        }

        let mut dev = FileDev::open_rdonly(&raw_path(disk))
            .map_err(|error| format!("只读打开 disk{disk} 失败: {error}"))?;
        let after_share =
            domain_evidence(&mut dev, &parsed, PartitionRole::Share, &secrets.share_v2)?;
        let after_encrypt = domain_evidence(
            &mut dev,
            &parsed,
            PartitionRole::Encrypt,
            &secrets.encrypt_v2,
        )?;

        for (name, old, new) in [
            ("Share", &before.0, &after_share),
            ("Encrypt", &before.1, &after_encrypt),
        ] {
            if old.raw_file_key != new.raw_file_key {
                return Err(format!("{name} rewrap 改变了 K_old"));
            }
            if old.first_sector_sha256 != new.first_sector_sha256 {
                return Err(format!("{name} rewrap 改写了 ciphertext/data extent"));
            }
            if old.lba7_wrapper == new.lba7_wrapper || old.lba12_wrapper == new.lba12_wrapper {
                return Err(format!("{name} rewrap wrapper 未发生变化"));
            }
        }

        println!("password-only RewrapVerified：PASS");
        println!("  自动备份: {backup_path}");
        println!("  Share K_old 保持 / wrapper 更新 / data SHA 保持: PASS");
        println!("  Share RAW SHA-256: {}", after_share.first_sector_sha256);
        println!("  Encrypt K_old 保持 / wrapper 更新 / data SHA 保持: PASS");
        println!(
            "  Encrypt RAW SHA-256: {}",
            after_encrypt.first_sector_sha256
        );
        println!("  旧密码失效 / 新密码生效 / 新密码互换失败: PASS");
        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let args = parse_args()?;
        let runner = SysRunner;
        let (identity, serial_sha256) = probe_target(&runner, args.disk, None)?;
        println!(
            "HIL probe: disk{} VID:PID={:04x}:{:04x} sectors={} device_id={}",
            args.disk,
            identity.vid(),
            identity.pid(),
            identity.total_sectors(),
            identity.device_id()
        );
        println!("hardware_serial_sha256={serial_sha256}");

        if args.probe {
            return Ok(());
        }
        if env::var(ENABLE_ENV).ok().as_deref() != Some("YES") {
            return Err(format!("未启用真实密码 HIL；必须显式设置 {ENABLE_ENV}=YES"));
        }
        let expected_serial = args.expected_serial_sha256.as_deref().ok_or_else(|| {
            "真实写盘必须提供 --expected-serial-sha256 <probe 输出值>".to_string()
        })?;
        probe_target(&runner, args.disk, Some(expected_serial))?;

        let secrets = read_secrets(args.stdin_secrets, args.generate_secrets)?;
        let before = match args.stage {
            Stage::All | Stage::Build => Some(build_distinct_password_mode0(
                &runner,
                args.disk,
                &identity,
                &serial_sha256,
                &secrets,
            )?),
            Stage::Rewrap => None,
        };
        if matches!(args.stage, Stage::All | Stage::Rewrap) {
            rewrap_passwords(
                &runner,
                args.disk,
                &identity,
                &serial_sha256,
                &secrets,
                before,
            )?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() {
    if let Err(error) = macos::run() {
        eprintln!("real_usb_password_hil: {error}");
        std::process::exit(1);
    }
}
