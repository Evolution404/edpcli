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
        "UsbSuspensionWnd lifecycle/control flag",
        "EdpEDisk.exe::OnInitDialog",
        "zero-initializes the full 0x80B EESI payload",
        "netac_onlydisk_20260804_lba10_head.hex",
        "3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39",
        "240d04e7c97d300c5081f793d72850d49acbf5408bc0d8cf32de8eef7a5e8f02",
        "tagEdpEDiskTmpUse",
        "edpdiskglobal.h:481",
        "useCount=0xFFFFFFFF",
        "EETU reverse[0..101] writer-uninitialized opaque backing",
        "runtime preserves the full 0x80 EETU while only consuming time/useCount",
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
        "同一个空字符串至少出现2种不同且非零的 post-NUL backing",
        "同一个空 Office 字符串至少出现3种不同且非零的 post-NUL backing",
        "Phison MP/FW manufacturing metadata sector, EDP-opaque",
        "MPALL_F1_9000_v372_0B.exe",
        "CBaseController::WriteF2Mark",
        "CU32SSBaseContoller::WriteF2Mark",
        "F1-F2 MARK",
        "公开同 identity/capacity 记录同时存在 PS2307 与 PS2309",
        "LBA8 static version/writeTime/reserved header",
        "LBA6 C-string slots have profile-dependent post-NUL backing bytes",
        "LBA6 m_encrypt current producer !SAFE gate",
        "writer-uninitialized backing",
        "strcpy_s@0x1B9B0",
        "MacInfo[6]",
        "LBA8 dynamic ELABEL + encrypted backing + preserved tail",
        "registration semantic reader key set = Label/GLab/Dept/User/Autonum/Rmark/Unit",
        "runtime EdpEDiskCtrl reader parses all 17 ELABEL keys",
        "ELABEL NUL 后到 encrypted_len 的字节属于既有 backing",
        "cross-generation unowned preserve/ignore",
        "raw-zero/full-rolling 物理表示与 current/legacy identity 不是同一个维度",
        "85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec",
        "legacy MBR partition-table fragment",
        "LBA4 current writer machine-code node layout",
        "LBA12 v0x0206 hidden default-password file-key wrapping",
        "LBA12 alternate wrapping-mode algorithm map",
        "normalDetail.algorithm",
        "LabelInfo.crypt",
        "WriteNormalULabel",
        "this+0x6EC",
        "LBA12 packed Reserved[7] producer/negative-consumer closure",
        "LBA12 EncryptFileKey32 compatibility slot structural-cache / negative-semantic-consumer closure",
        "packed entry-local `Version` compatibility metadata",
        "全树22份完整历史备份",
        "old 72-byte ABI has no EncryptFileKey32 slot",
        "LBA0 legacy MBR message-pointer bytes",
        "4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed",
        "lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix",
        "cross-profile unowned preserve / historical-zero compatibility region",
        "optional SAFE1 / legacy `SectorSize` compatibility overlay",
        "2c8877b90c5d42d73f17c511ef5984efc8135543bda0746ef54f347320d78e8f",
        "standard Windows MBR disk signature",
        "CEMSUsbRegsiter.dll::fcn.10046320",
        "共57份完整历史快照",
        "Netac_USB_API.dll::sub_10003880",
        "00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec",
        "rep movsd, ECX=0x80",
        "CCEMSSafeUsbRegsiter::UsbFormat",
        "usb20dll.dll!_IF_DiskFormat",
        "NewUsb20.dll!FormatExA_NetacAPI",
        "LBA7 packed 64-byte ABI versus Linux natural 72-byte ABI",
       "LBA7 v0x0064 packed legacy file-key wrapping",
        "compatibility metadata 生命周期闭合",
        "CDiskReader::GetTagPartitionInfo",
        "formal ABI compatibility metadata",
        "dormant pass-info",
        "edpdiskglobal.h:164/165",
        "512 COMPLETE / 0 PARTIAL / 0 UNKNOWN",
        "Update_EDPEDISKSHOWPARAM",
        "bNoUsbChkPasSafe",
        "0x1003DC16..0x1003DC26",
        "BackupPromptInfo",
        "0xC0 = 3×0x40",
        "ordinal4=`EDP_DiskNumber`",
        "ordinal3=`EDP_DeviceNumber`",
        "MACAddress<i>=<12位大写无分隔MAC>",
        "MACCount=<N>",
        "lba9_dept_continuation_preserves_both_official_reader_join_profiles",
        "legacy join=59",
        "ReWrite11Sector",
        "IOCTL_DISK_GET_DRIVE_GEOMETRY",
        "Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector",
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
        DOC.contains("| LBA0 | 112 | 400 | 0 | 21.9% |"),
        "LBA0 progress must retain the closed SectorSize overlay, MBR signature, and compatibility regions"
    );
    assert!(
        DOC.contains("| LBA1 | 0 | 512 | 0 | 0.0% |"),
        "LBA1 GPT profile must remain PARTIAL until a positive real GPT sample exists"
    );
    assert!(
        DOC.contains("| LBA2 | 0 | 512 | 0 | 0.0% |"),
        "LBA2 GPT profile must remain PARTIAL until a positive real GPT sample exists"
    );
    assert!(
        DOC.contains("| LBA6 | 419 | 93 | 0 | 81.8% |"),
        "LBA6 progress must retain the closed Dept prefix, autoid, Office, and Label backing semantics"
    );
    assert!(
        DOC.contains("| LBA7 | 512 | 0 | 0 | 100.0% |"),
        "LBA7 progress must retain the fully closed packed table and pass-info compatibility fields"
    );
    assert!(
        DOC.contains("| LBA8 | 476 | 36 | 0 | 93.0% |"),
        "LBA8 progress must retain the closed dynamic ELABEL/backing/tail storage semantics"
    );
    assert!(
        DOC.contains("| LBA9 | 276 | 236 | 0 | 53.9% |"),
        "LBA9 progress must retain the closed EETU reverse backing and EPPE writer-owned zero tail semantics"
    );
    assert!(
        DOC.contains("| LBA10 | 424 | 88 | 0 | 82.8% |"),
        "LBA10 progress must retain the closed +0x04 control flag and preserve/ignore tail semantics"
    );
    assert!(
        DOC.contains("| LBA12 | 464 | 48 | 0 | 90.6% |"),
        "LBA12 progress must retain the closed Version/NeedDisturb, EncryptFileKey32, and dormant pass-info compatibility fields"
    );
    assert!(
        DOC.contains("| LBA11 | 512 | 0 | 0 | 100.0% |"),
        "LBA11 must remain fully closed once the CHS repair writer/reader profile is accounted for"
    );
}
