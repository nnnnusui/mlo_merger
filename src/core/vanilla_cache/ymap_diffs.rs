//! Differences of each vanilla YMAP from its previous version.
//!
//! For every YMAP that a version after its first changes, `diff/{version}/ymap/{name}.diff.json`
//! holds its differences from the version before, and `ymap_versions.json` lists the versions
//! holding each such YMAP's content, oldest first. The diffs record every change however small,
//! entity parents as raw indices and entity moves, so applying them in order to a YMAP's oldest
//! file rebuilds each later version ([`VanillaYmapHistory::rebuild`]). Grass batches whose
//! instance changes would not rebuild them exactly are replaced whole, and any other array whose
//! diff would not, such as one whose kept items are reordered, is rewritten: every item removed
//! and the new ones added in order. The documents are written compactly, as they are read by
//! tools and can be large.
//!
//! A rebuild of the cache keeps the previous cache's diffs between unchanged files while the
//! diff layout, [`ALGORITHM_VERSION`] and the hash names the YMAPs are decoded with are
//! unchanged; a forced rebuild takes every diff again.
//!
//! Generation is currently disabled (see "Limitations" in docs/reference.md), so the
//! writing side is unused.
#![allow(dead_code)]

use std::collections::HashMap;

use rayon::prelude::*;
use struct_diff::Diff;

use super::*;
use crate::core::{
  config::matching::MatchTolerances,
  conversion::load_hash_names_or_empty,
  format::ymap::{
    binary::ymap_to_model,
    compare::{
      EntityParent, ItemDiff, YmapComparisonDiff, YmapDiffContext, YmapDiffFields, YmapEntityDiff,
      instanced_data, raw_entity_parents,
    },
    model::Ymap,
  },
};

const INDEX_FILE: &str = "ymap_versions.json";
/// Bump when how the diffs are taken changes, so diffs taken before are not reused.
const ALGORITHM_VERSION: u32 = 1;

/// Differences of a vanilla YMAP from its previous version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct VanillaYmapDiff {
  /// The layout of `diffs`, [`YmapComparisonDiff::SCHEMA_VERSION`].
  pub schema_version: u32,
  pub previous: VanillaYmapVersion,
  pub current: VanillaYmapVersion,
  pub diffs: YmapDiffFields,
}

/// One version of a vanilla YMAP's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct VanillaYmapVersion {
  /// The vanilla version, such as `0001-update`.
  pub version: String,
  pub sha256: String,
  /// The raw archive file holding it, relative to the archive.
  pub object: String,
}

/// `ymap_versions.json`: the versions of each vanilla YMAP that a version after its first
/// changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "VanillaYmapVersionIndex"))]
pub(super) struct YmapVersionIndex {
  format_version: u32,
  vanilla_manifest_sha256: String,
  version: String,
  /// [`YmapComparisonDiff::SCHEMA_VERSION`] when the diffs were taken.
  diff_schema_version: u32,
  /// [`ALGORITHM_VERSION`] when the diffs were taken.
  algorithm_version: u32,
  /// SHA-256 of the raw archive's `hash_names.json` the YMAPs were decoded with, if it had one.
  hash_names_sha256: Option<String>,
  /// The versions holding each YMAP's content, oldest first, keyed by lowercase file name.
  ymaps: BTreeMap<String, Vec<VanillaYmapVersion>>,
}

/// The versions of one vanilla YMAP, read from the derived cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaYmapHistory {
  /// Lowercase file name.
  pub name: String,
  /// The versions holding its content, oldest first.
  pub versions: Vec<VanillaYmapVersion>,
  cache_dir: PathBuf,
}

impl VanillaYmapHistory {
  /// The diff document taking the YMAP from `versions[index - 1]` to `versions[index]`.
  pub fn diff_path(
    &self,
    index: usize,
  ) -> PathBuf {
    self.cache_dir.join(diff_object(&self.versions[index].version, &self.name))
  }

