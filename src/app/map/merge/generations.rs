//! Resolving generations when merging: carrying each resource's changes of vanilla items over
//! to the items standing for them in newer generations of their YMAPs that the game loads.
//!
//! The vanilla cache pairs the items of each generation of a YMAP family with those of the newer
//! ones, records which YMAPs the game loads, and names the successor of each YMAP it does not
//! load in place of which a newer generation is loaded (see
//! [`generations`](crate::app::map::vanilla_cache::generations)). Merges deploy no YMAP with a
//! successor. Every removal, modification or replacement a resource's YMAP makes to a vanilla
//! item, after its LOD light pair document's `skip` and its overwrite, is carried to each item it
//! pairs with in a newer generation's loaded YMAP, YMAPs the resource ships included, as are the
//! removals its LOD light pair document implies for a vanilla YMAP it does not ship. The carried
//! changes are written to `generations/{resource}/{version}/{file name}.diff.json` of the merge
//! output ([`YmapPropagatedDiff`]), `version` naming the stage that first ships the generation
//! they come from, each naming the item it came from in its `reason`, and merged as one more
//! source of that YMAP, newest origin first. The lights of a LOD light pair are carried as one: a
//! light removed from either YMAP of a pair is removed from both YMAPs of the newer pairs.
//! Entities a resource adds with no parent to a YMAP with a successor are added to the end of the
//! successor, with the physics dictionaries the resource adds there, and LOD lights to the end of
//! the same YMAP of the successor pair, in order, so that both YMAPs keep their lights at the same
//! indices.
//!
//! Entity parent changes are left out of the modifications carried, as are changes of items of a
//! nested array. When several changes of one generation reach one item, a removal wins, and
//! otherwise the first is kept. The changes of a YMAP with a successor that reach no loaded item,
//! and its other additions, are listed in `generations/left_behind.json` as left behind.

use crate::platform::fs;
use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::operation::Result;
use crate::{
  app::map::{
    overwrite::{OverwriteFingerprint, Overwrites},
    source_cache::{MergeSourceFile, YmapImplied, resource_name},
    vanilla_cache::generations::{Counterparts, GenerationIndex, ItemKind},
  },
  common::{json::write_json, path::ensure_safe_path},
  core::format::ymap::compare::{
    ItemDiff, YmapComparisonDiff, YmapComparisonFile, YmapDiffFields, YmapPropagatedDiff,
    YmapPropagationOrigin, lod_light_pairs::LightSide,
  },
};

/// The directory of the carried documents in the merge output.
pub(super) const GENERATIONS_DIR: &str = "generations";
/// File name of the changes left behind, in [`GENERATIONS_DIR`].
const LEFT_BEHIND_FILE: &str = "left_behind.json";

/// A change of a YMAP merges do not deploy that is carried nowhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LeftBehind {
  /// The resource ID.
  pub resource: String,
  /// The resource's stream path of the YMAP making the change.
  pub stream_path: String,
  /// The vanilla YMAP the change is made to, in lowercase.
  pub file_name: String,
  /// The array, as its path in the diff JSON, such as `entities` or `distant_lod_lights.items`.
  pub array: String,
  /// The changed item's `base_index`, or an addition's `target_index`.
  pub index: Option<usize>,
  /// `Removed`, `Modified` or `Added`.
  pub change: String,
  pub reason: LeftBehindReason,
}

/// Why a change is left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum LeftBehindReason {
  /// No newer generation the game loads has an item standing for the changed one.
  NoLoadedCounterpart,
  /// The change removes an item no newer generation the game loads has: the loaded generation
  /// lacks it already.
  AlreadyAbsent,
  /// The addition is an entity with a parent, or an item of another array than entities and LOD
  /// lights.
  AdditionNotCarried,
}

/// The changes of one source YMAP to carry: its resource, file and stream path, the files its
/// diff was taken from, and its differences.
struct CarrySource {
  resource_id: String,
  resource_name: String,
  stream_path: String,
  file_name: String,
  origin: YmapPropagationOrigin,
  diffs: YmapDiffFields,
}

