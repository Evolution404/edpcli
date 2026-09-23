//! Contract tests for the protocol reverse-engineering ledger.
//!
//! The documentation is part of the protocol safety boundary. Semantic closure
//! and real-device profile coverage are tracked independently so first-party
//! runtime evidence is never mislabeled as a physical capture.

use std::path::Path;

const DOC: &str = include_str!("../docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md");
const PROTOCOL_INDEX: &str = include_str!("../docs/protocol/README.md");
const LCE: &str = include_str!("../docs/protocol/LCE.md");

fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let start_pos = text.find(start).expect("missing start marker") + start.len();
    let rest = &text[start_pos..];
    let end_pos = rest.find(end).expect("missing end marker");
    &rest[..end_pos]
}

#[test]
fn protocol_analysis_has_one_canonical_document() {
    assert!(
        Path::new("docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md").is_file(),
        "canonical protocol analysis document is missing"
    );

    for obsolete in [
        "docs/PROTOCOL_BYTE_TRACE_2026-09-19.md",
        "docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md",
        "docs/HANDOFF_PROVISION_NEW_USB_2026-09-19.md",
        "docs/HISTORICAL_DLL_TARGETS_2026-09-21.md",
        "docs/PHISON_F2_TRACE_2026-09-21.md",
        "docs/EDP_PROTOCOL_REVERSE_ENGINEERING_STATUS.md",
        "docs/EDP_PROTOCOL_LIVE_STATUS.md",
        "docs/EDP_PROTOCOL_ENGINEERING_PLAN.md",
        "docs/PROVISION_NEW_USB_PLAN_2026-09-19.md",
    ] {
        assert!(
            !Path::new(obsolete).exists(),
            "obsolete parallel protocol document must not reappear: {obsolete}"
        );
    }

    assert!(DOC.contains("调用方负责身份/材料"));
    assert!(DOC.contains("来源不属于盘面字段语义缺口"));

    for required_section in [
        "# EDP LBA0–LBA12 协议逆向与验证总文档",
        "## 3. 严格逐字节进度",
        "## 4. 字段证据账本",
        "## 7. 代码与测试门禁",
        "## 8. 后续证据提升（不改变当前 6656B 完全闭环基线）",
        "## 10. 验证历程附录",
        "## 11. 历史 DLL / 配置类型取证目标",
        "## 12. Phison F2 / LBA3 专项取证",
    ] {
        assert!(
            DOC.contains(required_section),
            "canonical protocol document lost required total/branch structure: {required_section}"
        );
    }
}

