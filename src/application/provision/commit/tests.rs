use super::*;
use crate::application::error::{MediaState, OperationError};
use std::io;

#[derive(Clone, Copy)]
enum Fault {
    Write,
    Sync,
    Readback,
    Mismatch,
    Rollback,
}

struct FaultDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    attempts: Vec<u32>,
    writes: usize,
    fault: Fault,
    injected: bool,
}

impl SectorDev for FaultDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        if self.writes > 0 && !self.injected {
            match self.fault {
                Fault::Readback => {
                    self.injected = true;
                    return Err(io::Error::other("injected readback failure"));
                }
                Fault::Mismatch => {
                    self.injected = true;
                    let mut sector = self.sectors.get(&lba).cloned().unwrap();
                    sector[0] ^= 1;
                    return Ok(sector);
                }
                _ => {}
            }
        }
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0; SECTOR]))
    }

    fn write_sector(&mut self, lba: u32, bytes: &[u8]) -> io::Result<()> {
        self.attempts.push(lba);
        if self.writes > 0 {
            if matches!(self.fault, Fault::Rollback) {
                return Err(io::Error::other(
                    "injected persistent write/rollback failure",
                ));
            }
            if matches!(self.fault, Fault::Write) && !self.injected {
                self.injected = true;
                return Err(io::Error::other("injected one-shot write failure"));
            }
        }
        self.writes += 1;
        self.sectors.insert(lba, bytes.to_vec());
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        if self.writes > 0 && matches!(self.fault, Fault::Sync) && !self.injected {
            self.injected = true;
            return Err(io::Error::other("injected post-write sync failure"));
        }
        Ok(())
    }

    // This test device is already writable.
    fn reopen_rdwr(&mut self, _: std::time::Duration) -> std::io::Result<()> {
        Ok(())
    }
}

fn choices() -> Vec<PlannedPartitionFormat> {
    let key = [0x42; 16];
    let plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionSizes::new(32, 64, 128),
        crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
            chs_bytes: 0,
            start_byte_offset: 4_194_000 * SECTOR as u64,
            start_lba: 4_194_000,
            size_bytes: 3072,
            size_sectors: 6,
        },
        wrap_legacy_lba7_file_key(DEFAULT_KEY_DOMAIN_PASSWORD, [0; 8]),
        wrap_file_key(DEFAULT_KEY_DOMAIN_PASSWORD, key, FileKeyWrapMode::Sm4),
    )
    .unwrap();
    plan_format_targets(
        &plan,
        &FormatOptions {
            boot: true,
            share: true,
            encrypt: true,
            ..Default::default()
        },
        &[1, 2, 3],
        &key,
    )
    .unwrap()
}

#[test]
fn format_faults_preserve_transaction_state_and_never_write_later_partitions() {
    let choices = choices();
    let first = &choices[0].target.geometry;
    for (fault, code, state) in [
        (Fault::Write, EXIT_ROLLED_BACK, MediaState::RolledBack),
        (Fault::Sync, EXIT_ROLLED_BACK, MediaState::RolledBack),
        (Fault::Readback, EXIT_ROLLED_BACK, MediaState::RolledBack),
        (Fault::Mismatch, EXIT_ROLLED_BACK, MediaState::RolledBack),
        (Fault::Rollback, EXIT_INTERMEDIATE, MediaState::Intermediate),
    ] {
        let mut dev = FaultDev {
            sectors: BTreeMap::new(),
            attempts: Vec::new(),
            writes: 0,
            fault,
            injected: false,
        };
        // This is the production sequencing function, using the real sparse
        // filesystem writer and sector transaction executor with an in-memory device.
        let formats = execute_selected_formats(&choices, |choice| {
            execute_partition_format(&mut dev, choice).map_err(OperationError::from)
        });
        let error = formats[0]
            .result
            .as_ref()
            .unwrap_err()
            .operation_error()
            .unwrap();
        assert_eq!(error.code, Some(code));
        assert_eq!(error.media_state, Some(state));
        assert!(formats[1..].iter().all(|format| matches!(
            format.result,
            Err(PartitionFormatError::Skipped {
                after: PartitionRole::Boot
            })
        )));
        assert!(!dev.attempts.is_empty());
        assert!(dev
            .attempts
            .iter()
            .all(|&lba| u64::from(lba) >= first.start_sector
                && u64::from(lba) < first.start_sector + first.sector_count()));
        if state == MediaState::RolledBack {
            assert!(dev
                .sectors
                .values()
                .all(|sector| sector == &vec![0; SECTOR]));
        }
        let outcome = ProvisionWriteOutcome {
            backup: super::super::super::post_restore::MetadataBackupReport {
                path: "memory-only.edpb".into(),
                partition_count: 3,
                edp_protocol_saved: true,
            },
            commit: ProvisionCommitOutcome::Official(ProvisionCommitReport {
                provision_succeeded: true,
                formats,
            }),
            warnings: vec![ProvisionWarning::IncompleteFormat],
        };
        assert_eq!(outcome.exit_code(), code);
        let ProvisionCommitOutcome::Official(report) = &outcome.commit else {
            unreachable!()
        };
        assert!(
            report.provision_succeeded,
            "a format rollback does not undo the earlier protocol commit"
        );
    }
}

#[test]
fn unknown_state_and_prewrite_rejection_stop_the_format_chain() {
    let choices = choices();
    for (state, source_code, exit_code) in [
        (MediaState::Unknown, EXIT_IO, EXIT_INTERMEDIATE),
        (MediaState::Unchanged, EXIT_TARGET, EXIT_TARGET),
    ] {
        let mut executed = Vec::new();
        let formats = execute_selected_formats(&choices, |choice| {
            executed.push(choice.target.role);
            Err(
                OperationError::from(err(source_code, "injected verification failure"))
                    .with_media_state(state),
            )
        });
        assert_eq!(executed, [PartitionRole::Boot]);
        assert!(formats[1..]
            .iter()
            .all(|format| format.result.as_ref().unwrap_err().is_skipped()));
        let outcome = ProvisionWriteOutcome {
            backup: super::super::super::post_restore::MetadataBackupReport {
                path: "memory-only.edpb".into(),
                partition_count: 3,
                edp_protocol_saved: true,
            },
            commit: ProvisionCommitOutcome::Official(ProvisionCommitReport {
                provision_succeeded: true,
                formats,
            }),
            warnings: vec![],
        };
        assert_eq!(outcome.exit_code(), exit_code);
    }
}

#[test]
fn failure_after_one_success_preserves_completed_partition_and_skips_the_next() {
    let choices = choices();
    let mut executed = Vec::new();
    let formats = execute_selected_formats(&choices, |choice| {
        executed.push(choice.target.role);
        if choice.target.role == PartitionRole::Boot {
            Ok(())
        } else {
            Err(OperationError::from(err(
                EXIT_ROLLED_BACK,
                "second partition rolled back",
            )))
        }
    });
    assert_eq!(executed, [PartitionRole::Boot, PartitionRole::Share]);
    assert!(formats[0].result.is_ok());
    assert_eq!(
        formats[1]
            .result
            .as_ref()
            .unwrap_err()
            .operation_error()
            .unwrap()
            .code,
        Some(EXIT_ROLLED_BACK)
    );
    assert!(matches!(
        formats[2].result,
        Err(PartitionFormatError::Skipped {
            after: PartitionRole::Share
        })
    ));
}
