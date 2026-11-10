//! Which resources' files each stream file that resources add is merged from, and the additions
//! the merge leaves out of them as an earlier one makes them already (`added_stream_files.json`).
//!
//! Every resource's version of a stream file that resources add is merged, each compared with an
//! empty file of its format, and the additions an earlier one makes already are left out; YMAPs
//! are the only format merged so far (see [`crate::core::format::ymap::compare::added`]). They are merged from the resource whose name
//! sorts last, as the one loaded last, so its version wins where they conflict. A stream file
//! whose resource overwrite sets `ignore` is left out.

use super::Result;
use crate::common::json::read_json;
use crate::common::path::stream_key;
use crate::core::config::matching;
use crate::core::format::ymap::compare::{YmapComparisonDiff, YmapDiffFields};
use crate::core::overwrite::{OverwriteFingerprint, Overwrites};
use crate::core::source_cache::{MergeInputs, MergeSourceFile};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// How one stream file that resources add is merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedStreamFile {
  /// The stream files merged, in merge order: the one loaded last first.
  pub sources: Vec<AddedStreamFileSource>,
  /// The names of every resource shipping the YMAP, ignored ones included, in load order. When
  /// there are several, the merged YMAP is always written, so that it replaces theirs.
  pub shipped_by: Vec<String>,
  /// The additions of a source that an earlier source in merge order makes already, which the
  /// merge leaves out.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub left_out: Vec<LeftOutAddition>,
}

impl AddedStreamFile {
  /// Whether several resources ship it, so the merged YMAP is always written to replace theirs.
  pub fn replaces_others(&self) -> bool {
    self.shipped_by.len() > 1
  }

  /// The place in merge order of the stream file of the resource with ID `resource`.
  pub fn merge_position(
    &self,
    resource: &str,
  ) -> Option<usize> {
    self.sources.iter().position(|source| source.resource == resource)
  }
}

/// An addition the merge leaves out of a stream file that resources add, as an earlier source in merge
/// order makes it already.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LeftOutAddition {
  /// The item left out.
  #[serde(flatten)]
  pub item: AddedItem,
  /// The item of an earlier source it is the same as, which is kept.
  pub kept: AddedItem,
}

/// An item of one resource's file of a stream file that resources add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedItem {
  /// The resource's ID, its path relative to the source root.
  pub resource: String,
  /// The path below the resource's `stream` directory.
  pub stream_path: String,
  /// The array holding it, as diff documents name it, such as `entities` or
  /// `instanced_data.grass_instance_list`.
  pub field: String,
  /// Its index in that array of the stream file.
  pub index: usize,
}

/// One resource's file of a stream file that resources add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedStreamFileSource {
  /// The resource's ID, its path relative to the source root.
  pub resource: String,
  /// The path below the resource's `stream` directory.
  pub stream_path: String,
}

