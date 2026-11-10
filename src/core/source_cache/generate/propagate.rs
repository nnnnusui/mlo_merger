//! Carries each resource's changes of vanilla items over to the same items of the newer
//! generations of their YMAPs, as `propagated/{resource}/{file name}.diff.json`.
//!
//! The vanilla cache pairs the items of each generation of a YMAP family with those of the newer
//! ones (see [`generations`](crate::core::vanilla_cache::generations)). For every removal or
//! modification a resource's diff makes to a vanilla item, the same item of each newer
//! generation is removed or modified too, in a [`YmapPropagatedDiff`] of that YMAP whose changes
//! name the item they were carried over from in their `reason`. The lights of a LOD light pair
//! are carried as one: a light removed from either YMAP of a pair is removed from both YMAPs of
//! the newer pairs.
//!
//! Only removals and modifications are carried; additions and replacements have no counterpart
//! in other generations, and neither do entity parent changes, which are left out of the
//! modifications carried. Changes are not carried to YMAPs the resource ships itself, whose own
//! diffs say what it does to them. When several changes reach one item of a YMAP, a removal
//! wins, and otherwise the first is kept.
//!
//! Every document is derived from the diff documents and the vanilla cache's generation index,
//! so `propagated/inputs.json` records a digest of both, and the documents are kept while it
//! matches. It also lists the documents for merges.

use crate::platform::fs;
use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  path::Path,
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::Result;
use crate::{
  common::{json::write_json, path::ensure_safe_path},
  core::{
    format::ymap::compare::{
      YmapComparisonDiff, YmapComparisonFile, YmapPropagatedDiff, YmapPropagationOrigin,
      lod_light_pairs::LightSide,
    },
    source_cache::{read_metadata, stream_file_diff_path},
    vanilla_cache::generations::{self, GenerationIndex, GenerationUnit, ItemKind},
  },
};

/// The directory of the documents in the source cache.
pub(in crate::core::source_cache) const PROPAGATED_DIR: &str = "propagated";
/// File name of the record of what the documents were derived from, in [`PROPAGATED_DIR`].
const INPUTS_FILE: &str = "inputs.json";
/// Bump when [`PropagationRecord`] or how documents are derived changes.
const RECORD_VERSION: u32 = 1;

/// What the documents in [`PROPAGATED_DIR`] were derived from, and the documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::core::source_cache) struct PropagationRecord {
  record_version: u32,
  /// A digest of the diff record, the generation index and the layouts of both documents.
  digest: String,
  pub(in crate::core::source_cache) documents: Vec<PropagatedDocument>,
}

/// One document of a [`PropagationRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::core::source_cache) struct PropagatedDocument {
  /// The resource ID.
  pub(in crate::core::source_cache) resource: String,
  /// The vanilla YMAP the document changes, in lowercase.
  pub(in crate::core::source_cache) file_name: String,
  /// The document, relative to the source cache.
  pub(in crate::core::source_cache) path: String,
}

/// Reads the record of the source cache at `cache_dir`; `None` when it has none it can read.
pub(in crate::core::source_cache) fn read_record(cache_dir: &Path) -> Option<PropagationRecord> {
  crate::common::json::read_json::<PropagationRecord>(
    cache_dir.join(PROPAGATED_DIR).join(INPUTS_FILE),
  )
  .ok()
  .filter(|record| record.record_version == RECORD_VERSION)
}

/// How many documents [`write_propagated_diffs`] wrote, and whether it kept the previous ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core::source_cache) struct PropagationCounts {
  pub documents: usize,
  pub reused: bool,
}

