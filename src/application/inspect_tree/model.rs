use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectNodeKind {
    Device,
    Region,
    Extent,
    Sector,
    Structure,
    Group,
    Field,
    Partition,
    UnknownRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskRegionSemantic {
    Protocol,
    Reserved,
    PartitionTable,
    PlainPartition,
    Unallocated,
    Lce,
    Tail,
    TailMetadataMirror,
    TailRestoreNode,
    Partition { partition_type: u32 },
    MbrPartition { partition_type: u8 },
    Unknown,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectNodeRange {
    pub start_lba: u64,
    pub sector_count: u64,
    pub byte_range: Option<AbsoluteByteRange>,
}

impl InspectNodeRange {
    pub fn sectors(start_lba: u64, sector_count: u64) -> Self {
        Self {
            start_lba,
            sector_count,
            byte_range: None,
        }
    }

    pub fn from_bytes(range: AbsoluteByteRange) -> Self {
        let start_lba = range.start_lba();
        let sector_count = if range.end_exclusive == range.start {
            0
        } else {
            range.end_lba().saturating_sub(start_lba).saturating_add(1)
        };
        Self {
            start_lba,
            sector_count,
            byte_range: Some(range),
        }
    }

    pub fn end_lba_exclusive(self) -> u64 {
        self.start_lba.saturating_add(self.sector_count)
    }

    pub fn contains_lba(self, lba: u64) -> bool {
        lba >= self.start_lba && lba < self.end_lba_exclusive()
    }
}

/// Render a non-empty half-open LBA extent as a single closed UI range.
/// Invalid or empty extents have no visible closed-range representation.
pub fn format_lba_closed_range(start: u64, end_exclusive: u64) -> Option<String> {
    (end_exclusive > start).then(|| format!("[{start}..{}]", end_exclusive - 1))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectChildren {
    None,
    Materialized(Vec<InspectNode>),
    LazySectors { start_lba: u64, sector_count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectNode {
    pub id: String,
    pub label: String,
    pub kind: InspectNodeKind,
    pub range: InspectNodeRange,
    pub children: InspectChildren,
    pub decoder: Option<InspectDecoderKind>,
    pub status: SemanticStatus,
    pub region_semantic: Option<DiskRegionSemantic>,
}

impl InspectNode {
    pub fn materialize_sector_page(&self, offset: u64, limit: usize) -> Vec<InspectNode> {
        let InspectChildren::LazySectors {
            start_lba,
            sector_count,
        } = self.children
        else {
            return Vec::new();
        };
        if offset >= sector_count || limit == 0 {
            return Vec::new();
        }
        let count = (sector_count - offset).min(limit as u64);
        (0..count)
            .map(|index| {
                let lba = start_lba + offset + index;
                let mut node = sector_stub(lba, self.decoder, self.status);
                node.region_semantic = self.region_semantic;
                node
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectLazySectorLocation {
    pub node_path: Vec<String>,
    pub start_lba: u64,
    pub sector_count: u64,
}

pub(super) fn node_paths_match<F>(
    node: &InspectNode,
    path: &mut Vec<String>,
    predicate: &F,
    out: &mut Vec<Vec<String>>,
) where
    F: Fn(&InspectNode) -> bool,
{
    path.push(node.id.clone());
    if predicate(node) {
        out.push(path.clone());
    }
    if let InspectChildren::Materialized(children) = &node.children {
        for child in children {
            node_paths_match(child, path, predicate, out);
        }
    }
    path.pop();
}

fn lazy_sector_location(
    node: &InspectNode,
    lba: u64,
    path: &mut Vec<String>,
) -> Option<InspectLazySectorLocation> {
    if !node.range.contains_lba(lba) {
        return None;
    }
    path.push(node.id.clone());
    match &node.children {
        InspectChildren::LazySectors {
            start_lba,
            sector_count,
        } if lba >= *start_lba && lba < start_lba.saturating_add(*sector_count) => {
            Some(InspectLazySectorLocation {
                node_path: path.clone(),
                start_lba: *start_lba,
                sector_count: *sector_count,
            })
        }
        InspectChildren::Materialized(children) => {
            for child in children {
                if let Some(found) = lazy_sector_location(child, lba, path) {
                    return Some(found);
                }
            }
            path.pop();
            None
        }
        _ => {
            path.pop();
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectTopology {
    pub root: InspectNode,
}

impl InspectTopology {
    pub fn regions_for_lba(&self, lba: u64) -> Vec<&InspectNode> {
        let InspectChildren::Materialized(children) = &self.root.children else {
            return Vec::new();
        };
        children
            .iter()
            .filter(|node| node.range.contains_lba(lba))
            .collect()
    }

    pub fn primary_region_for_lba(&self, lba: u64) -> Option<&InspectNode> {
        self.regions_for_lba(lba).into_iter().next()
    }

    pub fn lazy_sector_location(&self, lba: u64) -> Option<InspectLazySectorLocation> {
        lazy_sector_location(&self.root, lba, &mut Vec::new())
    }

    pub fn find_label_paths(&self, query: &str) -> Vec<Vec<String>> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        node_paths_match(
            &self.root,
            &mut Vec::new(),
            &|node| node.label.to_lowercase().contains(&query),
            &mut out,
        );
        out
    }
}