#[test]
fn protocol_index_and_lce_replace_parallel_live_status_document() {
    assert!(Path::new("docs/protocol/README.md").is_file());
    assert!(Path::new("docs/protocol/LCE.md").is_file());
    for required in [
        "6656/6656B COMPLETE",
        "LCE = LBA7 兼容扩展区",
        "audit/protocol/profile_coverage.tsv",
    ] {
        assert!(
            PROTOCOL_INDEX.contains(required),
            "protocol index lost required boundary: {required}"
        );
    }
    for required in [
        "物理位置/大小",
        "LBA7 写入端/指针生成",
        "驱动物理读写映射",
        "上层业务触发",
        "**OPEN**",
    ] {
        assert!(
            LCE.contains(required),
            "LCE document lost closure boundary: {required}"
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
    let complete_pct = complete as f64 * 100.0 / (13 * 512) as f64;
    let partial_pct = partial as f64 * 100.0 / (13 * 512) as f64;
    let unknown_pct = unknown as f64 * 100.0 / (13 * 512) as f64;
    assert!(
        DOC.contains(&format!("COMPLETE：{complete}B / 6656B = {complete_pct:.1}%"))
            && DOC.contains(&format!("PARTIAL：{partial}B / 6656B = {partial_pct:.1}%"))
            && DOC.contains(&format!("UNKNOWN：{unknown}B / 6656B = {unknown_pct:.1}%")),
        "displayed semantic totals must match the strict-progress ledger without enforcing a historical completion floor"
    );
}

#[test]
fn every_complete_field_has_producer_consumer_and_declared_evidence_boundary() {
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
        let observed_evidence = cols[7];

        for (label, value) in [
            ("producer", producer),
            ("consumer", consumer),
            ("observed evidence", observed_evidence),
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
        "共享/type2 卷标",
        "加密/type4 卷标",
        "UsbSuspensionWnd 生命周期/控制标志",
        "EdpEDisk.exe::OnInitDialog",
        "将完整 0x80B EESI 负载清零",
        "特定用途物理正向",
        "audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.bin",
        "d72f6fcd192e92e6423e3b078cffa46725d8e781cdbd48dfcdaa470b72d208bd",
        "P-EESI-NETAC",
        "3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39",
        "tagEdpEDiskTmpUse",
        "edpdiskglobal.h:481",
        "useCount=0xFFFFFFFF",
        "EETU reverse[0..101] 写入端未初始化不透明保留底层字节",
        "运行时保留完整 0x80 EETU，同时只消费 time/useCount",
        "OutManage switch is off",
        "不透明原样保留 / 写保护探测临时扇区",
        "ERROR_WRITE_PROTECT(0x13)",
        "076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560",
        "BuildSector1_Gpt",
        "BuildSector2_Gpt",
        "GPT_Header",
        "GPT_Partition",
        "overflow_marker(0x40245E2A)",
        "m_autoid / Autonum",
        "同一个空字符串至少出现2种不同且非零的 NUL 后保留底层字节",
        "同一个空办公字符串至少出现3种不同且非零的 NUL 后保留底层字节",
        "manufacturer-owned opaque MP metadata / EDP preserve-only sector",
        "MPALL_F1_9000_v372_0B.exe",
        "CBaseController::WriteF2Mark",
        "CU32SSBaseContoller::WriteF2Mark",
        "F1-F2 MARK",
        "公开同身份/容量记录同时存在 PS2307 与 PS2309",
        "LBA8 静态版本/writeTime/保留头",
        "LBA6 C 字符串槽具有随配置类型变化的 NUL 后保留底层字节",
        "LBA6 m_encrypt 当前写入端 `!SAFE` 门禁",
        "只写 `!SAFE` 标签代际元数据",
        "写入端负责 `m_crcUsbID[0]` 身份/密钥元数据",
        "写入端负责双倍 CRC 兼容保护值",
        "写入端未初始化保留底层字节",
        "strcpy_s@0x1B9B0",
        "MacInfo[6]",
        "LBA8 动态 ELABEL + 加密后的保留底层字节 + 保留尾部",
        "registration semantic reader key set = Label/GLab/Dept/User/Autonum/Rmark/Unit",
        "runtime EdpEDiskCtrl reader parses all 17 ELABEL keys",
        "ELABEL NUL 后到 encrypted_len 的字节属于既有 backing",
        "跨代无所有者、原样保留/忽略",
        "原始全零/完整滚动物理表示与当前/旧版身份不是同一个维度",
        "85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec",
        "旧版 MBR 分区表片段",
        "fcn.10006370",
        "排除 v19.11.4.1 作为非零快照写入端",
        "fcn.10022f80@0x10022F80",
        "CEMS2.0 join59 读取端仅",
        "本地现有全部标记写入端都使用 60",
        "LBA4 当前写入端机器码节点布局",
        "LBA4.MyHardinfo == LBA8.HDSerialInfo",
        "历史恢复节点读取端/激活消费端",
        "ISUdiskRegsiterObj::virtual_8@0x1000B9C0",
        "request+0x150..+0x160",
        "ReadUsbHserialsInfo",
        "RestoreRegsiterUsb",
        "固定恢复节点 `SingleUsbFlg` 元数据",
        "固定恢复节点 `NewLabFlag = LLGB`",
        "固定恢复节点 `Version = 1`",
        "固定恢复节点扇区元组 `08 04 0C 01`",
        "LBA12 v0x0206 隐藏的默认密码文件密钥封装",
        "LBA12 备用封装模式算法映射",
        "EESI 调用方负责的兼容扩展",
        "normalDetail.algorithm",
        "LabelInfo.crypt",
        "WriteNormalULabel",
        "this+0x6EC",
        "LBA12 打包 `Reserved[7]` 写入端/负消费端闭环",
        "LBA12 EncryptFileKey32 兼容槽结构缓存 / 负语义消费端闭环",
        "紧凑布局条目内 `Version` 兼容元数据",
        "全树22份完整历史备份",
        "旧版 72 字节 ABI 不存在 EncryptFileKey32 槽",
        "LBA0 旧版 MBR 消息指针字节",
        "4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed",
        "lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix",
        "跨配置类型无所有者原样保留 / 历史全零兼容区域",
        "跨配置类型固定全零引导代码尾部填充",
        "七个跨配置类型不变的全零指令操作数字节",
        "第一/第二条旧版 MBR 错误消息",
        "aigo_l8302_netac_lba0_prefix.hex",
        "可选 SAFE1 / 旧版 `SectorSize` 兼容覆盖项",
        "2c8877b90c5d42d73f17c511ef5984efc8135543bda0746ef54f347320d78e8f",
        "标准 Windows MBR 磁盘签名",
        "CEMSUsbRegsiter.dll::fcn.10046320",
        "共57份完整历史快照",
        "Netac_USB_API.dll::sub_10003880",
        "00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec",
        "official_usb_main_bsec_lba0_prefix.hex",
        "official_netac_mbr_lba0_prefix.hex",
        "8×全零 + 11×UsbMainBSec + 1×Netac",
        "S-NETAC-MBR",
        "rep movsd, ECX=0x80",
        "CCEMSSafeUsbRegsiter::UsbFormat",
        "usb20dll.dll!_IF_DiskFormat",
        "NewUsb20.dll!FormatExA_NetacAPI",
        "LBA7 打包 64 字节 ABI 与 Linux 自然对齐 72 字节 ABI",
        "LBA7 v0x0064 打包旧版文件密钥封装",
        "兼容元数据生命周期闭合",
        "CDiskReader::GetTagPartitionInfo",
        "正式 ABI 兼容元数据",
        "未启用密码信息",
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
        "旧版 join=59",
        "ReWrite11Sector",
        "IOCTL_DISK_GET_DRIVE_GEOMETRY",
        "Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector",
        "22/22",
        "不能单独把字段升级为完全闭环",
        "禁止把免密转换盘",
        "19 份 SHA-256 唯一",
        "20份唯一金标",
        "完整 6656B SHA-256 去重",
        "audit/protocol/gold/strict-encrypted/",
        "audit/protocol/gold/authentic-nopwd/",
        "/Users/zhangyuxi/.edpcli-backup",
        "/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4",
        "用于产品回归，**没有协议参考价值**",
    ] {
        assert!(
            DOC.contains(required),
            "protocol ledger lost required evidence: {required}"
        );
    }
    assert!(
        DOC.contains("| LBA0 | 512 | 0 | 0 | 100.0% |"),
        "LBA0 progress must retain all three closed bootstrap producer profiles plus the already-closed compatibility and partition regions"
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
        DOC.contains("| LBA4 | 512 | 0 | 0 | 100.0% |"),
        "LBA4 progress must retain the caller-owned HSerial and bDataToServer closures"
    );
    assert!(
        DOC.contains("| LBA6 | 512 | 0 | 0 | 100.0% |"),
        "LBA6 progress must retain the closed string slots, join discriminator, deterministic legacy MBR snapshot/zero-underlay profiles, crcUsbID pair, m_encrypt metadata, and static template regions"
    );
    assert!(
        DOC.contains("| LBA7 | 512 | 0 | 0 | 100.0% |"),
        "LBA7 progress must retain the fully closed packed table and pass-info compatibility fields"
    );
    assert!(
        DOC.contains("| LBA8 | 512 | 0 | 0 | 100.0% |"),
        "LBA8 progress must retain the closed dynamic ELABEL/backing/tail storage semantics"
    );
    assert!(
        DOC.contains("| LBA9 | 512 | 0 | 0 | 100.0% |"),
        "LBA9 progress must retain the closed EETU, join59/join60 continuation, SAPF/User overlap, and EPPE semantics"
    );
    assert!(
        DOC.contains("| LBA10 | 512 | 0 | 0 | 100.0% |"),
        "LBA10 must remain fully closed once the provenance-audited positive EESI physical profile is accounted for"
    );
    assert!(
        DOC.contains("P-EESI-NETAC")
            && DOC.contains("19+1 通用统计集")
            && DOC.contains("不纳入19+1 通用统计集"),
        "the positive EESI capture must remain purpose-specific physical evidence rather than silently changing the 19+1 通用统计集"
    );
    assert!(
        !DOC.contains("front_LBA0_12.bin"),
        "obsolete third-source audit22 material must not be promoted into the current general census"
    );
    assert!(
        DOC.contains("| LBA12 | 512 | 0 | 0 | 100.0% |"),
        "LBA12 progress must retain the closed Version/NeedDisturb, EncryptFileKey32, and 未启用密码信息 compatibility fields"
    );
    assert!(
        DOC.contains("| LBA11 | 512 | 0 | 0 | 100.0% |"),
        "LBA11 must remain fully closed once the CHS repair writer/reader profile is accounted for"
    );
}
