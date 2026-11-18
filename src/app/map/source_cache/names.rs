//! Hash names spelled by the resources' file names.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use super::{Result, read_metadata};
use crate::common::json::{read_json, write_json};
use crate::core::{format::gamefile::meta::jenk_hash, game_archive::lod_name_candidates};

/// The source cache's hash-name table, in the layout of the vanilla `hash_names.json`.
pub(crate) const HASH_NAMES: &str = "hash_names.json";

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct HashNames {
  names: BTreeMap<String, String>,
}

/// Writes `hash_names.json` for the published inventory: every inventoried file's lowercase
/// name stem, and the LOD names derived from its `.ydr` and `.ydd` files. An unchanged file is
/// kept.
///
/// A resource's own YMAPs, archetypes and LOD models are missing from the vanilla names, so
/// without these its parents and entities read as `hash_XXXXXXXX`.
pub(super) fn write_hash_names(output: &Path) -> Result<()> {
  let metadata = read_metadata(output)?;
  let mut file_names = metadata
    .resources
    .values()
    .flat_map(|inventory| inventory.files_by_format.values().flatten())
    .map(|file| file.file_name.to_ascii_lowercase())
    .collect::<Vec<_>>();
  file_names.sort();
  file_names.dedup();
  let mut names = BTreeMap::new();
  let mut add = |name: String| {
    if !name.trim().is_empty() {
      names.entry(format!("{:08X}", jenk_hash(&name))).or_insert(name);
    }
  };
  for file_name in &file_names {
    let stem = file_name.rsplit_once('.').map_or(file_name.as_str(), |(stem, _)| stem);
    add(stem.to_owned());
  }
  for file_name in &file_names {
    lod_name_candidates(file_name).into_iter().for_each(&mut add);
  }
  let names = HashNames {
    names,
  };
  let path = output.join(HASH_NAMES);
  if read_json::<HashNames>(&path).is_ok_and(|previous| previous == names) {
    return Ok(());
  }
  write_json(&path, &names)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::map::source_cache::write_test_inventory;

  #[test]
  fn file_names_and_their_lod_names_are_written_once() {
    let dir = std::env::temp_dir().join(format!("mlo_source_names_{}", std::process::id()));
    let _ = crate::platform::fs::remove_dir_all(&dir);
    crate::platform::fs::create_dir_all(&dir).unwrap();
    write_test_inventory(
      &dir,
      &[
        "resources/a/Custom_Ext_Lod.ymap",
        "resources/a/custom_prop.ydr",
        "resources/b/custom_ext_slod1_children.ydd",
      ],
    );
    write_hash_names(&dir).unwrap();
    let path = dir.join(HASH_NAMES);
    let names = read_json::<HashNames>(&path).unwrap().names;
    let name = |value: &str| names.get(&format!("{:08X}", jenk_hash(value))).map(String::as_str);
    assert_eq!(name("custom_ext_lod"), Some("custom_ext_lod"));
    assert_eq!(name("custom_prop_lod"), Some("custom_prop_lod"));
    assert_eq!(name("custom_ext_slod1"), Some("custom_ext_slod1"));

    let modified = crate::platform::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_hash_names(&dir).unwrap();
    assert_eq!(crate::platform::fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let _ = crate::platform::fs::remove_dir_all(&dir);
  }
}
