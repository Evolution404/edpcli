//! Machine checks for the profile-aware LBA0-LBA12 byte ledger.

use std::collections::{HashMap, HashSet};

const LEDGER: &str = include_str!("../audit/protocol/byte_ledger.tsv");
const EVIDENCE: &str = include_str!("../audit/protocol/evidence_manifest.tsv");
const DOC: &str = include_str!("../docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md");

fn parse_hex(value: &str) -> usize {
    usize::from_str_radix(value, 16).expect("hex ledger offset")
}

fn offsets(spec: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for piece in spec.split(',') {
        let (start, end) = match piece.split_once('-') {
            Some((start, end)) => (parse_hex(start), parse_hex(end)),
            None => {
                let value = parse_hex(piece);
                (value, value)
            }
        };
        assert!(start <= end && end < 512, "bad ledger range: {piece}");
        out.extend(start..=end);
    }
    out
}

fn evidence_modalities() -> HashMap<&'static str, &'static str> {
    let mut out = HashMap::new();
    for line in EVIDENCE
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 9, "bad evidence manifest row: {line}");
        assert!(
            out.insert(cols[0], cols[1]).is_none(),
            "duplicate evidence id"
        );
    }
    out
}

#[test]
fn byte_ledger_covers_exactly_6656_bytes_without_overlap() {
    let evidence = evidence_modalities();
    let mut seen = vec![false; 13 * 512];
    let mut complete = [0usize; 13];
    let mut partial = [0usize; 13];

    for line in LEDGER
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 9, "bad byte-ledger row: {line}");
        let lba: usize = cols[0].parse().expect("numeric LBA");
        assert!(lba < 13, "unexpected LBA: {lba}");
        let status = cols[2];
        assert!(matches!(status, "COMPLETE" | "PARTIAL" | "UNKNOWN"));
        assert!(
            !cols[4].is_empty(),
            "every ledger row needs profile tags: {line}"
        );

        let producer_ids: Vec<_> = cols[5].split(';').filter(|id| !id.is_empty()).collect();
        let consumer_ids: Vec<_> = cols[6].split(';').filter(|id| !id.is_empty()).collect();
        let physical_ids: Vec<_> = cols[7].split(';').filter(|id| !id.is_empty()).collect();
        for id in producer_ids
            .iter()
            .chain(consumer_ids.iter())
            .chain(physical_ids.iter())
        {
            assert!(
                evidence.contains_key(id),
                "unknown evidence id {id}: {line}"
            );
        }
        if status == "COMPLETE" {
            assert!(
                !producer_ids.is_empty(),
                "COMPLETE lacks producer evidence: {line}"
            );
            assert!(
                !consumer_ids.is_empty(),
                "COMPLETE lacks consumer evidence: {line}"
            );
            assert!(
                !physical_ids.is_empty(),
                "COMPLETE lacks physical evidence: {line}"
            );
            assert!(
                physical_ids.iter().all(|id| evidence[id] == "physical"),
                "COMPLETE physical evidence must point to physical captures: {line}"
            );
            assert!(
                producer_ids
                    .iter()
                    .any(|id| matches!(evidence[id], "static" | "virtual")),
                "COMPLETE producer evidence must identify static or official virtual proof: {line}"
            );
        }

        for offset in offsets(cols[1]) {
            let index = lba * 512 + offset;
            assert!(!seen[index], "overlap at LBA{lba}+0x{offset:03x}");
            seen[index] = true;
            match status {
                "COMPLETE" => complete[lba] += 1,
                "PARTIAL" => partial[lba] += 1,
                _ => {}
            }
        }
    }

    assert!(
        seen.into_iter().all(|value| value),
        "byte ledger contains gaps"
    );
    let expected_complete = [
        512, 512, 512, 512, 512, 512, 497, 512, 512, 384, 512, 512, 512,
    ];
    let expected_partial = [0, 0, 0, 0, 0, 0, 15, 0, 0, 128, 0, 0, 0];
    assert_eq!(complete, expected_complete);
    assert_eq!(partial, expected_partial);
    assert_eq!(complete.iter().sum::<usize>(), 6513);
    assert_eq!(partial.iter().sum::<usize>(), 143);

    for lba in 0..13 {
        let progress = format!("| LBA{lba} | {} | {} | 0 |", complete[lba], partial[lba]);
        assert!(
            DOC.contains(&progress),
            "canonical strict-progress table diverged from byte ledger: {progress}"
        );
    }
}

#[test]
fn lba0_bootstrap_blob_is_closed_while_selector_provenance_stays_separate() {
    let row = LEDGER
        .lines()
        .find(|line| line.starts_with("0\t000-0e0,0e2-0e7"))
        .expect("LBA0 bootstrap blob row");
    assert!(row.contains("\tCOMPLETE\t"));
    assert!(row.contains("S-NETAC-MBR"));
    assert!(row.contains("P-GOLD-NOPWD"));
    assert!(row.contains("selector chooses a known producer profile"));
    assert!(DOC.contains("8×zero + 11×UsbMainBSec + 1×Netac"));
    assert!(DOC.contains("调用链 provenance 开放问题"));
}

