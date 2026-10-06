//! EDP Backup Container v1.
//!
//! The container is self-contained: raw evidence, manifest and integrity
//! metadata live in one .edpb file. Legacy .bin files are not runtime input.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod codec;
mod identity;
mod limits;
pub(crate) use limits::MAX_CONTAINER_BYTES;
mod model;
mod read;
mod validate;
mod write;

pub use crate::sha256::{sha256_hex, sha256_reader_hex};
use codec::*;
pub use identity::{canonical_media_identity, manifest_identity_from_snapshot};
use identity::{inferred_manifest_identity, validate_manifest_identity};
pub use model::*;
use model::{CHUNK_MAGIC, FILE_MAGIC, FOOTER_MAGIC};
pub use read::{read_artifact, read_raw_protocol, verify_file, VerifiedBackupReader};
use validate::validate_manifest_graph;
pub use write::{
    write_core_backup, write_core_backup_with_identity, write_core_backup_with_notes,
    write_metadata_backup, write_metadata_backup_with_identity,
};