/// Carries the changes of `sources`, the merge's source YMAPs, to newer loaded generations by
/// the generation index of the vanilla cache at `vanilla_cache_dir`: writes the documents and
/// the changes left behind into [`GENERATIONS_DIR`] of `staging`, and returns the documents as
/// merge sources, newest origin first, with the changes left behind. Each source's diff is taken after its LOD light pair
/// document's `skip` and its overwrite in `overwrites`.
pub(super) fn carry_generations(
  vanilla_cache_dir: &Path,
  sources: &[MergeSourceFile],
  overwrites: &Overwrites,
  staging: &Path,
) -> Result<(Vec<MergeSourceFile>, Vec<LeftBehind>)> {
  let Some(index) = GenerationIndex::load(vanilla_cache_dir)? else {
    tracing::warn!(
      "The vanilla cache has no YMAP generation index; changes are not carried over to newer generations"
    );
    return Ok((Vec::new(), Vec::new()));
  };
  let mut carry_sources = Vec::new();
  for source in sources.iter().filter(|source| !source.propagated) {
    let Some(diff_path) = &source.diff_path else {
      continue;
    };
    let Ok(document) = crate::common::json::read_json::<YmapComparisonDiff>(diff_path) else {
      continue;
    };
    let origin = YmapPropagationOrigin {
      target: document.target.clone(),
      base: document.base.clone(),
    };
    let mut diffs = document.diffs;
    let implied = source
      .implied_path
      .as_ref()
      .and_then(|path| crate::common::json::read_json::<YmapImplied>(path).ok())
      .filter(|implied| implied.target == origin.target && implied.base == origin.base);
    if let Some(skip) = implied.as_ref().and_then(|implied| implied.skip.as_ref()) {
      crate::app::map::overwrite::skip_diffs(&mut diffs, skip)?;
    }
    if let Some(overwrite) = overwrites.for_source(source) {
      let label = format!("{} ({})", source.file_name, source.resource);
      overwrite.apply(
        &mut diffs,
        &OverwriteFingerprint::from(&origin.target),
        &OverwriteFingerprint::from(&origin.base),
        &label,
      )?;
    }
    let partner = implied.and_then(|implied| implied.partner).map(|partner| {
      let side = LightSide::of(&source.file_name);
      let kept = side.map(|side| side.removed(&diffs)).unwrap_or_default();
      (partner.file_name, keep_removals(partner.diffs, &kept))
    });
    let carried = |file_name: String, diffs: YmapDiffFields| CarrySource {
      resource_id: source.resource.clone(),
      resource_name: resource_name(&source.resource),
      stream_path: source.stream_path.clone(),
      file_name,
      origin: origin.clone(),
      diffs,
    };
    carry_sources.push(carried(source.file_name.to_ascii_lowercase(), diffs));
    if let Some((file_name, diffs)) = partner {
      carry_sources.push(carried(file_name, diffs));
    }
  }
  let vanilla_files = crate::app::map::vanilla_cache::latest_files(vanilla_cache_dir)?
    .into_iter()
    .map(|file| (file.name.to_ascii_lowercase(), (file.path, file.object)))
    .collect();
  let propagation = Propagation {
    vanilla_files,
    index: &index,
    counterparts: Counterparts::new(&index, vanilla_cache_dir),
    item_counts: HashMap::new(),
    left_behind: Vec::new(),
  };
  let target = staging.join(GENERATIONS_DIR);
  ensure_safe_path(staging, &target)?;
  let (mut documents, left_behind) = propagation.write(&carry_sources, &target)?;
  fs::create_dir_all(&target)?;
  write_json(&target.join(LEFT_BEHIND_FILE), &left_behind)?;
  if !left_behind.is_empty() {
    tracing::info!(
      "{} changes of vanilla YMAPs that merges do not deploy reach no YMAP the game loads",
      left_behind.len()
    );
  }
  tracing::info!("Carried changes over to newer generations in {} documents", documents.len());
  // Changes carried from a newer generation come first, so that they win over an older one's.
  documents.sort_by(|left, right| {
    right.origin_version.cmp(&left.origin_version).then_with(|| left.path.cmp(&right.path))
  });
  let carried = documents
    .into_iter()
    .map(|document| {
      let path = target.join(&document.path);
      MergeSourceFile {
        resource: document.resource,
        path: path.clone(),
        stored_path: format!("{GENERATIONS_DIR}/{}", document.path),
        moved: false,
        original_path: PathBuf::new(),
        source_path: String::new(),
        file_name: document.file_name,
        stream_path: String::new(),
        diff_path: Some(path),
        implied_path: None,
        propagated: true,
      }
    })
    .collect();
  Ok((carried, left_behind))
}