/// Rebuilds [`PROPAGATED_DIR`] from the diff documents in `output` (the source cache) and the
/// generation index of the vanilla cache at `vanilla_cache_dir`, unless its record shows it
/// current; see the module docs.
pub(in crate::core::source_cache) fn write_propagated_diffs(
  output: &Path,
  vanilla_cache_dir: &Path,
) -> Result<PropagationCounts> {
  let target = output.join(PROPAGATED_DIR);
  let staging = output.join(format!("{PROPAGATED_DIR}.tmp"));
  ensure_safe_path(output, &target)?;
  ensure_safe_path(output, &staging)?;
  let index = GenerationIndex::load(vanilla_cache_dir)?;
  let digest = hex::encode(Sha256::digest(serde_json::to_vec(&(
    RECORD_VERSION,
    YmapPropagatedDiff::SCHEMA_VERSION,
    generations::ALGORITHM_VERSION,
    hex::encode(Sha256::digest(fs::read(output.join("diff").join("inputs.json"))?)),
    generations::index_digest(vanilla_cache_dir)?,
  ))?));
  if let Some(record) = read_record(output)
    && record.digest == digest
    && record.documents.iter().all(|document| fs::is_file(output.join(&document.path)))
  {
    return Ok(PropagationCounts {
      documents: record.documents.len(),
      reused: true,
    });
  }

  remove_dir_if_exists(&staging)?;
  fs::create_dir_all(&staging)?;
  let mut documents = Vec::new();
  if let Some(index) = &index {
    let propagation = Propagation {
      output,
      vanilla_cache_dir,
      index,
      units: HashMap::new(),
      unpaired: 0,
    };
    documents = propagation.write(&staging)?;
  }
  let record = PropagationRecord {
    record_version: RECORD_VERSION,
    digest,
    documents,
  };
  write_json(&staging.join(INPUTS_FILE), &record)?;
  remove_dir_if_exists(&target)?;
  fs::rename(&staging, &target)?;
  if index.is_none() {
    tracing::warn!(
      "The vanilla cache has no YMAP generation index; changes are not carried over to newer generations"
    );
  }
  tracing::info!(
    "Wrote {} YMAP diffs carried over to newer generations in {}",
    record.documents.len(),
    target.display()
  );
  Ok(PropagationCounts {
    documents: record.documents.len(),
    reused: false,
  })
}

/// The arrays of a YMAP diff whose item changes are carried, by their path in its JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Array {
  Entities,
  LodLights,
  DistantLodLights,
  CarGenerators,
  BoxOccluders,
  OccludeModels,
  TimeCycleModifiers,
}

impl Array {
  const ALL: [Self; 7] = [
    Self::Entities,
    Self::LodLights,
    Self::DistantLodLights,
    Self::CarGenerators,
    Self::BoxOccluders,
    Self::OccludeModels,
    Self::TimeCycleModifiers,
  ];

  fn path(self) -> &'static [&'static str] {
    match self {
      Self::Entities => &["entities"],
      Self::LodLights => &["lod_lights"],
      Self::DistantLodLights => &["distant_lod_lights", "items"],
      Self::CarGenerators => &["car_generators"],
      Self::BoxOccluders => &["box_occluders"],
      Self::OccludeModels => &["occlude_models"],
      Self::TimeCycleModifiers => &["time_cycle_modifiers"],
    }
  }

  /// The array as `reason`s name it.
  fn label(self) -> String {
    self.path().join(".")
  }

  fn kind(self) -> ItemKind {
    match self {
      Self::Entities => ItemKind::Entities,
      Self::LodLights | Self::DistantLodLights => ItemKind::Lights,
      Self::CarGenerators => ItemKind::CarGenerators,
      Self::BoxOccluders => ItemKind::BoxOccluders,
      Self::OccludeModels => ItemKind::OccludeModels,
      Self::TimeCycleModifiers => ItemKind::TimeCycleModifiers,
    }
  }
}

/// A change carried to one item.
#[derive(Debug, Clone)]
enum Carried {
  Removed {
    reason: String,
  },
  /// The changes as the diff JSON holds them.
  Modified {
    changes: Value,
    reason: String,
  },
}