/// The files of `inputs` each added stream file is merged from; see the module docs.
pub(super) fn resolve(
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> BTreeMap<String, AddedStreamFile> {
  let mut shippers = BTreeMap::<String, Vec<&MergeSourceFile>>::new();
  for source in &inputs.ymap {
    let name = source.file_name.to_ascii_lowercase();
    if inputs.added_stream_files.contains(&name) {
      shippers.entry(name).or_default().push(source);
    }
  }
  let mut files = BTreeMap::new();
  for (name, mut sources) in shippers {
    // Resource names are unique, so the last by lowercase name is the last loaded.
    sources.sort_by_key(|source| source.resource_name().to_ascii_lowercase());
    let shipped_by = sources.iter().map(|source| source.resource_name()).collect();
    let merged = sources
      .iter()
      .rev()
      .filter(|source| {
        let ignored = overwrites.for_source(source).is_some_and(|overwrite| overwrite.ignore);
        if ignored {
          tracing::info!(
            "Added stream file {name} of {} is ignored by its overwrite",
            source.resource_name()
          );
        }
        !ignored
      })
      .map(|source| AddedStreamFileSource {
        resource: source.resource.clone(),
        stream_path: source.stream_path.replace('\\', "/"),
      })
      .collect::<Vec<_>>();
    if merged.is_empty() {
      continue;
    }
    files.insert(
      name,
      AddedStreamFile {
        sources: merged,
        shipped_by,
        left_out: Vec::new(),
      },
    );
  }
  files
}

/// Records in each of `files` merged from several sources the additions the merge leaves out,
/// thinning their source-cache diff documents, with their overwrites applied, as it does (see
/// [`crate::core::format::ymap::compare::added`]).
pub(super) fn record_left_out(
  files: &mut BTreeMap<String, AddedStreamFile>,
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> Result<()> {
  let by_key = inputs.ymap_by_key();
  let tolerances = matching::current();
  for (name, file) in files.iter_mut().filter(|(_, file)| file.sources.len() > 1) {
    let mut full = Vec::new();
    let mut kept = Vec::new();
    for entry in &file.sources {
      let source = by_key
        .get(&(
          crate::core::source_cache::resource_name(&entry.resource).to_ascii_lowercase(),
          stream_key(&entry.stream_path),
        ))
        .ok_or_else(|| {
          format!("Source {} of added stream file {name} is not cached", entry.resource)
        })?;
      let path = source.diff_path.as_ref().ok_or_else(|| {
        format!("Source {} of added stream file {name} has no diff document", entry.resource)
      })?;
      let document: YmapComparisonDiff = read_json(path)?;
      let mut diffs = document.diffs.clone();
      if let Some(overwrite) = overwrites.for_source(source) {
        overwrite.apply(
          &mut diffs,
          &OverwriteFingerprint::from(&document.target),
          &OverwriteFingerprint::from(&document.base),
          name,
        )?;
      }
      full.push(serde_json::to_value(&document.diffs)?);
      kept.push(serde_json::to_value(&diffs)?);
    }
    let mut thinned = kept
      .iter()
      .map(|diffs| serde_json::from_value::<YmapDiffFields>(diffs.clone()))
      .collect::<std::result::Result<Vec<_>, _>>()?;
    let left_out =
      crate::core::format::ymap::compare::added::thin_additions(&mut thinned, &tolerances).left_out;
    // An addition's place among the changes an overwrite leaves, to its index in the source:
    // a diff against the empty YMAP adds every item, in order.
    let item = |(source, position): (usize, usize), field: &str| -> Result<AddedItem> {
      let index = source_index(&full[source], &kept[source], field, position)
        .ok_or_else(|| format!("Cannot place addition {position} of {field} in {name}"))?;
      let entry = &file.sources[source];
      Ok(AddedItem {
        resource: entry.resource.clone(),
        stream_path: entry.stream_path.clone(),
        field: field.to_string(),
        index,
      })
    };
    file.left_out = left_out
      .iter()
      .map(|left_out| {
        Ok(LeftOutAddition {
          item: item(left_out.at, left_out.field)?,
          kept: item(left_out.kept, left_out.field)?,
        })
      })
      .collect::<Result<_>>()?;
  }
  Ok(())
}

/// The index in `full`'s list of changes to `field` of the change at `position` of `kept`'s,
/// which holds some of those changes in the same order.
fn source_index(
  full: &Value,
  kept: &Value,
  field: &str,
  position: usize,
) -> Option<usize> {
  let list = |diffs: &Value| {
    field.split('.').try_fold(diffs, |value, key| value.get(key)).and_then(Value::as_array).cloned()
  };
  let (full, kept) = (list(full)?, list(kept)?);
  let mut index = 0;
  for (at, change) in kept.iter().enumerate() {
    while full.get(index)? != change {
      index += 1;
    }
    if at == position {
      return Some(index);
    }
    index += 1;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn additions_an_overwrite_leaves_are_placed_by_their_index_in_the_source() {
    let added = |guid: u32| serde_json::json!({"Added": {"entity": {"guid": guid}}});
    let full = serde_json::json!({"instanced_data": {"grass_instance_list": [
      added(1), added(2), added(3), added(2)
    ]}});
    // The overwrite left out the first two.
    let kept = serde_json::json!({"instanced_data": {"grass_instance_list": [added(3), added(2)]}});
    let field = "instanced_data.grass_instance_list";
    assert_eq!(source_index(&full, &kept, field, 0), Some(2));
    assert_eq!(source_index(&full, &kept, field, 1), Some(3));
    assert_eq!(source_index(&full, &kept, field, 2), None);
    assert_eq!(source_index(&full, &kept, "entities", 0), None);
  }
}