/// `diffs`, a LOD light pair document's removals of the vanilla other YMAP, but those of lights
/// not in `kept`, which the source's overwrite keeps it from removing.
fn keep_removals(
  mut diffs: YmapDiffFields,
  kept: &BTreeSet<usize>,
) -> YmapDiffFields {
  fn retain<T, C>(
    items: &mut Option<Vec<ItemDiff<T, C>>>,
    kept: &BTreeSet<usize>,
  ) {
    if let Some(list) = items {
      list.retain(|item| match item {
        ItemDiff::Removed {
          base_index,
          ..
        } => kept.contains(base_index),
        _ => true,
      });
      if list.is_empty() {
        *items = None;
      }
    }
  }
  retain(&mut diffs.lod_lights, kept);
  if let Some(lights) = &mut diffs.distant_lod_lights {
    retain(&mut lights.items, kept);
  }
  diffs
}

/// A document of carried changes, relative to [`GENERATIONS_DIR`].
struct CarriedDocument {
  resource: String,
  file_name: String,
  origin_version: String,
  path: String,
}

/// The arrays of a YMAP diff whose item changes are carried, by their path in its JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
  /// The item in place of the vanilla one, as the diff JSON holds it.
  Replaced {
    item: Value,
  },
}

/// The changes carried to one vanilla YMAP from one generation of one resource, by array and
/// item index, and the items and physics dictionaries added to the end of its arrays.
#[derive(Default)]
struct Target {
  items: BTreeMap<Array, BTreeMap<usize, Carried>>,
  /// Per array, the vanilla YMAP's item count, after which items are added, and each addition
  /// as the diff JSON holds it but for its anchor and target index.
  added: BTreeMap<Array, (usize, Vec<Map<String, Value>>)>,
  added_dictionaries: BTreeSet<String>,
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
      Some(
        Carried::Modified {
          ..
        }
        | Carried::Replaced {
          ..
        },
      ) => matches!(change, Carried::Removed { .. }),
      Some(Carried::Removed {
        ..
      }) => false,
    };
    if replaces {
      items.insert(index, change);
    }
  }

  fn is_empty(&self) -> bool {
    self.items.is_empty() && self.added.is_empty()
  }

  /// The diff JSON of the carried changes.
  fn diffs(&self) -> Value {
    let mut root = Map::new();
    let mut arrays = BTreeMap::<Array, Vec<Value>>::new();
    for (array, items) in &self.items {
      arrays.insert(
        *array,
        items
          .iter()
          .map(|(&index, change)| match change {
            Carried::Removed {
              reason,
            } => serde_json::json!({ "Removed": { "base_index": index, "reason": reason } }),
            Carried::Modified {
              changes,
              reason,
            } => serde_json::json!({ "Modified": {
              "base_index": index, "target_index": index, "changes": changes, "reason": reason,
            }}),
            Carried::Replaced {
              item,
            } => serde_json::json!({ "Replaced": {
              "base_index": index, "target_index": index, "item": item,
            }}),
          })
          .collect(),
      );
    }
    for (array, (count, additions)) in &self.added {
      let anchor = count.checked_sub(1);
      arrays.entry(*array).or_default().extend(additions.iter().map(|addition| {
        let mut addition = addition.clone();
        addition.insert("anchor_base_index".into(), serde_json::json!(anchor));
        serde_json::json!({ "Added": addition })
      }));
    }
    arrays.retain(|_, diffs| !diffs.is_empty());
    for (array, diffs) in arrays {
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
    if self.added.contains_key(&Array::Entities) && !self.added_dictionaries.is_empty() {
      root.insert(
        "physics_dictionaries".into(),
        Value::Array(
          self
            .added_dictionaries
            .iter()
            .map(|name| serde_json::json!({ "Added": { "anchor_base_index": null, "item": name } }))
            .collect(),
        ),
      );
    }
    Value::Object(root)
  }
}

