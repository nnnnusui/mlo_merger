use std::collections::{BTreeMap, BTreeSet};

use crate::core::format::ymap::diff::reference_hash;

use super::types::{
  DerivedVanillaFile, ResourceInventory, SourceYmapRef, YmapLoadPlan, YmapRelationshipIndex,
};

pub(super) fn build(resources: &BTreeMap<String, ResourceInventory>) -> YmapLoadPlan {
  let changed_source_parents: BTreeSet<_> = resources
    .iter()
    .flat_map(|(resource_id, resource)| {
      resource.files_by_format.get(".ymap").into_iter().flatten().filter_map(|file| {
        file.vanilla.as_ref().is_none_or(|baseline| !baseline.content_matches).then(|| {
          SourceYmapRef {
            resource: resource_id.clone(),
            source_path: file.path.clone(),
            file_name: file.file_name.clone(),
          }
        })
      })
    })
    .collect();

  let changed_parent_hashes: BTreeSet<_> = changed_source_parents
    .iter()
    .map(|parent| {
      reference_hash(parent.file_name.strip_suffix(".ymap").unwrap_or(&parent.file_name))
    })
    .filter(|hash| *hash != 0)
    .collect();

  let additional_source_children = resources
    .iter()
    .flat_map(|(resource_id, resource)| {
      resource.files_by_format.get(".ymap").into_iter().flatten().filter_map(|file| {
        file.ymap_parent_hash.as_ref().and_then(|parent_hash| {
          u32::from_str_radix(parent_hash, 16)
            .ok()
            .filter(|hash| changed_parent_hashes.contains(hash))
            .map(|_| SourceYmapRef {
              resource: resource_id.clone(),
              source_path: file.path.clone(),
              file_name: file.file_name.clone(),
            })
        })
      })
    })
    .collect();

  YmapLoadPlan {
    changed_source_parents,
    additional_source_children,
  }
}

pub(super) fn vanilla_ymaps_to_read(
  changed_source_parents: &BTreeSet<SourceYmapRef>,
  relationships: &YmapRelationshipIndex,
  vanilla_files: &BTreeMap<String, DerivedVanillaFile>,
) -> BTreeSet<String> {
  let mut vanilla_ymaps_to_read = BTreeSet::new();
  for parent in changed_source_parents {
    let parent_name = parent.file_name.to_ascii_lowercase();
    if vanilla_files.contains_key(&parent_name) {
      vanilla_ymaps_to_read.insert(parent_name);
    }
    let parent_hash =
      reference_hash(parent.file_name.strip_suffix(".ymap").unwrap_or(&parent.file_name));
    if parent_hash == 0 {
      continue;
    }
    if let Some(children) = relationships.children_by_parent_hash.get(&format!("{parent_hash:08x}"))
    {
      vanilla_ymaps_to_read.extend(children.iter().map(|name| name.to_ascii_lowercase()));
    }
  }
  vanilla_ymaps_to_read
}
