use super::*;
use crate::core::vanilla::{
  CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod duplicate_reports;
mod incremental_ybn;
mod incremental_ymap;
mod path_validation;