/// The changes carried to one vanilla YMAP for one resource, by array and item index.
#[derive(Default)]
struct Target {
  items: BTreeMap<Array, BTreeMap<usize, Carried>>,
  origins: Vec<YmapPropagationOrigin>,
}

impl Target {
  fn carry(
    &mut self,
    array: Array,
    index: usize,
    change: Carried,
  ) {
    let items = self.items.entry(array).or_default();
    let replaces = match items.get(&index) {
      None => true,
      Some(Carried::Modified {
        ..
      }) => matches!(change, Carried::Removed { .. }),
      Some(Carried::Removed {
        ..
      }) => false,
    };
    if replaces {
      items.insert(index, change);
    }
  }

  /// The diff JSON of the carried changes.
  fn diffs(&self) -> Value {
    let mut root = Map::new();
    for (array, items) in &self.items {
      let diffs = items
        .iter()
        .map(|(&index, change)| match change {
          Carried::Removed {
            reason,
          } => serde_json::json!({ "Removed": { "index": index, "reason": reason } }),
          Carried::Modified {
            changes,
            reason,
          } => serde_json::json!({ "Modified": {
            "before_index": index, "after_index": index, "changes": changes, "reason": reason,
          }}),
        })
        .collect::<Vec<_>>();
      let path = array.path();
      let mut parent = &mut root;
      for key in &path[..path.len() - 1] {
        parent = parent
          .entry(*key)
          .or_insert_with(|| Value::Object(Map::new()))
          .as_object_mut()
          .expect("diff arrays are nested in objects");
      }
      parent.insert(path[path.len() - 1].to_owned(), Value::Array(diffs));
    }
    Value::Object(root)
  }
}

/// A change a resource's diff makes to a vanilla item.
struct Change {
  array: Array,
  index: usize,
  /// The field changes of a modification; `None` for a removal.
  changes: Option<Value>,
}

struct Propagation<'a> {
  output: &'a Path,
  vanilla_cache_dir: &'a Path,
  index: &'a GenerationIndex,
  units: HashMap<String, GenerationUnit>,
  /// How many changes found no counterpart in a newer generation.
  unpaired: usize,
}

