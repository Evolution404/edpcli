use std::collections::BTreeMap;
use std::io;

use edpcli::{
    backup_deep::{analyze_partition, AnalysisStatus, PartitionReader},
    backup_metadata::PartitionGeometry,
    common::SECTOR,
    diskio::{execute_write_transaction, SectorDev, WriteTransactionPlan},
    provision::{
        build_plain_provision_write_plan, DiskProvisionKind, OfficialFilesystemFormat,
        PlainCleanupExtent, PlainPartitionSpec, PlainProvisionPlan,
    },
};

#[derive(Default)]
struct VirtualDisk {
    sectors: BTreeMap<u32, Vec<u8>>,
}

impl SectorDev for VirtualDisk {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0; SECTOR]))
    }

    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        self.sectors.insert(lba, data.to_vec());
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct PartitionView<'a> {
    disk: &'a mut VirtualDisk,
    start_lba: u64,
}

impl PartitionReader for PartitionView<'_> {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        let absolute = self
            .start_lba
            .checked_add(lba)
            .ok_or_else(|| io::Error::other("partition LBA overflow"))?;
        let absolute =
            u32::try_from(absolute).map_err(|_| io::Error::other("partition LBA exceeds u32"))?;
        self.disk.read_sector(absolute)
    }
}

fn exfat(start_lba: u64, sector_count: u64, label: &str) -> PlainPartitionSpec {
    PlainPartitionSpec::new(
        start_lba,
        sector_count,
        OfficialFilesystemFormat::ExFat,
        label,
    )
}

fn geometry(index: usize, part: &PlainPartitionSpec, count: usize) -> PartitionGeometry {
    PartitionGeometry {
        index,
        partition_type: 0,
        partition_count: count as u32,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: part.start_lba,
        sector_size: SECTOR as u64,
        partition_size: part.sector_count * SECTOR as u64,
        sector_count: part.sector_count,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    }
}

fn run_case(partitions: Vec<PlainPartitionSpec>) {
    let total_sectors = 500_000;
    let plan = PlainProvisionPlan::new(total_sectors, partitions).unwrap();
    let serials = (0..plan.partitions.len())
        .map(|index| 0x1234_5000 + index as u32)
        .collect::<Vec<_>>();
    let write = build_plain_provision_write_plan(
        &plan,
        Some(PlainCleanupExtent::new(450_000, 6)),
        &serials,
    )
    .unwrap();
    let transaction = WriteTransactionPlan::from_plain_provision(&write).unwrap();

    let lba3_sentinel = vec![0xA3; SECTOR];
    let mut disk = VirtualDisk::default();
    disk.sectors.insert(3, lba3_sentinel.clone());
    for lba in 450_000..450_006 {
        disk.sectors.insert(lba, vec![0xCE; SECTOR]);
    }

    execute_write_transaction(&mut disk, &transaction).unwrap();

    assert_eq!(disk.read_sector(3).unwrap(), lba3_sentinel);
    for lba in 450_000..450_006 {
        assert_eq!(disk.read_sector(lba).unwrap(), vec![0; SECTOR]);
    }

    let lba7 = disk.read_sector(7).unwrap();
    let lba12 = disk.read_sector(12).unwrap();
    assert_eq!(
        DiskProvisionKind::from_sectors(&lba7, &lba12, "virtual-device"),
        DiskProvisionKind::Plain
    );

    let mbr = disk.read_sector(0).unwrap();
    assert_eq!(&mbr[510..512], &[0x55, 0xaa]);
    for (index, part) in plan.partitions.iter().enumerate() {
        let offset = 0x1be + index * 16;
        assert_eq!(mbr[offset + 4], 0x07);
        assert_eq!(
            u32::from_le_bytes(mbr[offset + 8..offset + 12].try_into().unwrap()) as u64,
            part.start_lba
        );
        assert_eq!(
            u32::from_le_bytes(mbr[offset + 12..offset + 16].try_into().unwrap()) as u64,
            part.sector_count
        );

        let geometry = geometry(index, part, plan.partitions.len());
        let mut reader = PartitionView {
            disk: &mut disk,
            start_lba: part.start_lba,
        };
        let report = analyze_partition(&geometry, &mut reader);
        assert_eq!(
            report.status,
            AnalysisStatus::Parsed,
            "P{}: {}",
            index + 1,
            report.reason
        );
        assert_eq!(report.filesystem.as_deref(), Some("exfat"));
        assert_eq!(report.file_count, Some(0));
    }
}

#[test]
fn plain_virtual_hil_covers_one_to_four_partitions_gaps_and_non_2048_starts() {
    run_case(vec![exfat(2_048, 60_000, "P1")]);
    run_case(vec![
        exfat(1_024, 50_000, "P1"),
        exfat(80_000, 50_000, "P2"),
    ]);
    run_case(vec![
        exfat(4_096, 50_000, "P1"),
        exfat(90_000, 50_000, "P2"),
        exfat(180_000, 50_000, "P3"),
    ]);
    run_case(vec![
        exfat(8_192, 50_000, "P1"),
        exfat(100_000, 50_000, "P2"),
        exfat(200_000, 50_000, "P3"),
        exfat(300_000, 50_000, "P4"),
    ]);
}