/// A change a resource's diff makes to a vanilla item.
struct Change {
  array: Array,
  index: usize,
  kind: ChangeKind,
}

/// What a change does to a vanilla item.
enum ChangeKind {
  Removed,
  /// The field changes, as the diff JSON holds them.
  Modified(Value),
  /// The item in its place, as the diff JSON holds it.
  Replaced(Value),
}

impl ChangeKind {
  fn label(&self) -> &'static str {
    match self {
      Self::Removed => "Removed",
      Self::Modified(_) => "Modified",
      Self::Replaced(_) => "Replaced",
    }
  }
}

/// An addition a resource's diff makes.
struct Addition {
  array: Array,
  target_index: Option<usize>,
  /// The addition as the diff JSON holds it but for its anchor and target index, when it can be
  /// carried: an entity with no parent, or a LOD light.
  carried: Option<Map<String, Value>>,
}

/// The documents of one resource, keyed by the vanilla YMAP they change and the stage of the
/// generation the changes come from.
type Targets = BTreeMap<(String, String), Target>;

struct Propagation<'a> {
  /// The latest vanilla files keyed by lowercase file name, as files of the vanilla archive
  /// with their paths in it.
  vanilla_files: HashMap<String, (std::path::PathBuf, String)>,
  index: &'a GenerationIndex,
  counterparts: Counterparts<'a>,
  /// The item count of each array of each latest vanilla YMAP read so far.
  item_counts: HashMap<(String, Array), usize>,
  left_behind: Vec<LeftBehind>,
}