impl Propagation<'_> {
  /// Writes the documents into `staging` and lists them.
  fn write(
    mut self,
    staging: &Path,
  ) -> Result<Vec<PropagatedDocument>> {
    let metadata = read_metadata(self.output)?;
    let mut documents = Vec::new();
    let mut vanilla_files = HashMap::<String, YmapComparisonFile>::new();
    for (resource_id, resource) in &metadata.resources {
      let resource_name =
        resource.source.file_name().ok_or("Resource has no name")?.to_string_lossy().into_owned();
      let ymaps = resource.files_by_format.get(".ymap").map(Vec::as_slice).unwrap_or_default();
      let shipped =
        ymaps.iter().map(|file| file.file_name.to_ascii_lowercase()).collect::<BTreeSet<_>>();
      let mut targets = BTreeMap::<String, Target>::new();
      for file in ymaps {
        if file.vanilla.as_ref().is_none_or(|vanilla| vanilla.content_matches) {
          continue;
        }
        let diff_path = self
          .output
          .join(crate::core::source_cache::DIFF_DIR)
          .join(stream_file_diff_path(&resource.source, &file.path)?);
        let Ok(document) = crate::common::json::read_json::<YmapComparisonDiff>(&diff_path) else {
          continue;
        };
        let origin = YmapPropagationOrigin {
          source: document.source.clone(),
          vanilla: document.vanilla.clone(),
        };
        let reached =
          self.carry(&resource_name, &file.file_name, &document, &shipped, &mut targets)?;
        for name in reached {
          let target = targets.get_mut(&name).ok_or("Carried change has no target")?;
          if !target.origins.contains(&origin) {
            target.origins.push(origin.clone());
          }
        }
      }
      for (file_name, target) in targets {
        if target.items.is_empty() {
          continue;
        }
        let vanilla = match vanilla_files.get(&file_name) {
          Some(vanilla) => vanilla.clone(),
          None => {
            let vanilla = self.vanilla_file(&file_name)?;
            vanilla_files.insert(file_name.clone(), vanilla.clone());
            vanilla
          }
        };
        let document = YmapPropagatedDiff {
          schema_version: YmapPropagatedDiff::SCHEMA_VERSION,
          resource: resource_id.clone(),
          vanilla,
          origins: target.origins.clone(),
          diffs: serde_json::from_value(target.diffs())
            .map_err(|error| format!("Carried diff of {file_name} for {resource_name}: {error}"))?,
        };
        let relative = Path::new(&resource_name).join(format!("{file_name}.diff.json"));
        let path = staging.join(&relative);
        ensure_safe_path(staging, &path)?;
        fs::create_dir_all(path.parent().ok_or("Carried diff path has no parent")?)?;
        write_json(&path, &document)?;
        documents.push(PropagatedDocument {
          resource: resource_id.clone(),
          file_name,
          path: Path::new(PROPAGATED_DIR).join(relative).to_string_lossy().replace('\\', "/"),
        });
      }
    }
    if self.unpaired > 0 {
      tracing::info!(
        "{} changes of vanilla items have no counterpart in a newer generation and are not carried over",
        self.unpaired
      );
    }
    Ok(documents)
  }

  /// Adds the changes `document` (of the resource's YMAP `file_name`) makes to vanilla items to
  /// `targets`, for each newer generation's YMAP the resource does not ship; returns the YMAPs
  /// it carried changes to.
  fn carry(
    &mut self,
    resource_name: &str,
    file_name: &str,
    document: &YmapComparisonDiff,
    shipped: &BTreeSet<String>,
    targets: &mut BTreeMap<String, Target>,
  ) -> Result<BTreeSet<String>> {
    let mut reached = BTreeSet::new();
    let index = self.index;
    let Some(located) = index.locate(file_name) else {
      return Ok(reached);
    };
    let changes = changes(&serde_json::to_value(&document.diffs)?);
    if changes.is_empty() {
      return Ok(reached);
    }
    let file_label = file_name.to_ascii_lowercase();
    for unit_ref in located.family.units.iter().filter(|unit| unit.older == located.generation) {
      let unit = self.unit(&unit_ref.key)?;
      // This YMAP's paired items of each kind, by index.
      let mut pairs = HashMap::<ItemKind, HashMap<u32, (u32, u32)>>::new();
      for change in &changes {
        pairs.entry(change.array.kind()).or_insert_with(|| {
          unit
            .pairs
            .get(&change.array.kind())
            .into_iter()
            .flatten()
            .filter(|[older_file, ..]| *older_file as usize == located.file)
            .map(|&[_, older_index, newer_file, newer_index]| {
              (older_index, (newer_file, newer_index))
            })
            .collect()
        });
      }
      for change in &changes {
        let Some(&(newer_file, newer_index)) = u32::try_from(change.index)
          .ok()
          .and_then(|index| pairs[&change.array.kind()].get(&index))
        else {
          tracing::debug!(
            "{file_label} {}[{}] of {resource_name} has no counterpart in the {} generation",
            change.array.label(),
            change.index,
            located.family.generations[unit_ref.newer].prefix
          );
          self.unpaired += 1;
          continue;
        };
        let newer = &unit.newer[newer_file as usize];
        let reason = format!(
          "propagated from {resource_name}: {file_label} {}[{}]",
          change.array.label(),
          change.index
        );
        // The YMAPs the change goes to, with the array it changes there.
        let mut destinations = Vec::new();
        match change.array {
          Array::LodLights | Array::DistantLodLights => {
            let lod =
              LightSide::partner_name(&newer.name).filter(|_| newer.partner_sha256.is_some());
            match &change.changes {
              None => {
                destinations.push((newer.name.clone(), Array::DistantLodLights));
                destinations.extend(lod.map(|lod| (lod, Array::LodLights)));
              }
              Some(_) if change.array == Array::DistantLodLights => {
                destinations.push((newer.name.clone(), Array::DistantLodLights));
              }
              Some(_) => destinations.extend(lod.map(|lod| (lod, Array::LodLights))),
            }
          }
          array => destinations.push((newer.name.clone(), array)),
        }
        for (destination, array) in destinations {
          if shipped.contains(&destination) {
            continue;
          }
          let change = match &change.changes {
            None => Carried::Removed {
              reason: reason.clone(),
            },
            Some(changes) => Carried::Modified {
              changes: changes.clone(),
              reason: reason.clone(),
            },
          };
          targets.entry(destination.clone()).or_default().carry(
            array,
            newer_index as usize,
            change,
          );
          reached.insert(destination);
        }
      }
    }
    Ok(reached)
  }

  fn unit(
    &mut self,
    key: &str,
  ) -> Result<GenerationUnit> {
    if let Some(unit) = self.units.get(key) {
      return Ok(unit.clone());
    }
    let unit = generations::load_unit(self.vanilla_cache_dir, key)?;
    self.units.insert(key.to_owned(), unit.clone());
    Ok(unit)
  }

  /// The latest vanilla YMAP `file_name` as documents record it.
  fn vanilla_file(
    &self,
    file_name: &str,
  ) -> Result<YmapComparisonFile> {
    let stored = format!("latest/ymap/{file_name}");
    let path = fs::canonicalize(self.vanilla_cache_dir.join(&stored))?;
    let fingerprint = crate::common::fingerprint::Fingerprint::read(&path, None, false)?;
    Ok(YmapComparisonFile {
      path: stored,
      sha256: fingerprint.sha256,
      size: fingerprint.size,
      modified_seconds: fingerprint.modified_seconds,
      modified_nanos: fingerprint.modified_nanos,
    })
  }
}