#[test]
fn historical_rejections_and_lba10_positive_physical_gate_are_explicit() {
    let matrix = include_str!("../audit/protocol/historical_matrix.tsv");
    assert!(matrix.contains("rejected for the exact strict-HSerial producer"));
    assert!(matrix.contains("LBA4 post-XOR flag writer is positive"));
    assert!(matrix.contains("0x10006249..0x10006257"));
    assert!(matrix.contains("probe_lba4_v19_writer.py"));
    assert!(matrix.contains("c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8"));
    assert!(matrix.contains("f5e6ddbb4e3097c9968296b43627543ecacdc24b174e52f8f049b289d7264efc"));
    assert!(matrix.contains("RepairSafe6Label@0x10008B20"));
    assert!(matrix.contains("join59"));
    assert!(matrix.contains(
        "rejected complete recovered 2020 BusManage request-constructor family for strict nonzero HSerial"
    ));
    assert!(matrix.contains("dynamic_MBR_template"));
    assert!(matrix.contains("2019-11-12"));
    assert!(matrix.contains("not component build date"));
    assert!(matrix.contains("not evidence of an earlier generation"));
    assert!(matrix.contains("install_2024_05_03_15_33_19.log"));
    assert!(matrix.contains("UsbOnlyInfo_zero"));
    assert!(matrix.contains("independently dated/hashed component bytes"));
    assert!(matrix.contains("cemssafeudisklabeltool.exe local variants"));
    assert!(matrix.contains("ReadUsbHserialsInfo@0x100054A0"));
    assert!(matrix.contains("0x10010020/0x100103B0/0x10010580/0x10010730"));
    assert!(matrix.contains("0x100108C3"));
    assert!(matrix.contains("0x10010920"));
    assert!(matrix.contains("disk_end-0x80000"));
    assert!(matrix.contains("ISUdiskRegsiterObj vtable 0x1019DB54"));
    assert!(matrix.contains("virtual_68@0x1000E650"));
    assert!(matrix.contains("directly disproves DeviceNumber/DiskNumber == HSerial[5]"));
    assert!(matrix.contains("db3d0a94694cbed20696ef00b8f22e8e111e3f12068c763efc3e703fb960bc65"));
    assert!(matrix.contains("93364d3f6798570fc10345cfe30d46d8b86b68f8e3468c49fa76ae7e8a42d2a9"));
    assert!(matrix.contains("GENERIC_READ"));

    let lba10 = LEDGER
        .lines()
        .find(|line| line.starts_with("10\t000-07f\tCOMPLETE\t"))
        .expect("LBA10 active EESI row");
    assert!(lba10.contains("P-EESI-NETAC"));
    assert!(lba10.contains("purpose-specific positive"));
    assert!(lba10.contains("general census"));
}

#[test]
fn host_hardinfo_and_optional_usb_only_info_have_separate_closed_lifecycles() {
    let lba4 = LEDGER
        .lines()
        .find(|line| line.starts_with("4\t035-038\tCOMPLETE\t"))
        .expect("LBA4 MyHardinfo row");
    let lba8 = LEDGER
        .lines()
        .find(|line| line.starts_with("8\t014-017\tCOMPLETE\t"))
        .expect("LBA8 HDSerialInfo row");
    let usb_only = LEDGER
        .lines()
        .find(|line| line.starts_with("8\t01e-02d\tCOMPLETE\t"))
        .expect("LBA8 UsbOnlyInfo row");

    for row in [lba4, lba8] {
        assert!(row.contains("S-WIN-191141"));
        assert!(row.contains("host"));
        assert!(row.contains("P-GOLD-ENC"));
    }
    assert!(usb_only.contains("strict-legacy-absent"));
    assert!(usb_only.contains("semantic consumers do not branch"));
    assert!(DOC.contains("A68BAE08"));
    assert!(DOC.contains("不同目标U盘"));
}

#[test]
fn cems2_join59_reader_does_not_get_promoted_to_a_writer() {
    let lba6 = LEDGER
        .lines()
        .find(|line| line.starts_with("6\t03f\tPARTIAL\t"))
        .expect("LBA6 join boundary row");
    let lba9 = LEDGER
        .lines()
        .find(|line| line.starts_with("9\t080-0ff\tPARTIAL\t"))
        .expect("LBA9 continuation row");

    for row in [lba6, lba9] {
        assert!(row.contains("S-FILEOPHOOK-2022"));
        assert!(row.contains("producer") || row.contains("writer"));
    }
    assert!(DOC.contains("fcn.10026280"));
    assert!(DOC.contains("GENERIC_READ"));
    assert!(DOC.contains("fcn.18002B880"));
    assert!(DOC.contains("不能据此推导出 sector writer"));
}