impl Propagation<'_> {
  /// Writes the documents of the changes of `sources` into `directory` and lists them, with the
  /// changes left behind.
  fn write(
    mut self,
    sources: &[CarrySource],
    directory: &Path,
  ) -> Result<(Vec<CarriedDocument>, Vec<LeftBehind>)> {
    let mut documents = Vec::new();
    let mut vanilla_files = HashMap::<String, YmapComparisonFile>::new();
    let mut by_resource = BTreeMap::<&str, Vec<&CarrySource>>::new();
    for source in sources {
      by_resource.entry(&source.resource_id).or_default().push(source);
    }
    for (resource_id, sources) in by_resource {
      let mut targets = Targets::new();
      let mut resource_name = String::new();
      for carry_source in sources {
        resource_name.clone_from(&carry_source.resource_name);
        let source = Source {
          resource_id,
          resource_name: &carry_source.resource_name,
          stream_path: &carry_source.stream_path,
          file_name: &carry_source.file_name,
        };
        let reached = self.carry(&source, &carry_source.diffs, &mut targets)?;
        for key in reached {
          let target = targets.get_mut(&key).ok_or("Carried change has no target")?;
          if !target.origins.contains(&carry_source.origin) {
            target.origins.push(carry_source.origin.clone());
          }
        }
      }
      for ((file_name, origin_version), target) in targets {
        if target.is_empty() {
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
          resource: resource_id.to_owned(),
          base: vanilla,
          origins: target.origins.clone(),
          diffs: serde_json::from_value(target.diffs())
            .map_err(|error| format!("Carried diff of {file_name} for {resource_name}: {error}"))?,
        };
        let relative =
          Path::new(&resource_name).join(&origin_version).join(format!("{file_name}.diff.json"));
        let path = directory.join(&relative);
        ensure_safe_path(directory, &path)?;
        fs::create_dir_all(path.parent().ok_or("Carried diff path has no parent")?)?;
        write_json(&path, &document)?;
        documents.push(CarriedDocument {
          resource: resource_id.to_owned(),
          file_name,
          origin_version,
          path: relative.to_string_lossy().replace('\\', "/"),
        });
      }
    }
    Ok((documents, self.left_behind))
  }

  /// Adds the changes `diffs` (of the source's YMAP) makes to vanilla items to `targets`, at the
  /// items of newer loaded generations standing for them; returns the targets it reached.
  fn carry(
    &mut self,
    source: &Source<'_>,
    diffs: &YmapDiffFields,
    targets: &mut Targets,
  ) -> Result<BTreeSet<(String, String)>> {
    let mut reached = BTreeSet::new();
    let index = self.index;
    let file_label = source.file_name.to_ascii_lowercase();
    let Some(located) = index.locate(&file_label) else {
      return Ok(reached);
    };
    let generation = &located.family.generations[located.generation];
    let listed = generation.files[located.file].clone();
    let origin_version = generation.first_version.clone();
    let diffs = serde_json::to_value(diffs)?;
    for change in changes(&diffs) {
      let counterparts =
        self.counterparts.newer(&listed, change.array.kind(), change.index as u32)?;
      if counterparts.is_empty() {
        // A change of a YMAP that merges deploy applies there.
        if index.loads(&listed) || index.successor(&file_label).is_none() {
          continue;
        }
        tracing::debug!(
          "{file_label} {}[{}] of {} has no counterpart in a loaded generation",
          change.array.label(),
          change.index,
          source.resource_name
        );
        self.leave_behind(
          source,
          change.array,
          Some(change.index),
          &change,
          match change.kind {
            ChangeKind::Removed => LeftBehindReason::AlreadyAbsent,
            _ => LeftBehindReason::NoLoadedCounterpart,
          },
        );
        continue;
      }
      for (latest, latest_index) in counterparts {
        let reason = format!(
          "propagated from {}: {file_label} {}[{}]",
          source.resource_name,
          change.array.label(),
          change.index
        );
        // The YMAPs the change goes to, with the array it changes there.
        let mut destinations = Vec::new();
        match change.array {
          Array::LodLights | Array::DistantLodLights => {
            let lod =
              LightSide::partner_name(&latest).filter(|lod| self.vanilla_files.contains_key(lod));
            match (&change.kind, change.array) {
              (ChangeKind::Removed, _) => {
                destinations.push((latest.clone(), Array::DistantLodLights));
                destinations.extend(lod.map(|lod| (lod, Array::LodLights)));
              }
              (_, Array::DistantLodLights) => {
                destinations.push((latest.clone(), Array::DistantLodLights));
              }
              _ => destinations.extend(lod.map(|lod| (lod, Array::LodLights))),
            }
          }
          array => destinations.push((latest.clone(), array)),
        }
        for (destination, array) in destinations {
          let carried = match &change.kind {
            ChangeKind::Removed => Carried::Removed {
              reason: reason.clone(),
            },
            ChangeKind::Modified(changes) => Carried::Modified {
              changes: changes.clone(),
              reason: reason.clone(),
            },
            ChangeKind::Replaced(item) => Carried::Replaced {
              item: item.clone(),
            },
          };
          let key = (destination, origin_version.clone());
          targets.entry(key.clone()).or_default().carry(array, latest_index as usize, carried);
          reached.insert(key);
        }
      }
    }
    // Additions to a YMAP the game loads stay there.
    let Some(successor) = index.successor(&file_label) else {
      return Ok(reached);
    };
    let additions = additions(&diffs);
    let dictionaries = added_dictionaries(&diffs);
    // Lights a resource adds to one YMAP of a pair only are left out of its diff before.
    for addition in additions {
      let Some(carried) = addition.carried.clone() else {
        self.leave_behind_addition(source, &addition);
        continue;
      };
      let count = self.item_count(&successor, addition.array)?;
      let key = (successor.clone(), origin_version.clone());
      let target = targets.entry(key.clone()).or_default();
      target.added.entry(addition.array).or_insert_with(|| (count, Vec::new())).1.push(carried);
      if addition.array == Array::Entities {
        target.added_dictionaries.extend(dictionaries.iter().cloned());
      }
      reached.insert(key);
    }
    Ok(reached)
  }

  fn leave_behind(
    &mut self,
    source: &Source<'_>,
    array: Array,
    index: Option<usize>,
    change: &Change,
    reason: LeftBehindReason,
  ) {
    self.left_behind.push(LeftBehind {
      resource: source.resource_id.to_owned(),
      stream_path: source.stream_path.to_owned(),
      file_name: source.file_name.to_ascii_lowercase(),
      array: array.label(),
      index,
      change: change.kind.label().into(),
      reason,
    });
  }

  fn leave_behind_addition(
    &mut self,
    source: &Source<'_>,
    addition: &Addition,
  ) {
    self.left_behind.push(LeftBehind {
      resource: source.resource_id.to_owned(),
      stream_path: source.stream_path.to_owned(),
      file_name: source.file_name.to_ascii_lowercase(),
      array: addition.array.label(),
      index: addition.target_index,
      change: "Added".into(),
      reason: LeftBehindReason::AdditionNotCarried,
    });
  }

  /// How many items of `array` the latest vanilla YMAP `file_name` has.
  fn item_count(
    &mut self,
    file_name: &str,
    array: Array,
  ) -> Result<usize> {
    let key = (file_name.to_owned(), array);
    if let Some(count) = self.item_counts.get(&key) {
      return Ok(*count);
    }
    let (path, _) = self
      .vanilla_files
      .get(file_name)
      .ok_or_else(|| format!("No latest vanilla YMAP named {file_name}"))?;
    let ymap = crate::app::map::vanilla::read_ymap(path)?;
    let count = match array {
      Array::Entities => ymap.entities.len(),
      Array::LodLights => ymap.lod_lights.len(),
      Array::DistantLodLights => ymap.distant_lod_lights.items.len(),
      Array::CarGenerators => ymap.car_generators.len(),
      Array::BoxOccluders => ymap.box_occluders.len(),
      Array::OccludeModels => ymap.occlude_models.len(),
      Array::TimeCycleModifiers => ymap.time_cycle_modifiers.len(),
    };
    self.item_counts.insert(key, count);
    Ok(count)
  }

  /// The latest vanilla YMAP `file_name` as documents record it.
  fn vanilla_file(
    &self,
    file_name: &str,
  ) -> Result<YmapComparisonFile> {
    let (path, object) = self
      .vanilla_files
      .get(&file_name.to_ascii_lowercase())
      .ok_or_else(|| format!("No latest vanilla YMAP named {file_name}"))?;
    let fingerprint = crate::common::fingerprint::Fingerprint::read(path, None, false)?;
    Ok(YmapComparisonFile {
      path: object.clone(),
      sha256: fingerprint.sha256,
      size: fingerprint.size,
      modified_seconds: fingerprint.modified_seconds,
      modified_nanos: fingerprint.modified_nanos,
    })
  }
}

