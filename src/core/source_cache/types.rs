use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct FileFingerprint {
  pub(super) sha256: String,
  pub(super) modified_seconds: u64,
  pub(super) modified_nanos: u32,
  pub(super) size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct VanillaMatch {
  pub(super) version: String,
  pub(super) sha256: String,
  pub(super) content_matches: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct SourceFile {
  pub(super) path: String,
  pub(super) file_name: String,
  pub(super) sha256: String,
  pub(super) size: u64,
  pub(super) modified_seconds: u64,
  pub(super) modified_nanos: u32,
  pub(super) vanilla: Option<VanillaMatch>,
  pub(super) ymap_parent_hash: Option<String>,
  pub(super) metadata_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct ResourceInventory {
  pub(super) source: PathBuf,
  pub(super) files_by_format: BTreeMap<String, Vec<SourceFile>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SourceYmapRef {
  pub(super) resource: String,
  pub(super) source_path: String,
  pub(super) file_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct YmapLoadPlan {
  pub(super) changed_source_parents: BTreeSet<SourceYmapRef>,
  pub(super) additional_source_children: BTreeSet<SourceYmapRef>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct SourceCacheMetadata {
  pub(super) format_version: u32,
  pub(super) source_dir: PathBuf,
  pub(super) vanilla_dir: PathBuf,
  pub(super) vanilla_cache_dir: PathBuf,
  pub(super) vanilla_manifest_sha256: String,
  pub(super) vanilla_cache_revision: String,
  pub(super) latest_vanilla_version: String,
  pub(super) generated_at: String,
  pub(super) source_inputs: BTreeMap<String, FileFingerprint>,
  pub(super) resources: BTreeMap<String, ResourceInventory>,
  pub(super) scanned_stream_file_count: usize,
  pub(super) conflict_count: usize,
  pub(super) ymap_load_plan: YmapLoadPlan,
  pub(super) outputs: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DerivedVanillaCache {
  pub(super) format_version: u32,
  pub(super) vanilla_manifest_sha256: String,
  pub(super) latest_version: String,
  pub(super) files: BTreeMap<String, DerivedVanillaFile>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DerivedVanillaFile {
  pub(super) version: String,
  pub(super) sha256: String,
  pub(super) object: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct YmapRelationshipIndex {
  pub(super) format_version: u32,
  pub(super) vanilla_manifest_sha256: String,
  pub(super) version: String,
  pub(super) children_by_parent_hash: BTreeMap<String, Vec<String>>,
}
