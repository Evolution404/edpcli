use edpcli::tui::{
    disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind},
    ui::{
        render_operation_result, OperationResultSpec, ResultCard, ResultField, ResultSupplement,
        ResultTable, ResultTone, ResultValue,
    },
};
use ratatui::{backend::TestBackend, layout::Constraint, Terminal};

fn rendered(width: u16, height: u16) -> String {
    let spec = OperationResultSpec {
        title: "制盘结果".into(),
        status: "制盘成功".into(),
        tone: ResultTone::Success,
        fields: vec![
            ResultField::new("目标设备", ResultValue::primary("disk4").emphasized()),
            ResultField::new(
                "盘型",
                ResultValue {
                    text: "mode0（缺省三分区）".into(),
                    tone: ResultTone::Accent,
                    bold: true,
                },
            ),
            ResultField::new("总容量", ResultValue::primary("117.19 GiB").emphasized()),
            ResultField::new("验收", ResultValue::success("全部读回验证通过")),
        ],
        table: Some(ResultTable {
            title: "分区结果".into(),
            headers: vec![
                "分区".into(),
                "角色".into(),
                "文件系统".into(),
                "起始 LBA".into(),
                "大小".into(),
                "状态".into(),
            ],
            rows: vec![
                vec![
                    ResultValue::primary("P1"),
                    ResultValue::primary("启动区"),
                    ResultValue::primary("fat16"),
                    ResultValue::muted("63"),
                    ResultValue::primary("9.97 MiB"),
                    ResultValue::success("已格式化 · 读回通过"),
                ],
                vec![
                    ResultValue::primary("P2"),
                    ResultValue::primary("交换区"),
                    ResultValue::primary("exfat"),
                    ResultValue::muted("20480"),
                    ResultValue::primary("31.00 GiB"),
                    ResultValue::success("已格式化 · 读回通过"),
                ],
            ],
            widths: vec![
                Constraint::Length(6),
                Constraint::Length(14),
                Constraint::Length(10),
                Constraint::Length(13),
                Constraint::Length(12),
                Constraint::Min(18),
            ],
            supplement: Some(ResultSupplement::DiskCapacityMap {
                title: "新盘全局布局".into(),
                model: DiskLayoutModel::new(
                    1_000,
                    vec![
                        DiskLayoutSegment {
                            label: "EDP 主协议区".into(),
                            start_lba: 0,
                            sector_count: 13,
                            kind: DiskRegionKind::Protocol,
                        },
                        DiskLayoutSegment {
                            label: "启动/交换区".into(),
                            start_lba: 13,
                            sector_count: 700,
                            kind: DiskRegionKind::Combined,
                        },
                        DiskLayoutSegment {
                            label: "保密区".into(),
                            start_lba: 713,
                            sector_count: 200,
                            kind: DiskRegionKind::Encrypt,
                        },
                        DiskLayoutSegment {
                            label: "尾部区域".into(),
                            start_lba: 913,
                            sector_count: 87,
                            kind: DiskRegionKind::Tail,
                        },
                    ],
                ),
            }),
        }),
        cards: vec![
            ResultCard {
                title: "验收结果".into(),
                lines: vec![
                    ResultValue::success("✓ 制盘前自动备份已创建"),
                    ResultValue::success("✓ 协议与几何读回验证通过"),
                ],
            },
            ResultCard {
                title: "执行信息".into(),
                lines: vec![
                    ResultValue::primary("备份文件  disk4.edpb"),
                    ResultValue::primary("总耗时  19 秒"),
                ],
            },
        ],
        footer: "Enter / Esc 返回制盘中心".into(),
    };
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| render_operation_result(frame, frame.area(), &spec))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn shared_result_page_prioritizes_outcome_in_wide_and_narrow_viewports() {
    for (width, height) in [(80, 28), (140, 40)] {
        let text = rendered(width, height).replace(' ', "");
        for required in [
            "制盘成功",
            "目标设备",
            "disk4",
            "盘型",
            "mode0",
            "总容量",
            "分区结果",
            "启动区",
            "9.97MiB",
            "交换区",
            "31.00GiB",
            "新盘全局布局",
            "区域列表",
            "LBA范围",
            "验收结果",
            "自动备份已创建",
            "总耗时",
        ] {
            assert!(
                text.contains(required),
                "missing {required} at {width}x{height}"
            );
        }
        if width >= 110 {
            for marker in ["0%", "100%"] {
                assert!(
                    text.contains(marker),
                    "missing capacity marker {marker} at {width}x{height}"
                );
            }
            for region in ["EDP主协议区", "启动/交换区", "保密区", "尾部区域"] {
                assert!(
                    text.contains(region),
                    "missing region {region} at {width}x{height}"
                );
            }
        }
        assert!(!text.contains("最近进度事件"));
    }
}

