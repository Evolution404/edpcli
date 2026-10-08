//! Immutable display snapshots. These caches never authorize device operations.
use super::*;
use crate::tui::table_layout::{display_width, table_column_schema, TableKind, TableSort};
use std::{cell::RefCell, rc::Rc};

#[derive(PartialEq, Eq)]
struct ViewKey {
    generation: u64,
    query: String,
    filter: BackupDeviceFilter,
    sort: Option<TableSort>,
}

pub(crate) struct BackupViewSnapshot {
    pub indices: Vec<usize>,
    pub content_widths: Vec<usize>,
}

#[derive(Default)]
pub(super) struct BackupViewCache {
    snapshot: RefCell<Option<(ViewKey, Rc<BackupViewSnapshot>)>>,
}

impl AppState {
    pub(crate) fn backup_view_snapshot(&self) -> Rc<BackupViewSnapshot> {
        let query = if self.workspace() == Workspace::Backups {
            self.active_search_query().to_ascii_lowercase()
        } else {
            String::new()
        };
        let key = ViewKey {
            generation: self.backups.table_view.generation,
            query,
            filter: self.backups.device_filter.clone(),
            sort: self.table_interaction(TableKind::Backups).sort(),
        };
        if let Some((previous, snapshot)) = self.backups.view_cache.snapshot.borrow().as_ref() {
            if previous == &key {
                return Rc::clone(snapshot);
            }
        }
        let matches = (!key.query.is_empty()).then(|| {
            let mut mask = vec![false; self.backups.rows.len()];
            for &index in &self.shell.search_matches {
                if let Some(found) = mask.get_mut(index) {
                    *found = true;
                }
            }
            mask
        });
        let indices = self
            .backups
            .group_keys
            .iter()
            .enumerate()
            .filter_map(|(index, group)| {
                let device_matches = match &key.filter {
                    BackupDeviceFilter::All => true,
                    BackupDeviceFilter::Confirmed(expected) => group.as_ref() == Some(expected),
                    BackupDeviceFilter::Unresolved => group.is_none(),
                };
                (device_matches && matches.as_ref().is_none_or(|mask| mask[index])).then_some(index)
            })
            .collect();
        let indices = self
            .backups
            .table_view
            .sorted_indices(indices, self.table_interaction(TableKind::Backups));
        // Sorting changes row order, never the set of columns or their max
        // widths. Reuse the scan-time projection for an unfiltered table.
        let content_widths = if indices.len() == self.backups.table_view.rows.len() {
            self.backups.table_view.content_widths.clone()
        } else {
            let columns = table_column_schema(TableKind::Backups).expect("backup schema");
            let mut widths = columns
                .iter()
                .map(|column| display_width(column.heading))
                .collect::<Vec<_>>();
            for &index in &indices {
                for (width, value) in widths.iter_mut().zip(&self.backups.table_view.rows[index]) {
                    *width = (*width).max(display_width(value));
                }
            }
            widths
        };
        let snapshot = Rc::new(BackupViewSnapshot {
            indices,
            content_widths,
        });
        *self.backups.view_cache.snapshot.borrow_mut() = Some((key, Rc::clone(&snapshot)));
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_survives_animation_and_selection_but_invalidates_on_query_sort_and_refresh() {
        let mut state = crate::tui::demo::build_scene("backups").unwrap();
        let first = state.backup_view_snapshot();
        state.advance_animation();
        state.navigate(NavCommand::Down, 20);
        assert!(Rc::ptr_eq(&first, &state.backup_view_snapshot()));
        state.toggle_table_sort(TableKind::Backups);
        let sorted = state.backup_view_snapshot();
        assert!(!Rc::ptr_eq(&first, &sorted));
        state.navigate(NavCommand::Search, 20);
        state.push_input_char('x');
        let searched = state.backup_view_snapshot();
        assert!(!Rc::ptr_eq(&sorted, &searched));
        assert!(searched.indices.is_empty());
        let rows = state.backups().to_vec();
        state.replace_backups(rows);
        assert!(!Rc::ptr_eq(&searched, &state.backup_view_snapshot()));
    }

    #[test]
    fn device_filter_changes_snapshot_and_hidden_long_values_do_not_set_column_widths() {
        let mut state = crate::tui::demo::build_scene("backups").unwrap();
        let mut rows = state.backups().to_vec();
        rows[1].dept = Some("hidden".repeat(200));
        state.replace_backups(rows);
        state.backups.device_filter = BackupDeviceFilter::All;
        let all = state.backup_view_snapshot();
        state.backups.device_filter =
            BackupDeviceFilter::Confirmed(state.backups.group_keys[0].clone().unwrap());
        let filtered = state.backup_view_snapshot();
        assert!(!Rc::ptr_eq(&all, &filtered));
        assert_eq!(filtered.indices, vec![0]);
        assert!(filtered.content_widths[4] < all.content_widths[4]);
        let previous_tree = state.backup_device_tree_snapshot().to_vec();
        state.backup_device_tree_toggle();
        assert_eq!(state.backup_device_tree_snapshot().len(), 1);
        state.backup_device_tree_toggle();
        assert_eq!(state.backup_device_tree_snapshot(), previous_tree);
    }

    #[test]
    fn search_corpus_rebuilds_on_backup_refresh_and_summary_tracks_new_rows() {
        let mut state = crate::tui::demo::build_scene("backups").unwrap();
        state.backups.device_filter = BackupDeviceFilter::All;
        state.navigate(NavCommand::Search, 20);
        for ch in "unique-keyword-98".chars() {
            state.push_input_char(ch);
        }
        assert_eq!(state.visible_backup_count(), 0);
        let mut rows = state.backups().to_vec();
        rows[0].file_name = "unique-keyword-98.edpb".into();
        state.replace_backups(rows);
        assert_eq!(state.visible_backup_count(), 1);
        assert_eq!(state.backup_overview_counts().total, state.backups().len());
        state.replace_backups(Vec::new());
        assert_eq!(state.visible_backup_count(), 0);
        assert_eq!(state.backup_overview_counts().total, 0);
    }

    #[test]
    fn unfiltered_view_uses_source_column_widths_even_when_sorted() {
        let mut state = crate::tui::demo::build_scene("backups").unwrap();
        state.backups.device_filter = BackupDeviceFilter::All;
        let original = state.backup_view_snapshot();
        assert_eq!(
            original.content_widths,
            state.backups.table_view.content_widths
        );
        state
            .shell
            .horizontal_scroll
            .entry(TableKind::Backups)
            .or_default()
            .toggle_sort_for(3);
        let sorted = state.backup_view_snapshot();
        assert_eq!(
            sorted.content_widths,
            state.backups.table_view.content_widths
        );
        assert_eq!(sorted.indices.len(), state.backups.rows.len());
    }

    #[test]
    #[ignore = "manual desktop projection benchmark"]
    fn desktop_projection_benchmark() {
        use ratatui::{backend::TestBackend, Terminal};
        use std::time::Instant;
        let fixture = crate::tui::demo::build_scene("backups").unwrap().backups()[0].clone();
        for count in [100, 1_000, 10_000] {
            let rows = (0..count)
                .map(|index| {
                    let mut row = fixture.clone();
                    row.index = index;
                    row.path = format!("bench-{index}.edpb").into();
                    if let Some(identity) = row.identity.as_mut() {
                        identity.hardware.serial = Some(format!("BENCH-SERIAL-{index}"));
                        identity.hardware.serial_sha256 = None;
                    }
                    row
                })
                .collect();
            let mut state = AppState::new();
            let start = Instant::now();
            state.replace_backups(rows);
            state.navigate(NavCommand::WorkspaceBackups, 20);
            let build = start.elapsed();
            let snapshot = state.backup_view_snapshot();
            let start = Instant::now();
            for _ in 0..2_000 {
                assert!(Rc::ptr_eq(&snapshot, &state.backup_view_snapshot()));
            }
            let warm = start.elapsed();
            let mut terminal = Terminal::new(TestBackend::new(200, 60)).unwrap();
            let start = Instant::now();
            for _ in 0..20 {
                terminal
                    .draw(|frame| crate::tui::render::draw(frame, &state))
                    .unwrap();
            }
            let frame_ms = start.elapsed().as_secs_f64() * 1000.0 / 20.0;
            let start = Instant::now();
            state
                .shell
                .horizontal_scroll
                .entry(TableKind::Backups)
                .or_default()
                .toggle_sort_for(3);
            let sorted = state.backup_view_snapshot();
            let sort_ms = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(sorted.indices.len(), count);
            let start = Instant::now();
            state.backups.device_filter = BackupDeviceFilter::Unresolved;
            let filtered = state.backup_view_snapshot();
            let filter_ms = start.elapsed().as_secs_f64() * 1000.0;
            assert!(filtered.indices.len() <= count);
            state.backups.device_filter = BackupDeviceFilter::All;
            let start = Instant::now();
            state.navigate(NavCommand::Search, 20);
            for ch in "bench-01".chars() {
                state.push_input_char(ch);
            }
            let search_ms = start.elapsed().as_secs_f64() * 1000.0;
            println!(
                "count={count} groups={} build_ms={:.3} cached_snapshot_us={:.3} frame_ms={frame_ms:.3} sort_ms={sort_ms:.3} filter_ms={filter_ms:.3} search_ms={search_ms:.3}",
                state.backup_device_tree_snapshot().len(),
                build.as_secs_f64() * 1000.0,
                warm.as_secs_f64() * 1_000_000.0 / 2000.0,
            );
        }
    }
}
