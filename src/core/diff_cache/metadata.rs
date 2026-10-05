//! Loads GTAV manifests and validates ordered vanilla content histories.

use super::{Result, types::Variant};
use crate::core::vanilla_cache::VanillaCacheManifest;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn history(manifest: &VanillaCacheManifest) -> Result<BTreeMap<String, Vec<Variant>>> {
  if manifest.format_version != 1 || manifest.versions.is_empty() {
    return Err("A nonempty GTAV cache with schema 1 is required".into());
  }
  let mut files: BTreeMap<String, Vec<Variant>> = BTreeMap::new();
  let mut ids = BTreeSet::new();
  for (index, version) in manifest.versions.iter().enumerate() {
    let expected_parent = index.checked_sub(1).map(|index| &manifest.versions[index].id);
    if version.parent.as_ref() != expected_parent || !ids.insert(&version.id) {
      return Err(format!("Invalid GTAV version chain at {}", version.id).into());
    }
    for (name, change) in &version.changes {
      if name.contains(['/', '\\']) {
        return Err(format!("Invalid vanilla filename: {name}").into());
      }
      let variants = files.entry(name.to_ascii_lowercase()).or_default();
      if variants.last().map(|variant| &variant.file.sha256) != change.previous_sha256.as_ref() {
        return Err(format!("Invalid vanilla content chain for {name} at {}", version.id).into());
      }
      variants.push(Variant {
        index,
        version: version.id.clone(),
        file: change.file.clone(),
      });
    }
  }
  Ok(files)
}

pub(super) use crate::core::vanilla_cache::load_manifest;
