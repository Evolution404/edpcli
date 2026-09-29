#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("real_usb_k6_verify currently supports macOS only");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::env;
    use std::fs;
    use std::io::{self, Write};
    use std::path::PathBuf;

    use edpcli::application::device::guard_usb_disk;
    use edpcli::backup_metadata::PartitionGeometry;
    use edpcli::common::SECTOR;
    use edpcli::diskio::{raw_path, FileDev, SectorDev};
    use edpcli::filesystem::analysis::{
        analyze_partition, stream_file_payload, AnalysisStatus, PartitionReader,
    };
    use edpcli::provision::{
        parse_existing_provision, KeyDomainRole, PartitionRole, ProvisionImage,
        SourcePasswordKnowledge, TargetIdentity, DEFAULT_KEY_DOMAIN_PASSWORD,
    };
    use edpcli::sysinfo::{disk_total_sectors, CmdRunner, SysRunner};
    use serde::Deserialize;
    use sha2::{Digest, Sha256};

    const EXPECTED_VID: u16 = 0x3535;
    const EXPECTED_PID: u16 = 0x6300;
    const EXPECTED_TOTAL_SECTORS: u64 = 15_728_640;
    const EXPECTED_DEVICE_ID: &str = "disk&ven_aigo&prod_u335&rev_1100";

    #[derive(Debug)]
    struct Args {
        disk: u32,
        role: PartitionRole,
        prefix: String,
        manifest: PathBuf,
    }

    #[derive(Debug, Deserialize)]
    struct Manifest {
        file_count: usize,
        total_bytes: u64,
        aggregate_sha256: String,
        files: Vec<(String, u64, String)>,
    }

    struct RawPartitionReader<'a> {
        dev: &'a mut dyn SectorDev,
        start_lba: u64,
        sector_count: u64,
        file_key: Option<[u8; 16]>,
    }

    impl Drop for RawPartitionReader<'_> {
        fn drop(&mut self) {
            if let Some(key) = self.file_key.as_mut() {
                key.fill(0);
            }
        }
    }

    impl PartitionReader for RawPartitionReader<'_> {
        fn read_sector(&mut self, relative_lba: u64) -> io::Result<Vec<u8>> {
            if relative_lba >= self.sector_count {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "read outside source partition",
                ));
            }
            let absolute = self
                .start_lba
                .checked_add(relative_lba)
                .and_then(|lba| u32::try_from(lba).ok())
                .ok_or_else(|| io::Error::other("partition LBA overflow"))?;
            let raw = self.dev.read_sector(absolute)?;
            if raw.len() != SECTOR {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partition sector is truncated",
                ));
            }
            match self.file_key {
                Some(key) => {
                    edpcli::partition_transform::decrypt_mode2(&raw, &key).map_err(io::Error::other)
                }
                None => Ok(raw),
            }
        }
    }

    struct HashWriter {
        hash: Sha256,
        bytes: u64,
    }

    impl HashWriter {
        fn new() -> Self {
            Self {
                hash: Sha256::new(),
                bytes: 0,
            }
        }

        fn finish(self) -> (u64, String) {
            (self.bytes, format!("{:x}", self.hash.finalize()))
        }
    }

    impl Write for HashWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.hash.update(buf);
            self.bytes = self
                .bytes
                .checked_add(buf.len() as u64)
                .ok_or_else(|| io::Error::other("hash byte count overflow"))?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn parse_role(value: &str) -> Result<PartitionRole, String> {
        match value {
            "boot" => Ok(PartitionRole::Boot),
            "share" => Ok(PartitionRole::Share),
            "encrypt" => Ok(PartitionRole::Encrypt),
            "combined" => Ok(PartitionRole::BootShareCombined),
            other => Err(format!("unsupported --role {other}")),
        }
    }

    fn parse_args() -> Result<Args, String> {
        let mut disk = None;
        let mut role = None;
        let mut prefix = None;
        let mut manifest = None;
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--disk" => {
                    disk = Some(
                        args.next()
                            .ok_or("--disk requires a value")?
                            .parse()
                            .map_err(|_| "--disk must be an integer")?,
                    );
                }
                "--role" => {
                    role = Some(parse_role(&args.next().ok_or("--role requires a value")?)?);
                }
                "--prefix" => prefix = Some(args.next().ok_or("--prefix requires a value")?),
                "--manifest" => {
                    manifest = Some(PathBuf::from(
                        args.next().ok_or("--manifest requires a value")?,
                    ));
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(Args {
            disk: disk.ok_or("missing --disk")?,
            role: role.ok_or("missing --role")?,
            prefix: prefix.ok_or("missing --prefix")?,
            manifest: manifest.ok_or("missing --manifest")?,
        })
    }

    fn read_metadata(dev: &mut dyn SectorDev) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::with_capacity(13 * SECTOR);
        for lba in 0..13u32 {
            bytes.extend(
                dev.read_sector(lba)
                    .map_err(|error| format!("read LBA{lba} failed: {error}"))?,
            );
        }
        Ok(bytes)
    }

    fn target_path(prefix: &str, relative: &str) -> String {
        let prefix = prefix.trim_end_matches('/');
        let relative = relative.trim_start_matches('/');
        if prefix.is_empty() || prefix == "/" {
            format!("/{relative}")
        } else {
            format!("{prefix}/{relative}")
        }
    }

    pub fn run() -> Result<(), String> {
        let args = parse_args()?;
        let runner = SysRunner;
        guard_usb_disk(&runner, args.disk).map_err(|error| error.msg)?;
        let total = disk_total_sectors(&runner, args.disk)
            .ok_or_else(|| format!("cannot read disk{} sector count", args.disk))?;
        let probe = runner
            .hardware_probe(args.disk)
            .ok_or_else(|| format!("cannot read disk{} hardware identity", args.disk))?;
        let identity = TargetIdentity::from_probe(&probe, total)
            .map_err(|error| format!("cannot construct target identity: {error}"))?;
        if identity.vid() != EXPECTED_VID
            || identity.pid() != EXPECTED_PID
            || identity.total_sectors() != EXPECTED_TOTAL_SECTORS
            || identity.device_id() != EXPECTED_DEVICE_ID
        {
            return Err(format!(
                "real USB identity mismatch: {:04x}:{:04x} sectors={} device_id={}",
                identity.vid(),
                identity.pid(),
                identity.total_sectors(),
                identity.device_id()
            ));
        }

        let raw = raw_path(args.disk);
        let mut dev = FileDev::open_rdonly(&raw)
            .map_err(|error| format!("open {raw} read-only failed: {error}"))?;
        let metadata = read_metadata(&mut dev)?;
        let image = ProvisionImage::from_bytes(metadata)
            .map_err(|error| format!("protocol image invalid: {error}"))?;
        let source = parse_existing_provision(&image, identity.device_id(), total)
            .map_err(|error| format!("protocol parse failed: {error}"))?
            .ok_or("disk is not a registered EDP provision")?;
        let source_index = source
            .profile
            .partitions
            .iter()
            .position(|part| part.role == args.role)
            .ok_or_else(|| format!("source has no {:?} partition", args.role))?;
        let part = source.profile.partitions[source_index];
        let mut file_key = if part.physically_encrypted {
            let domain = KeyDomainRole::from_partition_role(part.role)
                .ok_or("encrypted partition has no key domain")?;
            match source.source_password_knowledge(domain, None) {
                SourcePasswordKnowledge::DefaultVerified => Some(
                    source.records[source_index]
                        .verified_sm4_file_key(DEFAULT_KEY_DOMAIN_PASSWORD)
                        .map_err(|error| format!("default FileKey verify failed: {error}"))?,
                ),
                knowledge => {
                    return Err(format!(
                        "encrypted source is not default-password verified: {knowledge:?}"
                    ));
                }
            }
        } else {
            None
        };

        let geometry = PartitionGeometry {
            index: source_index,
            partition_type: part.partition_type.raw(),
            partition_count: 1,
            need_disturb: 0,
            need_encrypt: 0,
            start_sector: part.start_lba,
            sector_size: SECTOR as u64,
            partition_size: part
                .sector_count
                .checked_mul(SECTOR as u64)
                .ok_or("partition byte size overflow")?,
            sector_count: part.sector_count,
            user_key_crc: 0,
            file_key_crc: 0,
            encrypt_mode: 0,
        };
        let mut reader = RawPartitionReader {
            dev: &mut dev,
            start_lba: part.start_lba,
            sector_count: part.sector_count,
            file_key,
        };
        let report = analyze_partition(&geometry, &mut reader);
        drop(reader);
        if report.status != AnalysisStatus::Parsed {
            return Err(format!("filesystem parse failed: {}", report.reason));
        }
        let entries = report
            .entries
            .ok_or("filesystem parser returned no entries")?;
        let manifest: Manifest = serde_json::from_slice(
            &fs::read(&args.manifest)
                .map_err(|error| format!("read manifest {:?}: {error}", args.manifest))?,
        )
        .map_err(|error| format!("parse manifest {:?}: {error}", args.manifest))?;
        if manifest.file_count != manifest.files.len() {
            return Err("manifest file_count does not match files array".into());
        }

        let mut aggregate = Sha256::new();
        let mut total_bytes = 0u64;
        for (relative, expected_size, expected_hash) in &manifest.files {
            let path = target_path(&args.prefix, relative);
            let entry = entries
                .iter()
                .find(|entry| !entry.is_directory && entry.path == path)
                .ok_or_else(|| format!("missing file {path:?}"))?
                .clone();
            if entry.logical_size != *expected_size {
                return Err(format!(
                    "size mismatch for {path:?}: {} != {}",
                    entry.logical_size, expected_size
                ));
            }
            let mut sink = HashWriter::new();
            let mut file_reader = RawPartitionReader {
                dev: &mut dev,
                start_lba: part.start_lba,
                sector_count: part.sector_count,
                file_key,
            };
            let summary = stream_file_payload(&mut file_reader, &entry, *expected_size, &mut sink)
                .map_err(|error| format!("stream {path:?} failed: {error}"))?;
            let (actual_size, actual_hash) = sink.finish();
            if summary.logical_size != *expected_size
                || actual_size != *expected_size
                || !actual_hash.eq_ignore_ascii_case(expected_hash)
            {
                return Err(format!(
                    "payload mismatch for {path:?}: summary={} actual_size={} expected_size={} actual_sha256={} expected_sha256={}",
                    summary.logical_size, actual_size, expected_size, actual_hash, expected_hash
                ));
            }
            total_bytes = total_bytes
                .checked_add(actual_size)
                .ok_or("verified byte count overflow")?;
            aggregate.update(relative.as_bytes());
            aggregate.update(b"\0");
            aggregate.update(actual_size.to_string().as_bytes());
            aggregate.update(b"\0");
            aggregate.update(actual_hash.as_bytes());
            aggregate.update(b"\n");
        }
        if total_bytes != manifest.total_bytes {
            return Err(format!(
                "total bytes mismatch: {total_bytes} != {}",
                manifest.total_bytes
            ));
        }
        let aggregate = format!("{:x}", aggregate.finalize());
        if !aggregate.eq_ignore_ascii_case(&manifest.aggregate_sha256) {
            return Err(format!(
                "aggregate SHA-256 mismatch: {aggregate} != {}",
                manifest.aggregate_sha256
            ));
        }
        if let Some(key) = file_key.as_mut() {
            key.fill(0);
        }
        println!(
            "K6 RAW VERIFY PASS role={:?} prefix={} files={} bytes={} aggregate_sha256={}",
            args.role, args.prefix, manifest.file_count, total_bytes, aggregate
        );
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() {
    if let Err(error) = macos::run() {
        eprintln!("real_usb_k6_verify: {error}");
        std::process::exit(1);
    }
}