/// The removals and modifications of vanilla items in the diff JSON `diffs`, in array order;
/// entity parent changes are left out of modifications, and modifications left with no change
/// are dropped, as are those changing items of a nested array, such as the triangles of an
/// occlude model, whose indices need not hold in another generation's item.
fn changes(diffs: &Value) -> Vec<Change> {
  let mut changes = Vec::new();
  for array in Array::ALL {
    let items = array.path().iter().try_fold(diffs, |value, key| value.get(*key));
    for item in items.and_then(Value::as_array).into_iter().flatten() {
      if let Some(index) = item.pointer("/Removed/index").and_then(Value::as_u64) {
        changes.push(Change {
          array,
          index: index as usize,
          changes: None,
        });
      } else if let Some(modified) = item.get("Modified")
        && let Some(index) = modified.get("before_index").and_then(Value::as_u64)
        && let Some(fields) = modified.get("changes").and_then(Value::as_object)
        && !fields.is_empty()
        && !fields.values().any(Value::is_array)
      {
        changes.push(Change {
          array,
          index: index as usize,
          changes: Some(Value::Object(fields.clone())),
        });
      }
    }
  }
  changes
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
  match fs::remove_dir_all(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Carries the diffs of the local source cache over, against the vanilla cache named by
  /// `MLO_VANILLA_CACHE` (default `asset/vanilla-cache`), which needs a generation index.
  #[test]
  #[ignore = "requires local asset/source-cache and a vanilla cache with generations"]
  fn carries_local_source_cache_diffs_to_newer_generations() {
    let asset = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset");
    let vanilla_cache = std::env::var_os("MLO_VANILLA_CACHE")
      .map_or_else(|| asset.join("vanilla-cache"), std::path::PathBuf::from);
    let output = std::env::temp_dir().join(format!("propagated_{}", std::process::id()));
    let _ = fs::remove_dir_all(&output);
    let source_cache = asset.join("source-cache");
    for entry in crate::platform::fs::WalkDir::new(source_cache.join("diff")) {
      let entry = entry.unwrap();
      let destination = output.join(entry.path().strip_prefix(&source_cache).unwrap());
      if entry.file_type().is_dir() {
        fs::create_dir_all(&destination).unwrap();
      } else {
        fs::copy(entry.path(), &destination).unwrap();
      }
    }
    fs::copy(source_cache.join("cache_info.json"), output.join("cache_info.json")).unwrap();

    let counts = write_propagated_diffs(&output, &vanilla_cache).unwrap();
    println!("{} documents -> {}", counts.documents, output.join(PROPAGATED_DIR).display());
    assert!(!counts.reused);
    let record = read_record(&output).unwrap();
    for document in &record.documents {
      let path = output.join(&document.path);
      let vanilla = vanilla_cache.join("latest/ymap").join(&document.file_name);
      assert!(YmapPropagatedDiff::read_current(&path, &vanilla).unwrap().is_some());
    }
    let rerun = write_propagated_diffs(&output, &vanilla_cache).unwrap();
    assert_eq!(
      rerun,
      PropagationCounts {
        documents: counts.documents,
        reused: true,
      }
    );
  }

  #[test]
  fn changes_read_removals_and_field_changes_of_every_carried_array() {
    let diffs = serde_json::json!({
      "entities": [
        { "Removed": { "index": 3 } },
        { "Modified": { "before_index": 1, "after_index": 0, "changes": {}, "parent": "None" } },
        { "Modified": { "before_index": 2, "after_index": 1, "changes": { "flags": 8 } } },
        { "Added": { "after": 0, "entity": {}, "parent": "None" } },
      ],
      "distant_lod_lights": { "items": [{ "Removed": { "index": 7, "implied": true } }] },
      "car_generators": [{ "Replaced": { "index": 0, "item": {} } }],
      "occlude_models": [
        { "Modified": { "before_index": 0, "after_index": 0, "changes": {
          "bmin": { "x": 1.0 }, "triangles": [{ "Removed": { "index": 2 } }],
        } } },
      ],
    });
    let changes = changes(&diffs)
      .into_iter()
      .map(|change| (change.array, change.index, change.changes))
      .collect::<Vec<_>>();
    assert_eq!(
      changes,
      [
        (Array::Entities, 3, None),
        (Array::Entities, 2, Some(serde_json::json!({ "flags": 8 }))),
        (Array::DistantLodLights, 7, None),
      ]
    );
  }

  #[test]
  fn a_removal_wins_over_a_modification_of_the_same_item() {
    let mut target = Target::default();
    let modified = |reason: &str| Carried::Modified {
      changes: serde_json::json!({ "flags": 1 }),
      reason: reason.into(),
    };
    target.carry(Array::Entities, 4, modified("first"));
    target.carry(Array::Entities, 4, modified("second"));
    target.carry(Array::Entities, 5, modified("kept"));
    target.carry(
      Array::Entities,
      4,
      Carried::Removed {
        reason: "removed".into(),
      },
    );
    target.carry(
      Array::Entities,
      5,
      Carried::Removed {
        reason: "removed".into(),
      },
    );
    target.carry(
      Array::LodLights,
      2,
      Carried::Removed {
        reason: "light".into(),
      },
    );
    let diffs: crate::core::format::ymap::compare::YmapDiffFields =
      serde_json::from_value(target.diffs()).unwrap();
    let json = serde_json::to_value(&diffs).unwrap();
    assert_eq!(
      json["entities"],
      serde_json::json!([
        { "Removed": { "index": 4, "reason": "removed" } },
        { "Removed": { "index": 5, "reason": "removed" } },
      ])
    );
    assert_eq!(
      json["lod_lights"],
      serde_json::json!([{ "Removed": { "index": 2, "reason": "light" } }])
    );
  }
}
