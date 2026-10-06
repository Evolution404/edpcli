use edpcli::application::backup::acquire_plain_metadata;
use edpcli::application::media_identity::{
    HardwareIdentityEvidence, MediaIdentityPin, MediaIdentityResumePin, MediaIdentitySnapshot,
    SerialQuality,
};
use edpcli::ports::SectorDev;
use std::io;
fn crc(data: &[u8]) -> u32 {
    let mut n = 0xffff_ffffu32;
    for b in data {
        n ^= *b as u32;
        for _ in 0..8 {
            n = if n & 1 != 0 {
                (n >> 1) ^ 0xedb8_8320
            } else {
                n >> 1
            };
        }
    }
    !n
}
fn header(count: u32, array_crc: u32) -> Vec<u8> {
    let mut b = vec![0u8; 512];
    b[..8].copy_from_slice(b"EFI PART");
    b[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    b[12..16].copy_from_slice(&92u32.to_le_bytes());
    for (pos, value) in [(24, 1u64), (32, 99_999), (40, 34), (48, 99_966), (72, 2)] {
        b[pos..pos + 8].copy_from_slice(&value.to_le_bytes());
    }
    b[56..72].fill(0x44);
    b[80..84].copy_from_slice(&count.to_le_bytes());
    b[84..88].copy_from_slice(&128u32.to_le_bytes());
    b[88..92].copy_from_slice(&array_crc.to_le_bytes());
    let sum = crc(&b[..92]);
    b[16..20].copy_from_slice(&sum.to_le_bytes());
    b
}
struct Dev {
    flips: bool,
    headers: usize,
    second_array_read: bool,
    entries: Vec<u8>,
    writes: usize,
}
impl SectorDev for Dev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        match lba {
            0 => {
                let mut b = vec![0u8; 512];
                b[450] = 0xee;
                b[454..458].copy_from_slice(&1u32.to_le_bytes());
                b[458..462].copy_from_slice(&99_999u32.to_le_bytes());
                b[510..].copy_from_slice(&[0x55, 0xaa]);
                Ok(b)
            }
            1 => {
                self.headers += 1;
                Ok(header(
                    if !self.flips || self.headers >= 2 {
                        65_540
                    } else {
                        4
                    },
                    crc(&self.entries),
                ))
            }
            2 => {
                if self.headers >= 2 {
                    self.second_array_read = true;
                    Err(io::Error::other(
                        "bounded audit: stop before oversized array I/O",
                    ))
                } else {
                    Ok(self.entries.clone())
                }
            }
            _ => Err(io::Error::other("unexpected audit read")),
        }
    }
    fn write_sector(&mut self, _: u32, _: &[u8]) -> io::Result<()> {
        self.writes += 1;
        Err(io::Error::other("audit never writes"))
    }
}
fn dev(flips: bool) -> Dev {
    let mut e = vec![0u8; 512];
    e[..16].fill(0x11);
    e[16..32].fill(0x22);
    e[32..40].copy_from_slice(&2048u64.to_le_bytes());
    e[40..48].copy_from_slice(&80_000u64.to_le_bytes());
    Dev {
        flips,
        headers: 0,
        second_array_read: false,
        entries: e,
        writes: 0,
    }
}
fn main() {
    let mut first = dev(false);
    let first_error = acquire_plain_metadata(&mut first, 100_000).unwrap_err();
    assert!(first_error.contains("安全上限"));
    assert!(!first.second_array_read);
    let mut changed = dev(true);
    let second_error = acquire_plain_metadata(&mut changed, 100_000).unwrap_err();
    assert!(changed.second_array_read);
    assert!(second_error.contains("bounded audit"));
    assert_eq!(changed.writes, 0);
    println!("GPT: first read rejects >8MiB before array I/O; changed second read reaches array I/O after allocating >8MiB. Error: {second_error}");
    let snapshot = MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            serial: Some("AUDIT-RAW-SERIAL-001".into()),
            serial_quality: SerialQuality::Usable,
            ..Default::default()
        },
        ..Default::default()
    };
    let image = vec![0u8; 6656];
    let pin = MediaIdentityPin::new(snapshot.clone(), &image);
    assert!(pin.verify(&snapshot, &image).is_ok());
    let resume = MediaIdentityResumePin::from_pin(&pin);
    let error = resume.validate().unwrap_err();
    println!("Pin: current raw-only identity passes in-memory pin check, but generated resume pin is rejected: {error}");
}
