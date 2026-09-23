//! Machine checks for the profile-aware LBA0-LBA12 byte ledger.

use std::collections::{HashMap, HashSet};

const LEDGER: &str = include_str!("../audit/protocol/byte_ledger.tsv");
const EVIDENCE: &str = include_str!("../audit/protocol/evidence_manifest.tsv");
const PROFILE_COVERAGE: &str = include_str!("../audit/protocol/profile_coverage.tsv");
const DOC: &str = include_str!("../docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md");

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
    let mut unknown = [0usize; 13];

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
                "UNKNOWN" => unknown[lba] += 1,
                _ => unreachable!(),
            }
        }
    }

    assert!(
        seen.into_iter().all(|value| value),
        "byte ledger contains gaps"
    );
    assert_eq!(
        complete.iter().sum::<usize>()
            + partial.iter().sum::<usize>()
            + unknown.iter().sum::<usize>(),
        13 * 512,
        "semantic status totals must cover exactly LBA0-LBA12 without a historical completion floor"
    );

    for lba in 0..13 {
        let progress = format!(
            "| LBA{lba} | {} | {} | {} |",
            complete[lba], partial[lba], unknown[lba]
        );
        assert!(
            DOC.contains(&progress),
            "canonical strict-progress table diverged from byte ledger: {progress}"
        );
    }
}

#[test]
fn profile_coverage_keeps_semantic_status_separate_from_physical_positive_evidence() {
    let evidence = evidence_modalities();
    let mut rows = 0usize;
    for line in PROFILE_COVERAGE
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 7, "bad profile-coverage row: {line}");
        assert!(matches!(cols[2], "COMPLETE" | "PARTIAL" | "UNKNOWN"));
        assert!(
            matches!(cols[3], "physical" | "virtual" | "static")
                || (cols[3] == "none" && cols[4] == "MISSING_PHYSICAL"),
            "absent positive wire evidence is only valid for MISSING_PHYSICAL: {line}"
        );
        assert!(matches!(cols[4], "COVERED" | "MISSING_PHYSICAL"));
        let ids: Vec<_> = cols[5].split(';').filter(|id| !id.is_empty()).collect();
        assert!(
            !ids.is_empty(),
            "profile coverage needs evidence ids: {line}"
        );
        for id in &ids {
            assert!(
                evidence.contains_key(id),
                "unknown profile evidence id {id}: {line}"
            );
        }
        match cols[4] {
            "COVERED" => assert!(
                ids.iter().any(|id| evidence[id] == "physical"),
                "COVERED profile must cite a physical capture: {line}"
            ),
            "MISSING_PHYSICAL" => assert!(
                ids.iter().all(|id| evidence[id] != "physical"),
                "MISSING_PHYSICAL must not cite a physical positive as coverage: {line}"
            ),
            _ => unreachable!(),
        }
        rows += 1;
    }
    assert!(rows > 0, "profile coverage ledger must not be empty");
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
    assert!(DOC.contains("8×全零 + 11×UsbMainBSec + 1×Netac"));
    assert!(DOC.contains("调用链来源开放问题"));
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
fn cems2_join59_reader_can_close_wire_semantics_without_becoming_a_writer() {
    let lba6 = LEDGER
        .lines()
        .find(|line| line.starts_with("6\t03f\tCOMPLETE\t"))
        .expect("LBA6 join boundary row");
    let lba9 = LEDGER
        .lines()
        .find(|line| line.starts_with("9\t080-0ff\tCOMPLETE\t"))
        .expect("LBA9 continuation row");

    for row in [lba6, lba9] {
        assert!(row.contains("S-FILEOPHOOK-2022"));
        assert!(row.contains("S-JOIN59-SEMANTIC"));
        assert!(row.contains("Exact") || row.contains("exact"));
        assert!(row.contains("writer") || row.contains("producer"));
    }
    assert!(DOC.contains("fcn.10026280"));
    assert!(DOC.contains("GENERIC_READ"));
    assert!(DOC.contains("fcn.18002B880"));
    assert!(DOC.contains("不能据此推导出扇区写入端"));
    assert!(DOC.contains("实现来源"));
}

#[test]
fn legacy_mbr_snapshot_can_close_semantics_without_inventing_the_old_copy_site() {
    let row = LEDGER
        .lines()
        .find(|line| line.starts_with("6\t1e0-1ed\tCOMPLETE\t"))
        .expect("LBA6 legacy MBR snapshot row");
    assert!(row.contains("S-MBR-SNAPSHOT-SEMANTIC"));
    assert!(row.contains("P-EESI-NETAC"));
    assert!(row.contains("C1 FF 07 EF FF FF"));
    assert!(row.contains("implementation provenance"));
    assert!(DOC.contains("P-EESI-NETAC"));
    assert!(DOC.contains("type4 PartionSize/512"));
    assert!(DOC.contains("精确历史复制点/配置类型选择器"));
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
    assert!(DOC.contains("调用方负责 `HSerialCRC[5]`"));
    assert!(DOC.contains("512/512与物理金标完全一致"));
}

#[test]
fn local_labeltool_patches_cannot_be_mistaken_for_official_protocol_evidence() {
    let note = include_str!("../audit/protocol/notes/labeltool_variant_diff.md");
    for required in [
        "b530a82b29bbc43be8d415225392ca135ab7df8a8d9f69c6598493c4942e9e11",
        "0x00449B76",
        "0x0042DDC0",
        "0x004289FB",
        "绝不能作为任何 LBA 字段的官方写入端证据",
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
    let note = include_str!("../audit/protocol/notes/devicenumber_identity.md");
    for required in [
        "0ef94c3679da6f27eac75959cf299bbad19676c251d88f7554fbc305407d6041",
        "EDP_DeviceNumber @ 0x10011E00",
        "EDP_DiskNumber @ 0x10012C90",
        "0xEDB88320",
        "0x100130A2",
        "fcn.10013210",
        "标准反射 IEEE CRC-32",
        "LBA4 `HSerialCRC[5]` 已作为调用方负责的五 DWORD 身份向量完全闭环",
        "单个 DWORD",
        "UsbLabelParam::HDOnlySerial[5]",
        "明确**不是** HSerial 生成算法",
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
    let note = include_str!("../audit/protocol/notes/lba3_manufacturer_boundary.md");
    for required in [
        "file_size - 0x200",
        "CBaseController::virtual_464",
        "标记页读取器/兼容性检查器",
        "0x459C7EB5",
        "0x22A482A8",
        "本地采集不足以确定 PS2307 或 PS2309",
        "仍**没有** USB 序列号",
    ] {
        assert!(
            note.contains(required),
            "LBA3 identity/manufacturing gate lost evidence boundary: {required}"
        );
    }
}
