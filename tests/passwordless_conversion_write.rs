use std::collections::BTreeMap;
use std::io;

use edpcli::{
    common::{EXIT_IO, EXIT_ROLLED_BACK, SECTOR},
    diskio::{atomic_write_passwordless_conversion_sectors, SectorDev},
};

#[derive(Default)]
struct MemoryDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    writes: Vec<u32>,
    syncs: u32,
    fail_write_call: Option<usize>,
    write_calls: usize,
}

impl SectorDev for MemoryDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0; SECTOR]))
    }

    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        self.write_calls += 1;
        if self.fail_write_call == Some(self.write_calls) {
            self.fail_write_call = None;
            return Err(io::Error::other("injected conversion write failure"));
        }
        self.writes.push(lba);
        self.sectors.insert(lba, data.to_vec());
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.syncs += 1;
        Ok(())
    }
}

fn patch() -> BTreeMap<u32, Vec<u8>> {
    [
        (0, vec![0x10; SECTOR]),
        (7, vec![0x17; SECTOR]),
        (12, vec![0x1c; SECTOR]),
        (63, vec![0x63; SECTOR]),
        (64, vec![0x64; SECTOR]),
        (100, vec![0x55; SECTOR]),
    ]
    .into_iter()
    .collect()
}

#[test]
fn strict_writer_writes_front_then_tables_then_mbr() {
    let mut dev = MemoryDev::default();
    atomic_write_passwordless_conversion_sectors(&mut dev, &patch(), 200).unwrap();
    assert_eq!(dev.writes, vec![63, 64, 100, 7, 12, 0]);
    assert!(dev.syncs >= 2);
    assert!(
        !dev.sectors.contains_key(&200),
        "type4 boundary must remain untouched"
    );
}

#[test]
fn strict_writer_rejects_type4_or_unrelated_metadata_before_any_write() {
    for forbidden in [6u32, 62, 200, 201] {
        let mut bad = patch();
        bad.insert(forbidden, vec![0xaa; SECTOR]);
        let mut dev = MemoryDev::default();
        let error = atomic_write_passwordless_conversion_sectors(&mut dev, &bad, 200).unwrap_err();
        assert_eq!(error.code, EXIT_IO, "{}", error.msg);
        assert!(
            dev.writes.is_empty(),
            "LBA{forbidden} must fail before writing"
        );
        assert_eq!(
            dev.syncs, 0,
            "bounds validation must precede sync preflight"
        );
    }
}

#[test]
fn strict_writer_requires_all_metadata_and_front_filesystem() {
    for missing in [0u32, 7, 12] {
        let mut bad = patch();
        bad.remove(&missing);
        let mut dev = MemoryDev::default();
        assert_eq!(
            atomic_write_passwordless_conversion_sectors(&mut dev, &bad, 200)
                .unwrap_err()
                .code,
            EXIT_IO
        );
        assert!(dev.writes.is_empty());
    }

    let metadata_only: BTreeMap<u32, Vec<u8>> = [
        (0, vec![0; SECTOR]),
        (7, vec![0; SECTOR]),
        (12, vec![0; SECTOR]),
    ]
    .into_iter()
    .collect();
    let mut dev = MemoryDev::default();
    assert_eq!(
        atomic_write_passwordless_conversion_sectors(&mut dev, &metadata_only, 200)
            .unwrap_err()
            .code,
        EXIT_IO
    );
    assert!(dev.writes.is_empty());
}

#[test]
fn strict_writer_rolls_back_front_and_metadata_as_one_transaction() {
    let p = patch();
    let mut dev = MemoryDev::default();
    for &lba in p.keys() {
        dev.sectors.insert(lba, vec![lba as u8; SECTOR]);
    }
    let before = dev.sectors.clone();
    dev.fail_write_call = Some(4); // LBA7 after all front metadata sectors.
    let error = atomic_write_passwordless_conversion_sectors(&mut dev, &p, 200).unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert_eq!(dev.sectors, before);
}