#[test]
fn lba4_hserial_is_closed_as_caller_owned_vector_without_inventing_devicenumber_algorithm() {
    let row = LEDGER
        .lines()
        .find(|line| line.starts_with("4\t020-033\tCOMPLETE\tcaller-owned HSerialCRC[5]"))
        .expect("LBA4 HSerialCRC[5] ledger row");

    assert!(row.contains("request+0x150..+0x160"));
    assert!(row.contains("V-LBA4-V19"));
    assert!(row.contains("bit-exact"));
    assert!(row.contains("explicitly not claimed as the HSerial generation algorithm"));
    assert!(DOC.contains("vtable+0x2C"));
    assert!(DOC.contains("0x1019DB54"));
    assert!(DOC.contains("virtual_44@0x100054A0"));
    assert!(DOC.contains("object+0x2488..+0x2498"));
    assert!(DOC.contains("fcn.10006090"));
    assert!(DOC.contains("fcn.10008800"));
    assert!(DOC.contains("disk_end-0x80000"));
    assert!(DOC.contains("ordinal3=`EDP_DeviceNumber`"));
    assert!(DOC.contains("ordinal4=`EDP_DiskNumber`"));
    assert!(DOC.contains("旧 `ReadUsbHserialsInfo` ABI 已直接排除这种等价关系"));
    assert!(DOC.contains("caller-owned `HSerialCRC[5]`"));
    assert!(DOC.contains("512/512与物理 gold 完全一致"));
}

#[test]
fn local_labeltool_patches_cannot_be_mistaken_for_official_protocol_evidence() {
    let note = include_str!("../audit/protocol/labeltool_variant_diff.md");
    for required in [
        "b530a82b29bbc43be8d415225392ca135ab7df8a8d9f69c6598493c4942e9e11",
        "0x00449B76",
        "0x0042DDC0",
        "0x004289FB",
        "must not be cited as official producer evidence",
        "CreateBusManageImp",
        "BusManageImp::WriteLabel@0x100A28E0",
        "0x996 = 2454",
    ] {
        assert!(
            note.contains(required),
            "label-tool integrity boundary lost evidence: {required}"
        );
    }
}

#[test]
fn devicenumber_host_identity_crc_boundary_is_explicit() {
    let note = include_str!("../audit/protocol/devicenumber_identity.md");
    for required in [
        "0ef94c3679da6f27eac75959cf299bbad19676c251d88f7554fbc305407d6041",
        "EDP_DeviceNumber @ 0x10011E00",
        "EDP_DiskNumber @ 0x10012C90",
        "0xEDB88320",
        "0x100130A2",
        "fcn.10013210",
        "standard reflected IEEE CRC-32",
        "LBA4 `HSerialCRC[5]` is COMPLETE as a caller-owned five-DWORD identity vector",
        "one DWORD",
        "UsbLabelParam::HDOnlySerial[5]",
        "explicitly **not** the HSerial generation algorithm",
    ] {
        assert!(
            note.contains(required),
            "DeviceNumber host-identity evidence lost boundary: {required}"
        );
    }
}

#[test]
fn lba3_is_closed_at_the_edp_preserve_only_boundary() {
    let row = LEDGER
        .lines()
        .find(|line| line.starts_with("3\t000-1ff\t"))
        .expect("LBA3 ledger row");
    for required in [
        "\tCOMPLETE\t",
        "manufacturer-owned opaque MP metadata / EDP preserve-only sector",
        "S-WIN-191141-LBA3",
        "S-REPAIR-2021",
        "never zero-fill",
        "provenance outside the EDP protocol boundary",
    ] {
        assert!(
            row.contains(required),
            "LBA3 preserve boundary lost: {required}"
        );
    }
    assert!(DOC.contains("不存在 `N=3`"));
}

#[test]
fn evidence_modalities_remain_distinct() {
    let modalities: HashSet<_> = evidence_modalities().into_values().collect();
    assert!(modalities.contains("physical"));
    assert!(modalities.contains("virtual"));
    assert!(modalities.contains("static"));
}

#[test]
fn lba3_manufacturing_gate_keeps_fw_marker_page_distinct_from_host_lba3() {
    let note = include_str!("../audit/protocol/lba3_identity.md");
    for required in [
        "file_size - 0x200",
        "CBaseController::virtual_464",
        "marker-page readers",
        "0x459C7EB5",
        "0x22A482A8",
        "Local capture inventory is insufficient to lock PS2307 versus PS2309",
        "do **not** contain USB serial",
    ] {
        assert!(
            note.contains(required),
            "LBA3 identity/manufacturing gate lost evidence boundary: {required}"
        );
    }
}
