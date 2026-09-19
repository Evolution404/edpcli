//! Contract tests for the protocol reverse-engineering ledger.
//!
//! The documentation is part of the protocol safety boundary: a field must not
//! be called COMPLETE unless producer, consumer and real-device evidence are
//! recorded together.

const DOC: &str = include_str!("../docs/PROTOCOL_BYTE_TRACE_2026-09-19.md");

fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let start_pos = text.find(start).expect("missing start marker") + start.len();
    let rest = &text[start_pos..];
    let end_pos = rest.find(end).expect("missing end marker");
    &rest[..end_pos]
}

#[test]
fn strict_progress_covers_exactly_lba0_through_lba12() {
    let table = between(
        DOC,
        "<!-- STRICT_PROGRESS_BEGIN -->",
        "<!-- STRICT_PROGRESS_END -->",
    );
    let mut seen = [false; 13];
    let mut complete = 0usize;
    let mut partial = 0usize;
    let mut unknown = 0usize;

    for line in table.lines() {
        let cols: Vec<_> = line.split('|').map(str::trim).collect();
        if cols.len() < 6 || !cols[1].starts_with("LBA") || cols[1] == "LBA" {
            continue;
        }
        let lba: usize = cols[1][3..].parse().expect("numeric LBA");
        assert!(lba < 13, "unexpected LBA row: {line}");
        assert!(!seen[lba], "duplicate LBA{lba} row");
        seen[lba] = true;

        let c: usize = cols[2].parse().expect("COMPLETE bytes");
        let p: usize = cols[3].parse().expect("PARTIAL bytes");
        let u: usize = cols[4].parse().expect("UNKNOWN bytes");
        assert_eq!(c + p + u, 512, "LBA{lba} does not cover exactly 512 bytes");
        complete += c;
        partial += p;
        unknown += u;
    }

    assert!(
        seen.into_iter().all(|value| value),
        "LBA0..12 must all be present"
    );
    assert_eq!(complete + partial + unknown, 13 * 512);
    assert!(
        complete >= 1535,
        "strict COMPLETE coverage regressed below the audited baseline: {complete}"
    );
    assert!(
        unknown <= 2537,
        "UNKNOWN coverage regressed above the audited baseline: {unknown}"
    );
}

#[test]
fn every_complete_field_has_producer_consumer_and_real_device_evidence() {
    let table = between(
        DOC,
        "<!-- FIELD_LEDGER_BEGIN -->",
        "<!-- FIELD_LEDGER_END -->",
    );
    let mut complete_rows = 0usize;

    for line in table.lines() {
        let cols: Vec<_> = line.split('|').map(str::trim).collect();
        if cols.len() < 10 || cols[3] != "COMPLETE" {
            continue;
        }
        complete_rows += 1;
        let producer = cols[5];
        let consumer = cols[6];
        let real_device = cols[7];

        for (label, value) in [
            ("producer", producer),
            ("consumer", consumer),
            ("real-device evidence", real_device),
        ] {
            assert!(
                !value.is_empty()
                    && value != "—"
                    && !value.contains("待查")
                    && !value.contains("未知"),
                "COMPLETE row lacks {label}: {line}"
            );
        }
    }

    assert!(
        complete_rows >= 15,
        "field ledger unexpectedly lost COMPLETE evidence rows"
    );
}

#[test]
fn documentation_keeps_the_official_provisioning_chain_and_strict_rules() {
    for required in [
        "cemssafeudisklabeltool.exe",
        "CreateBusManageImp",
        "BusManageImp::WriteNormalULabel",
        "CUsbRegsiter::RegsiterUsb",
        "WriteSectorData(..., count=0x0D)",
        "CLabelManage::BuildSector11",
        "CLabelManage::ReadSector11",
        "diskfile.cpp:672 / 1005",
        "diskfile.cpp:740 / 956",
        "diskfile.cpp:805 / 1102,1143",
        "ElabOffset",
        "edpdiskglobal.h:413",
        "SetVolumeLabelA",
        "Share/type2 volume label",
        "Encrypt/type4 volume label",
        "240d04e7c97d300c5081f793d72850d49acbf5408bc0d8cf32de8eef7a5e8f02",
        "tagEdpEDiskTmpUse",
        "edpdiskglobal.h:481",
        "useCount=0xFFFFFFFF",
        "OutManage switch is off",
        "opaque preserve / write-protection probe scratch sector",
        "ERROR_WRITE_PROTECT(0x13)",
        "076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560",
        "BuildSector1_Gpt",
        "BuildSector2_Gpt",
        "GPT_Header",
        "GPT_Partition",
        "overflow_marker(0x40245E2A)",
        "m_autoid / Autonum",
        "NUL 后真实槽尾大量非零",
        "LBA3 opaque manufacturer/MP sector",
        "LBA8 static version/writeTime/reserved header",
        "LBA6 C-string slots keep opaque post-NUL tails",
        "LBA4 current writer machine-code node layout",
        "22/22",
        "不能单独把字段升级为 COMPLETE",
        "禁止把免密转换盘",
    ] {
        assert!(
            DOC.contains(required),
            "protocol ledger lost required evidence: {required}"
        );
    }
    assert!(
        DOC.contains("| LBA1 | 0 | 512 | 0 | 0.0% |"),
        "LBA1 GPT profile must remain PARTIAL until a positive real GPT sample exists"
    );
    assert!(
        DOC.contains("| LBA2 | 0 | 512 | 0 | 0.0% |"),
        "LBA2 GPT profile must remain PARTIAL until a positive real GPT sample exists"
    );
}
