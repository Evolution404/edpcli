//! 全盘 Inspect 的只读拓扑模型。
//!
//! 本模块只表达磁盘结构与 lazy children，不保存 TUI 展开/焦点等界面状态。
//! 大范围 sector 通过 `LazySectors` 延迟 materialize，避免按磁盘容量分配节点。

use super::inspect::{AbsoluteByteRange, InspectDecoderKind, InspectField, InspectFieldKey};
use crate::edpb::SemanticStatus;
use crate::inspect_target::InspectDiskContext;

mod build;
mod model;
mod search;
mod topology;

use build::sector_stub;
pub use build::{field_node, sector_node_with_fields};
use model::node_paths_match;
pub use model::{
    format_lba_closed_range, DiskRegionSemantic, InspectChildren, InspectLazySectorLocation,
    InspectNode, InspectNodeKind, InspectNodeRange, InspectTopology,
};
pub use search::{find_sector_structured_path, find_sector_structured_paths};
pub use topology::build_inspect_topology;
#[cfg(test)]
mod tests;