#[test]
fn provision_result_snapshot_rebuilds_complete_official_disk_layout() {
    use edpcli::{
        provision::{OfficialPartitionMode, PartitionRole, ProvisionTarget},
        tui::state::{ProvisionResultPartition, ProvisionResultSnapshot},
    };

    let total_sectors = 245_760_000u64;
    let snapshot = ProvisionResultSnapshot {
        disk: 4,
        target: ProvisionTarget::Official(OfficialPartitionMode::BootShareCombined),
        total_bytes: total_sectors * 512,
        partitions: vec![
            ProvisionResultPartition {
                role: Some(PartitionRole::BootShareCombined),
                filesystem: None,
                start_lba: 63,
                size_bytes: 100_000 * 512,
                selected_for_format: true,
                disposition: None,
            },
            ProvisionResultPartition {
                role: Some(PartitionRole::Encrypt),
                filesystem: None,
                start_lba: 200_000,
                size_bytes: 1_000_000 * 512,
                selected_for_format: true,
                disposition: None,
            },
        ],
    };

    let model = snapshot.disk_layout_model().expect("result disk layout");
    model
        .validate_complete()
        .expect("complete result disk layout");
    let collapsed = model.collapsed_tail_model();
    for kind in [
        DiskRegionKind::Protocol,
        DiskRegionKind::Combined,
        DiskRegionKind::Encrypt,
        DiskRegionKind::Tail,
    ] {
        assert!(
            collapsed
                .segments
                .iter()
                .any(|segment| segment.kind == kind),
            "missing {kind:?}"
        );
    }
}

#[test]
fn provision_result_page_attaches_interactive_shared_full_disk_layout() {
    let source = include_str!("../src/tui/provision/result_render.rs");
    let layout = include_str!("../src/tui/provision/result_partition_layout.rs");
    let regions = include_str!("../src/tui/disk_region_list/render.rs");
    let workbench = include_str!("../src/tui/result_workbench.rs");
    let input = include_str!("../src/tui/runtime_input/provision.rs");
    let supplement = include_str!("../src/tui/ui/result_supplement.rs");
    let devices = include_str!("../src/tui/devices/presentation.rs");

    assert!(source.contains("render_result_workbench_shell"));
    assert!(layout.contains("disk_layout_model"));
    assert!(layout.contains("DiskCapacityMapProfile::Full"));
    assert!(layout.contains("TailExpansion::Collapsed"));
    assert!(layout.contains("render_result_region_list"));
    assert!(layout.contains(".with_marker(true)"));
    assert!(!layout.contains(".with_marker(false)"));

    assert!(
        layout.contains("theme.apply_selection(theme.table_text(), selected_row, focused)"),
        "result rows must use the shared table selection background"
    );
    assert!(
        layout.contains("theme.table_cell(base, column == active_column, focused)"),
        "only the active result column may receive bright/bold table-cell emphasis"
    );
    assert!(
        !layout.contains("row.style(theme.accent"),
        "result rows must never regress to accent/blue whole-row activation"
    );
    assert!(
        regions.contains("theme.apply_selection(theme.disk_region(segment.kind), active, focused)"),
        "disk region rows must share table-style background selection"
    );
    assert!(
        workbench.contains("render_disk_region_list_body"),
        "full disk layout must remain one top-level pane without a nested focused card"
    );
    assert!(
        input.contains("provision_result_shift_partition_column"),
        "result h/l semantics must keep partition-column navigation"
    );

    assert!(supplement.contains("render_disk_region_list"));
    assert!(supplement.contains("DiskRegionListMode::Readonly"));
    assert!(devices.contains("disk_region_list_lines"));
}


#[test]
fn restore_post_result_uses_shared_workbench_and_typed_layout() {
    let render = include_str!("../src/tui/restore_result_render.rs");
    let partition_layout = include_str!("../src/tui/restore_result_partition_layout.rs");
    let verification = include_str!("../src/tui/restore_result_verification.rs");
    let state = include_str!("../src/tui/restore_result_state.rs");
    let input = include_str!("../src/tui/runtime_input/backup_wizard.rs");
    let root = include_str!("../src/tui/render.rs");
    let state_prod = state.split("#[cfg(test)]").next().unwrap_or(state);
    let render_sources = format!("{render}\n{partition_layout}\n{verification}");

    assert!(root.contains("draw_post_restore_result(frame, area, state)"));
    assert!(root.contains(
        "WizardStage::PostRestore => unreachable!(\"post-restore uses shared result workbench\")"
    ));
    assert!(render.contains("render_result_workbench_shell"));
    for pane in ["分区结果", "全盘布局", "验收与执行"] {
        assert!(
            render_sources.contains(pane),
            "missing restore result pane {pane}"
        );
    }
    assert!(partition_layout.contains(".with_marker(true)"));
    assert!(
        partition_layout
            .contains("theme.apply_selection(theme.table_text(), selected_row, focused)"),
        "restore result rows must use the shared selected-row background"
    );
    assert!(
        partition_layout
            .contains("theme.table_cell(base, column == active_column, focused)"),
        "restore result active column must use the shared bright/bold cell style"
    );
    assert!(
        !render_sources.contains("row.style(theme.accent"),
        "restore result must never use accent/blue whole-row activation"
    );

    assert!(state_prod.contains("outcome.layout.as_ref()"));
    assert!(state_prod.contains("partition_index_for_selection"));
    assert!(
        !state_prod.contains(".role"),
        "TUI result geometry must not parse or infer string partition roles"
    );
    assert!(
        input.contains("post_restore_result_focused_pane")
            && input.contains("PaneId::ResultPartitions"),
        "post-restore actions must only activate from the partition pane"
    );
}
