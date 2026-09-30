#[path = "disk_region_list/render.rs"]
mod render;
#[path = "disk_region_list/state.rs"]
mod state;

pub(crate) use render::{disk_region_list_lines, render_disk_region_list};
pub(crate) use state::{DiskRegionListMode, DiskRegionListState};

#[cfg(test)]
#[path = "disk_region_list/tests.rs"]
mod tests;