/// The resource YMAP changes are carried from.
struct Source<'a> {
  resource_id: &'a str,
  resource_name: &'a str,
  stream_path: &'a str,
  file_name: &'a str,
}

/// The removals, modifications and replacements of vanilla items in the diff JSON `diffs`, in
/// array order; entity parent changes are left out of modifications, and modifications left with
/// no change are dropped, as are those changing items of a nested array, such as the triangles of
/// an occlude model, whose indices need not hold in another generation's item.
fn changes(diffs: &Value) -> Vec<Change> {
  let mut changes = Vec::new();
  for array in Array::ALL {
    for item in items(diffs, array) {
      if let Some(index) = item.pointer("/Removed/base_index").and_then(Value::as_u64) {
        changes.push(Change {
          array,
          index: index as usize,
          kind: ChangeKind::Removed,
        });
      } else if let Some(modified) = item.get("Modified")
        && let Some(index) = modified.get("base_index").and_then(Value::as_u64)
        && let Some(fields) = modified.get("changes").and_then(Value::as_object)
        && !fields.is_empty()
        && !fields.values().any(Value::is_array)
      {
        changes.push(Change {
          array,
          index: index as usize,
          kind: ChangeKind::Modified(Value::Object(fields.clone())),
        });
      } else if let Some(replaced) = item.get("Replaced")
        && let Some(index) = replaced.get("base_index").and_then(Value::as_u64)
        && let Some(item) = replaced.get("item")
      {
        changes.push(Change {
          array,
          index: index as usize,
          kind: ChangeKind::Replaced(item.clone()),
        });
      }
    }
  }
  changes
}

