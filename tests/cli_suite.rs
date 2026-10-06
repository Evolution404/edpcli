#[cfg(target_os = "macos")]
#[path = "support/edpb_manifest.rs"]
mod edpb_manifest;
#[path = "support/gold_name.rs"]
pub mod gold_name;

#[path = "common/mod.rs"]
pub mod common;

#[path = "cli_ux.rs"]
mod cli_ux;
#[path = "cli_v2_parser.rs"]
mod cli_v2_parser;
#[path = "cli_v2_surface_guard.rs"]
mod cli_v2_surface_guard;
#[path = "cli_write_safety.rs"]
mod cli_write_safety;
#[path = "command_spec.rs"]
mod command_spec;
#[path = "identify_list.rs"]
mod identify_list;
#[path = "post_restore_format_operation.rs"]
mod post_restore_format_operation;
#[path = "selectors.rs"]
mod selectors;