  /// Reads the diff taking the YMAP from `versions[index - 1]` to `versions[index]`, rejecting a
  /// document that describes other versions.
  pub fn read_diff(
    &self,
    index: usize,
  ) -> Result<VanillaYmapDiff> {
    let path = self.diff_path(index);
    let diff: VanillaYmapDiff = serde_json::from_reader(BufReader::new(
      fs::File::open(&path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?,
    ))?;
    if diff.schema_version != YmapComparisonDiff::SCHEMA_VERSION
      || diff.previous != self.versions[index - 1]
      || diff.current != self.versions[index]
    {
      return Err(format!("Vanilla YMAP diff is stale: {}", path.display()).into());
    }
    Ok(diff)
  }

  /// The YMAP as of `versions[index]`: its oldest file in the raw archive at `vanilla_dir`,
  /// decoded with `hash_names`, with the diffs of every later version through `index` applied.
  ///
  /// `hash_names` must be the archive's, which the diffs were taken with.
  pub fn rebuild(
    &self,
    vanilla_dir: &Path,
    index: usize,
    hash_names: &HashMap<u32, String>,
  ) -> Result<Ymap> {
    let mut ymap = read_version(vanilla_dir, &self.versions[0], hash_names)?;
    for index in 1..=index {
      ymap
        .apply(&self.read_diff(index)?.diffs)
        .map_err(|error| format!("{}: {error}", self.diff_path(index).display()))?;
    }
    Ok(ymap)
  }
}

/// The versions of vanilla YMAP `name` recorded in the derived cache at `cache_dir`, or `None`
/// when no version after its first changes it.
pub fn ymap_history(
  cache_dir: &Path,
  name: &str,
) -> Result<Option<VanillaYmapHistory>> {
  let path = cache_dir.join(INDEX_FILE);
  let index: YmapVersionIndex = serde_json::from_reader(BufReader::new(
    fs::File::open(&path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?,
  ))?;
  if index.format_version != CACHE_SCHEMA
    || index.diff_schema_version != YmapComparisonDiff::SCHEMA_VERSION
  {
    return Err(format!("Unsupported vanilla YMAP version index: {}", path.display()).into());
  }
  let name = name.to_ascii_lowercase();
  Ok(index.ymaps.get(&name).map(|versions| VanillaYmapHistory {
    versions: versions.clone(),
    name,
    cache_dir: cache_dir.to_path_buf(),
  }))
}

/// The diff document of YMAP `name` at `version`, relative to the cache.
fn diff_object(
  version: &str,
  name: &str,
) -> String {
  format!("diff/{version}/ymap/{name}.diff.json")
}

/// Reads one version of a YMAP from the raw archive at `vanilla_dir`.
fn read_version(
  vanilla_dir: &Path,
  version: &VanillaYmapVersion,
  hash_names: &HashMap<u32, String>,
) -> Result<Ymap> {
  let path = checked_source_path(vanilla_dir, &version.object)?;
  ymap_to_model(&fs::read(&path)?, hash_names)
    .map_err(|error| format!("{}: {error}", path.display()).into())
}

/// The differences taking `previous` to `current`, recording every change; see the module docs.
/// Also returns whether applying them to `previous` rebuilds `current` exactly.
fn exact_diff(
  previous: &Ymap,
  current: &Ymap,
) -> (YmapDiffFields, bool) {
  let ctx = YmapDiffContext {
    tolerances: MatchTolerances::ymap_exact(),
    vanilla_parents: raw_entity_parents(previous),
    modded_parents: raw_entity_parents(current),
    record_moves: true,
  };
  let mut diffs = previous.diff(current, &ctx).unwrap_or_default();
  if let Some(grass) =
    diffs.instanced_data.as_mut().and_then(|data| data.grass_instance_list.as_mut())
  {
    instanced_data::replace_inexact_grass_batches(
      &previous.instanced_data.grass_instance_list,
      &current.instanced_data.grass_instance_list,
      grass,
    );
  }
  let mut rebuilt = previous.clone();
  if rebuilt.apply(&diffs).is_ok() {
    rewrite_inexact_arrays(previous, &rebuilt, current, &mut diffs);
  }
  let mut rebuilt = previous.clone();
  let exact = rebuilt.apply(&diffs).is_ok() && rebuilt == *current;
  (diffs, exact)
}

/// Rewrites the diff of each array that `diffs`, applied to `previous`, rebuilt as `rebuilt`
/// rather than as `current` has it; see [`ItemDiff::rewrite`].
fn rewrite_inexact_arrays(
  previous: &Ymap,
  rebuilt: &Ymap,
  current: &Ymap,
  diffs: &mut YmapDiffFields,
) {
  fn rewrite<T: Clone + PartialEq, C>(
    diff: &mut Option<Vec<ItemDiff<T, C>>>,
    [previous, rebuilt, current]: [&[T]; 3],
  ) {
    if rebuilt != current {
      *diff = Some(ItemDiff::rewrite(previous.len(), current));
    }
  }
  if rebuilt.entities != current.entities {
    let removed = (0..previous.entities.len()).map(|index| YmapEntityDiff::Removed {
      index,
      reason: None,
    });
    let added = current.entities.iter().map(|entity| YmapEntityDiff::Added {
      after: None,
      entity: entity.clone(),
      parent: EntityParent::raw(entity),
    });
    diffs.entities = Some(removed.chain(added).collect());
  }
  rewrite(
    &mut diffs.box_occluders,
    [&previous.box_occluders, &rebuilt.box_occluders, &current.box_occluders],
  );
  rewrite(
    &mut diffs.occlude_models,
    [&previous.occlude_models, &rebuilt.occlude_models, &current.occlude_models],
  );
  rewrite(&mut diffs.lod_lights, [&previous.lod_lights, &rebuilt.lod_lights, &current.lod_lights]);
  rewrite(
    &mut diffs.distant_lod_lights.get_or_insert_with(Default::default).items,
    [
      &previous.distant_lod_lights.items,
      &rebuilt.distant_lod_lights.items,
      &current.distant_lod_lights.items,
    ],
  );
  rewrite(
    &mut diffs.physics_dictionaries,
    [&previous.physics_dictionaries, &rebuilt.physics_dictionaries, &current.physics_dictionaries],
  );
  let data = diffs.instanced_data.get_or_insert_with(Default::default);
  rewrite(
    &mut data.prop_instance_list,
    [previous, rebuilt, current].map(|ymap| ymap.instanced_data.prop_instance_list.as_slice()),
  );
  rewrite(
    &mut data.grass_instance_list,
    [previous, rebuilt, current].map(|ymap| ymap.instanced_data.grass_instance_list.as_slice()),
  );
  rewrite(
    &mut diffs.time_cycle_modifiers,
    [&previous.time_cycle_modifiers, &rebuilt.time_cycle_modifiers, &current.time_cycle_modifiers],
  );
  rewrite(
    &mut diffs.car_generators,
    [&previous.car_generators, &rebuilt.car_generators, &current.car_generators],
  );
  // Left empty unless rewritten.
  if diffs.distant_lod_lights.as_ref().is_some_and(|lights| *lights == Default::default()) {
    diffs.distant_lod_lights = None;
  }
  if diffs.instanced_data.as_ref().is_some_and(|data| *data == Default::default()) {
    diffs.instanced_data = None;
  }
}

/// The SHA-256 of the raw archive's `hash_names.json`, or `None` when it has none.
fn hash_names_sha256(vanilla_dir: &Path) -> Result<Option<String>> {
  match fs::read(vanilla_dir.join("hash_names.json")) {
    Ok(bytes) => Ok(Some(hex::encode(Sha256::digest(&bytes)))),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(error) => Err(error.into()),
  }
}

/// The versions of each YMAP that a version through `through_index` changes after its first.
fn ymap_versions(
  manifest: &VanillaCacheManifest,
  through_index: usize,
) -> BTreeMap<String, Vec<VanillaYmapVersion>> {
  let mut ymaps = BTreeMap::<String, Vec<VanillaYmapVersion>>::new();
  for version in manifest.versions.iter().take(through_index + 1) {
    for (name, change) in &version.changes {
      let name = name.to_ascii_lowercase();
      if name.ends_with(".ymap") {
        ymaps.entry(name).or_default().push(VanillaYmapVersion {
          version: version.id.clone(),
          sha256: change.file.sha256.clone(),
          object: change.file.object.clone(),
        });
      }
    }
  }
  ymaps.retain(|_, versions| versions.len() > 1);
  ymaps
}

/// Writes the diffs and index of the YMAPs that versions through `through_index` change after
/// their first into `staging`, copying those of the published cache at `previous`, if given, that
/// are current.
pub(super) fn write_ymap_diffs(
  vanilla_dir: &Path,
  manifest: &VanillaCacheManifest,
  revision: &str,
  through_index: usize,
  staging: &Path,
  previous: Option<&Path>,
) -> Result<()> {
  let ymaps = ymap_versions(manifest, through_index);
  let hash_names_sha256 = hash_names_sha256(vanilla_dir)?;
  let reusable = previous
    .and_then(read_index)
    .filter(|index| {
      index.diff_schema_version == YmapComparisonDiff::SCHEMA_VERSION
        && index.algorithm_version == ALGORITHM_VERSION
        && index.hash_names_sha256 == hash_names_sha256
    })
    .map(|index| index.ymaps)
    .unwrap_or_default();
  let mut reused = 0;
  let mut pending = Vec::new();
  for (name, versions) in &ymaps {
    let old = reusable.get(name).map(Vec::as_slice).unwrap_or_default();
    for pair in versions.windows(2) {
      let object = diff_object(&pair[1].version, name);
      fs::create_dir_all(staging.join(&object).parent().ok_or("Diff has no parent")?)?;
      let reusable = previous
        .map(|previous| previous.join(&object))
        .filter(|path| old.windows(2).any(|old| old == pair) && path.is_file());
      if let Some(path) = reusable {
        fs::copy(path, staging.join(&object))?;
        reused += 1;
      } else {
        pending.push((object, &pair[0], &pair[1]));
      }
    }
  }
  if !pending.is_empty() {
    let hash_names = load_hash_names_or_empty(&vanilla_dir.join("hash_names.json"));
    // Boxed errors cannot cross threads, so each is reported as text.
    pending.par_iter().try_for_each(|(object, previous, current)| {
      let write = || -> Result<()> {
        let (diffs, exact) = exact_diff(
          &read_version(vanilla_dir, previous, &hash_names)?,
          &read_version(vanilla_dir, current, &hash_names)?,
        );
        if !exact {
          tracing::warn!("{object} does not rebuild {} exactly", current.object);
        }
        write_compact_json(
          &staging.join(object),
          &VanillaYmapDiff {
            schema_version: YmapComparisonDiff::SCHEMA_VERSION,
            previous: (*previous).clone(),
            current: (*current).clone(),
            diffs,
          },
        )
      };
      write().map_err(|error| format!("{object}: {error}"))
    })?;
  }
  tracing::info!("Vanilla YMAP version diffs: {} written, {reused} reused", pending.len());
  write_json(
    &staging.join(INDEX_FILE),
    &YmapVersionIndex {
      format_version: CACHE_SCHEMA,
      vanilla_manifest_sha256: revision.to_owned(),
      version: manifest.versions[through_index].id.clone(),
      diff_schema_version: YmapComparisonDiff::SCHEMA_VERSION,
      algorithm_version: ALGORITHM_VERSION,
      hash_names_sha256,
      ymaps,
    },
  )
}

fn write_compact_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let mut writer = BufWriter::new(fs::File::create(path)?);
  serde_json::to_writer(&mut writer, value)?;
  writer.flush()?;
  Ok(())
}

fn read_index(cache_dir: &Path) -> Option<YmapVersionIndex> {
  let file = fs::File::open(cache_dir.join(INDEX_FILE)).ok()?;
  serde_json::from_reader(BufReader::new(file)).ok()
}

/// Whether the cache at `output_dir` holds the diffs of every YMAP that versions through
/// `latest_version` change, taken from the archive at `revision` as `ymap_versions.json`
/// records them.
pub(super) fn ymap_diffs_are_current(
  output_dir: &Path,
  revision: &str,
  latest_version: &str,
) -> bool {
  let Some(index) = read_index(output_dir) else {
    return false;
  };
  index.format_version == CACHE_SCHEMA
    && index.vanilla_manifest_sha256 == revision
    && index.version == latest_version
    && index.diff_schema_version == YmapComparisonDiff::SCHEMA_VERSION
    && index.algorithm_version == ALGORITHM_VERSION
    && index.ymaps.iter().all(|(name, versions)| {
      versions[1..]
        .iter()
        .all(|version| output_dir.join(diff_object(&version.version, name)).is_file())
    })
}