/// The additions in the diff JSON `diffs`, in array order.
fn additions(diffs: &Value) -> Vec<Addition> {
  let mut additions = Vec::new();
  for array in Array::ALL {
    for item in items(diffs, array) {
      let Some(added) = item.get("Added") else {
        continue;
      };
      let target_index =
        added.get("target_index").and_then(Value::as_u64).map(|index| index as usize);
      let carried = match array {
        Array::Entities if added.get("parent").is_some_and(|parent| parent == "None") => {
          Some(["entity", "parent"])
        }
        Array::LodLights | Array::DistantLodLights => Some(["item", "item"]),
        _ => None,
      }
      .and_then(|keys| {
        keys.iter().map(|key| Some(((*key).to_owned(), added.get(*key)?.clone()))).collect()
      });
      additions.push(Addition {
        array,
        target_index,
        carried,
      });
    }
  }
  additions
}

/// The physics dictionaries the diff JSON `diffs` adds.
fn added_dictionaries(diffs: &Value) -> BTreeSet<String> {
  diffs
    .get("physics_dictionaries")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|item| item.pointer("/Added/item").and_then(Value::as_str))
    .map(str::to_owned)
    .collect()
}

/// The diffs of `array` in the diff JSON `diffs`.
fn items(
  diffs: &Value,
  array: Array,
) -> impl Iterator<Item = &Value> {
  array
    .path()
    .iter()
    .try_fold(diffs, |value, key| value.get(*key))
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn changes_read_removals_field_changes_and_replacements_of_every_carried_array() {
    let diffs = serde_json::json!({
      "entities": [
        { "Removed": { "base_index": 3 } },
        { "Modified": { "base_index": 1, "target_index": 0, "changes": {}, "parent": "None" } },
        { "Modified": { "base_index": 2, "target_index": 1, "changes": { "flags": 8 } } },
        { "Added": { "anchor_base_index": 0, "entity": {}, "parent": "None" } },
      ],
      "distant_lod_lights": { "items": [{ "Removed": { "base_index": 7, "implied": true } }] },
      "car_generators": [{ "Replaced": { "base_index": 0, "item": {} } }],
      "occlude_models": [
        { "Modified": { "base_index": 0, "target_index": 0, "changes": {
          "bmin": { "x": 1.0 }, "triangles": [{ "Removed": { "base_index": 2 } }],
        } } },
      ],
    });
    let changes = changes(&diffs)
      .into_iter()
      .map(|change| {
        let value = match change.kind {
          ChangeKind::Removed => None,
          ChangeKind::Modified(value) | ChangeKind::Replaced(value) => Some(value),
        };
        (change.array, change.index, value)
      })
      .collect::<Vec<_>>();
    assert_eq!(
      changes,
      [
        (Array::Entities, 3, None),
        (Array::Entities, 2, Some(serde_json::json!({ "flags": 8 }))),
        (Array::DistantLodLights, 7, None),
        (Array::CarGenerators, 0, Some(serde_json::json!({}))),
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
        { "Removed": { "base_index": 4, "reason": "removed" } },
        { "Removed": { "base_index": 5, "reason": "removed" } },
      ])
    );
    assert_eq!(
      json["lod_lights"],
      serde_json::json!([{ "Removed": { "base_index": 2, "reason": "light" } }])
    );
  }
}
