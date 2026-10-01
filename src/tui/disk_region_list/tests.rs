use super::*;
use crate::tui::disk_layout::{
    DiskCapacitySelection, DiskLayoutModel, DiskLayoutSegment, DiskRegionKind,
};

fn fixture() -> DiskLayoutModel {
    DiskLayoutModel::new(
        100,
        vec![
            DiskLayoutSegment {
                label: "Protocol".into(),
                start_lba: 0,
                sector_count: 10,
                kind: DiskRegionKind::Protocol,
            },
            DiskLayoutSegment {
                label: "Boot".into(),
                start_lba: 10,
                sector_count: 40,
                kind: DiskRegionKind::Boot,
            },
            DiskLayoutSegment {
                label: "Free".into(),
                start_lba: 50,
                sector_count: 50,
                kind: DiskRegionKind::Free,
            },
        ],
    )
}

#[test]
fn selection_is_stable_geometry_not_row_ordinal() {
    let model = fixture();
    let mut state = DiskRegionListState::default();
    assert!(state.select_index(&model, 1, 4));
    let selected = state.selection().expect("selection");
    assert_eq!(selected.start_lba, 10);
    assert_eq!(selected.end_exclusive, 50);
    assert_eq!(selected.kind, DiskRegionKind::Boot);

    let with_inserted_region = DiskLayoutModel::new(
        100,
        vec![
            DiskLayoutSegment {
                label: "Protocol".into(),
                start_lba: 0,
                sector_count: 5,
                kind: DiskRegionKind::Protocol,
            },
            DiskLayoutSegment {
                label: "Metadata".into(),
                start_lba: 5,
                sector_count: 5,
                kind: DiskRegionKind::Metadata,
            },
            DiskLayoutSegment {
                label: "Boot".into(),
                start_lba: 10,
                sector_count: 40,
                kind: DiskRegionKind::Boot,
            },
            DiskLayoutSegment {
                label: "Free".into(),
                start_lba: 50,
                sector_count: 50,
                kind: DiskRegionKind::Free,
            },
        ],
    );
    assert_eq!(state.selected_index(&with_inserted_region), Some(2));
}

#[test]
fn geometry_bridge_requires_exact_region_match() {
    let model = fixture();
    let mut state = DiskRegionListState::default();
    let boot = DiskCapacitySelection {
        start_lba: 10,
        end_exclusive: 50,
        kind: DiskRegionKind::Boot,
    };
    assert!(state.select_geometry(&model, &boot, 4));
    assert_eq!(state.selected_index(&model), Some(1));

    let partial = DiskCapacitySelection {
        start_lba: 11,
        end_exclusive: 50,
        kind: DiskRegionKind::Boot,
    };
    assert!(!state.select_geometry(&model, &partial, 4));
    assert_eq!(state.selection(), Some(boot));
}

#[test]
fn moving_selection_updates_map_bridge_and_viewport() {
    let model = fixture();
    let mut state = DiskRegionListState::default();
    state.reconcile(&model, 1);
    assert_eq!(state.selected_index(&model), Some(0));
    assert!(state.move_selection(&model, 1, 1));
    assert_eq!(state.selected_index(&model), Some(1));
    assert_eq!(state.viewport.offset, 1);
    assert_eq!(
        state.selection().map(|selection| selection.kind),
        Some(DiskRegionKind::Boot)
    );
}
