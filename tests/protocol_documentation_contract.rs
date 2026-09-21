//! Contract tests for the protocol reverse-engineering ledger.
//!
//! The documentation is part of the protocol safety boundary: a field must not
//! be called COMPLETE unless producer, consumer and real-device evidence are
//! recorded together.

use std::path::Path;

const DOC: &str = include_str!("../docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md");

fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let start_pos = text.find(start).expect("missing start marker") + start.len();
    let rest = &text[start_pos..];
    let end_pos = rest.find(end).expect("missing end marker");
    &rest[..end_pos]
}

#[test]
fn protocol_analysis_has_one_canonical_document() {
    assert!(
        Path::new("docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md").is_file(),
        "canonical protocol analysis document is missing"
    );

    for obsolete in [
        "docs/PROTOCOL_BYTE_TRACE_2026-09-19.md",
        "docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md",
        "docs/HANDOFF_PROVISION_NEW_USB_2026-09-19.md",
        "docs/HISTORICAL_DLL_TARGETS_2026-09-21.md",
        "docs/PHISON_F2_TRACE_2026-09-21.md",
    ] {
        assert!(
            !Path::new(obsolete).exists(),
            "obsolete parallel protocol document must not reappear: {obsolete}"
        );
    }

    for required_section in [
        "# EDP LBA0–LBA12 协议逆向与验证总文档",
        "## 3. 严格逐字节进度",
        "## 4. 字段证据账本",
        "## 7. 代码与测试门禁",
        "## 8. 后续提升顺序",
        "## 10. 验证历程附录",
        "## 11. 历史 DLL / profile 取证目标",
        "## 12. Phison F2 / LBA3 专项取证",
    ] {
        assert!(
            DOC.contains(required_section),
            "canonical protocol document lost required total/branch structure: {required_section}"
        );
    }
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
        complete >= 5586,
        "strict COMPLETE coverage regressed below the corrected audited baseline: {complete}"
    );
    assert!(
        partial <= 1070,
        "PARTIAL coverage regressed above the corrected audited baseline: {partial}"
    );
    assert_eq!(
        unknown, 0,
        "all LBA0..12 bytes are at least PARTIAL after the completed UNKNOWN audit"
    );
    assert!(
        DOC.contains("COMPLETE：5586B / 6656B = 83.9%")
            && DOC.contains("PARTIAL：1070B / 6656B = 16.1%"),
        "displayed global totals must match the corrected strict-progress ledger"
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
        "Phison MP/manufacturing metadata sector, EDP-opaque",
        "MPALL_F1_9000_v372_0B.exe",
        "CBaseController::WriteF2Mark",
        "CU32SSBaseContoller::WriteF2Mark",
        "F1-F2 MARK",
        "公开同 identity/capacity 记录同时存在 PS2307 与 PS2309",
        "LBA8 static version/writeTime/reserved header",
        "LBA6 C-string slots have profile-dependent post-NUL backing bytes",
        "LBA6 m_encrypt current producer !SAFE gate",
        "write-only `!SAFE` label-generation metadata",
        "write-owned `m_crcUsbID[0]` identity/key metadata",
        "write-owned doubled CRC compatibility guard",
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
        "fcn.10006370",
        "排除 v19.11.4.1 作为 nonzero snapshot producer",
        "fcn.10022f80@0x10022F80",
        "CEMS2.0 join59 reader only",
        "all locally available marker writers use 60",
        "LBA4 current writer machine-code node layout",
        "LBA4.MyHardinfo == LBA8.HDSerialInfo",
        "historical restore-node reader/activation consumer",
        "ISUdiskRegsiterObj::virtual_8@0x1000B9C0",
        "request+0x150..+0x160",
        "ReadUsbHserialsInfo",
        "RestoreRegsiterUsb",
        "fixed restore-node `SingleUsbFlg` metadata",
        "fixed restore-node `NewLabFlag = LLGB`",
        "fixed restore-node `Version = 1`",
        "fixed restore-node sector tuple `08 04 0C 01`",
        "LBA12 v0x0206 hidden default-password file-key wrapping",
        "LBA12 alternate wrapping-mode algorithm map",
        "EESI caller-owned compatibility extension",
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
        "cross-profile fixed-zero bootstrap tail padding",
        "seven profile-invariant zero instruction-operand bytes",
        "first/second legacy MBR error-message NUL terminators",
        "aigo_l8302_netac_lba0_prefix.hex",
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
        "/Users/zhangyuxi/.edpcli-backup",
        "/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4",
        "analyze/last/three_disks/disk3/raw/front_LBA0_12.bin",
        "764fc0bbbdf5a9980498d710cb90bce3f8b623f66dec07cb580debc4869538b4",
        "os.open(dev, os.O_RDONLY)",
        "edpcli 自制免密盘，只能用于产品回归，**没有协议参考价值**",
    ] {
        assert!(
            DOC.contains(required),
            "protocol ledger lost required evidence: {required}"
        );
    }
    assert!(
        DOC.contains("| LBA0 | 142 | 370 | 0 | 27.7% |"),
        "LBA0 progress must retain the closed invariant bootstrap tail, SectorSize overlay, MBR signature, and compatibility regions"
    );
    assert!(
        DOC.contains("| LBA1 | 512 | 0 | 0 | 100.0% |"),
        "LBA1 must retain the closed official GPT positive-wire and absent-GPT profiles"
    );
    assert!(
        DOC.contains("| LBA2 | 512 | 0 | 0 | 100.0% |"),
        "LBA2 must retain the closed entry0 and unused-entry residual semantics"
    );
    assert!(
        DOC.contains("| LBA4 | 487 | 25 | 0 | 95.1% |"),
        "LBA4 progress must retain the closed backup-key seed and restore-node backing semantics"
    );
    assert!(
        DOC.contains("| LBA6 | 497 | 15 | 0 | 97.1% |"),
        "LBA6 progress must retain the closed string slots, crcUsbID pair, m_encrypt metadata, and static template regions"
    );
    assert!(
        DOC.contains("| LBA7 | 512 | 0 | 0 | 100.0% |"),
        "LBA7 progress must retain the fully closed packed table and pass-info compatibility fields"
    );
    assert!(
        DOC.contains("| LBA8 | 492 | 20 | 0 | 96.1% |"),
        "LBA8 progress must retain the closed dynamic ELABEL/backing/tail storage semantics"
    );
    assert!(
        DOC.contains("| LBA9 | 384 | 128 | 0 | 75.0% |"),
        "LBA9 progress must retain the closed EETU reverse backing and EPPE writer-owned zero tail semantics"
    );
    assert!(
        DOC.contains("| LBA10 | 512 | 0 | 0 | 100.0% |"),
        "LBA10 must be fully closed once the read-only SanDisk EESI gold capture is pinned"
    );
    assert!(
        DOC.contains("| LBA12 | 512 | 0 | 0 | 100.0% |"),
        "LBA12 progress must retain the closed Version/NeedDisturb, EncryptFileKey32, and dormant pass-info compatibility fields"
    );
    assert!(
        DOC.contains("| LBA11 | 512 | 0 | 0 | 100.0% |"),
        "LBA11 must remain fully closed once the CHS repair writer/reader profile is accounted for"
    );
}
