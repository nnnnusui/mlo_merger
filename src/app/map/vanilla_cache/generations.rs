//! Vanilla YMAPs that DLCs ship again under a prefix, and which of their items are the same.
//!
//! DLCs re-release vanilla YMAPs under a prefix of their own, such as `vw_lodlights_medium031`
//! and `m25_2_lodlights_medium031` for `lodlights_medium031`, and the game loads several of these
//! generations. A generation holds mostly the same items as the ones before it, though not at the
//! same indices: items are added and removed, and LOD lights move between blocks. A change a
//! source makes to an item of one generation therefore belongs to the same item of the newer
//! generations too.
//!
//! - A *generation* is the YMAPs of one prefix, the unprefixed YMAPs included. A YMAP's prefix is
//!   the shortest one leaving the name of another vanilla YMAP; prefixes found that way for at
//!   least [`MIN_PREFIX_FILES`] YMAPs also apply to YMAPs without such a counterpart, such as the
//!   new LOD light blocks of a DLC.
//! - A *family* is the YMAPs of all generations whose name without the prefix differs only in its
//!   trailing number, such as every `*distlodlights_small###`, as items move between those.
//!   `lodlights_*` YMAPs belong to the family of their `distlodlights_*` partner, whose lights
//!   they describe at the same indices.
//! - Generations of a family are ordered by the vanilla stage that first ships one of their
//!   YMAPs. For each older and newer generation, a *unit* pairs the items of the older one's
//!   YMAPs with those of the newer one's: items of one kind whose position is within
//!   [`TOLERANCE`] of each other and whose identifying attributes are equal, when exactly one item
//!   of either generation fits. An entity pairs first by its GUID, wherever the newer entity is,
//!   as DLCs rename and move the entities they ship again under the same GUID, such as
//!   `hei_cs6_04_mainrails2` for `cs6_04_mainrails2`; of several entities with its GUID, the one
//!   at its position with its archetype fits. An entity with no GUID in the newer generation pairs
//!   by position and archetype, as some DLCs give every entity of a YMAP a new GUID.
//!
//! Units are what takes time to build, as every YMAP of both generations is decoded, so each is
//! stored by a digest of the YMAPs it pairs (`generation-units/{key}.json`) and kept across cache
//! rebuilds: a unit is built again only when a YMAP of it changes, and switching game builds
//! reuses those built before. `generations.json` lists the families and units of the selected
//! build.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rayon::prelude::*;

use super::*;
use crate::core::format::ymap::{compare::lod_light_pairs::LightSide, model::ymap::Ymap};

/// File name of the generation index in the vanilla cache.
pub(crate) const GENERATIONS_FILE: &str = "generations.json";
/// Directory of the units in the vanilla cache.
const UNITS_DIR: &str = "generation-units";
/// Bump when how generations, families or units are formed, or the unit layout, change.
pub(crate) const ALGORITHM_VERSION: u32 = 4;
/// Prefixes found for at least this many YMAPs apply to YMAPs without an unprefixed counterpart.
const MIN_PREFIX_FILES: usize = 10;
/// Per-axis distance within which two items of a unit are at the same place.
const TOLERANCE: f32 = 0.01;

/// The kinds of items a unit pairs, by the name of the YMAP array holding them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum ItemKind {
  Entities,
  /// The lights of a LOD light pair, by their index in either YMAP of it.
  Lights,
  CarGenerators,
  BoxOccluders,
  TimeCycleModifiers,
}

impl ItemKind {
  const ALL: [Self; 5] = [
    Self::Entities,
    Self::Lights,
    Self::CarGenerators,
    Self::BoxOccluders,
    Self::TimeCycleModifiers,
  ];
}

/// A map from item kinds, with only the kinds it holds; `Partial<Record<ItemKind, V>>` in
/// TypeScript, since specta writes a map keyed by a named enum as an index signature, which
/// TypeScript rejects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByKind<V>(pub BTreeMap<ItemKind, V>);

impl<V> std::ops::Deref for ByKind<V> {
  type Target = BTreeMap<ItemKind, V>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<V> std::ops::DerefMut for ByKind<V> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

#[cfg(feature = "typescript")]
impl<V: specta::Type> specta::Type for ByKind<V> {
  fn inline(
    opts: specta::DefOpts,
    generics: &[specta::DataType],
  ) -> std::result::Result<specta::DataType, specta::ExportError> {
    use specta::{DataType, DataTypeReference, DefOpts};
    let key = ItemKind::reference(
      DefOpts {
        parent_inline: false,
        type_map: opts.type_map,
      },
      &[],
    )?;
    let value = match generics.first() {
      Some(value) => value.clone(),
      None => V::reference(
        DefOpts {
          parent_inline: false,
          type_map: opts.type_map,
        },
        &[],
      )?,
    };
    let record = DataType::Reference(DataTypeReference {
      name: "Record",
      sid: specta::sid!(@with_specta_path; "Record"; specta),
      generics: vec![key, value],
    });
    Ok(DataType::Reference(DataTypeReference {
      name: "Partial",
      sid: specta::sid!(@with_specta_path; "Partial"; specta),
      generics: vec![record],
    }))
  }
}

/// The generations and units of the selected game build, with those first shipped after it; see
/// the module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct GenerationIndex {
  pub format_version: u32,
  pub algorithm_version: u32,
  pub vanilla_manifest_sha256: String,
  pub version: String,
  pub families: BTreeMap<String, Family>,
  /// The YMAPs the game does not load, keyed by lowercase file name, with why: latest ones (see
  /// [`super::activity`]), and those first shipped after [`Self::version`].
  #[serde(default)]
  pub inactive: BTreeMap<String, String>,
  /// The YMAPs first shipped after [`Self::version`], in lowercase.
  #[serde(default)]
  pub newer_than_build: BTreeSet<String>,
}

/// The generations of one family, oldest first, the units pairing their items, and the YMAPs the
/// game loads in place of those it does not.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct Family {
  pub generations: Vec<Generation>,
  pub units: Vec<UnitRef>,
  /// Each YMAP the game does not load whose items another generation's loaded YMAP holds, with
  /// its successor: of the generations pairing its items, the newest newer one, or for a YMAP
  /// first shipped after the game build the newest older one, its loaded YMAP holding most of
  /// them.
  pub successors: BTreeMap<String, String>,
}

/// One generation of a family: its prefix, the stage first shipping one of its YMAPs, and its
/// YMAPs in lowercase, `distlodlights_*` ones standing for their LOD light pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct Generation {
  pub prefix: String,
  pub first_version: String,
  pub files: Vec<String>,
}

/// A unit of a family: its older and newer generation, by index, and its key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct UnitRef {
  pub older: usize,
  pub newer: usize,
  pub key: String,
}

/// The items of an older generation paired with those of a newer one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct GenerationUnit {
  pub algorithm_version: u32,
  pub family: String,
  pub older: Vec<UnitFile>,
  pub newer: Vec<UnitFile>,
  /// Per kind, `[older file, older index, newer file, newer index]` of each pair, files by their
  /// index in [`Self::older`] and [`Self::newer`].
  pub pairs: ByKind<Vec<[u32; 4]>>,
  /// Per kind, how many older items fit no newer item, and how many fit several or share theirs.
  pub unpaired: ByKind<UnpairedCounts>,
}

/// A YMAP of a unit, with the contents it was paired from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct UnitFile {
  pub name: String,
  pub sha256: String,
  /// The `lodlights_*` partner of a `distlodlights_*` YMAP, when vanilla has it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub partner_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct UnpairedCounts {
  pub missing: usize,
  pub ambiguous: usize,
}

impl GenerationIndex {
  /// Reads the index of the vanilla cache at `cache_dir`, `None` when it has none of this
  /// algorithm version.
  pub(crate) fn load(cache_dir: &Path) -> Result<Option<Self>> {
    let path = cache_dir.join(GENERATIONS_FILE);
    if !fs::is_file(&path) {
      return Ok(None);
    }
    let index: Self = crate::common::json::read_json(&path)?;
    Ok((index.algorithm_version == ALGORITHM_VERSION).then_some(index))
  }

  /// The family of the vanilla YMAP `file_name` and its generation there; a `lodlights_*` YMAP
  /// is found as its `distlodlights_*` partner, whose name is returned too.
  pub(crate) fn locate(
    &self,
    file_name: &str,
  ) -> Option<Located<'_>> {
    let name = file_name.to_ascii_lowercase();
    let listed = match LightSide::of(&name) {
      Some(LightSide::LodLights) => LightSide::partner_name(&name)?,
      _ => name,
    };
    self.families.values().find_map(|family| {
      family.generations.iter().enumerate().find_map(|(generation, files)| {
        let file = files.files.iter().position(|file| *file == listed)?;
        Some(Located {
          family,
          generation,
          file,
        })
      })
    })
  }

  /// The successor of the vanilla YMAP `file_name`, which the game does not load, when another
  /// generation holds its items; a `lodlights_*` YMAP's is its `distlodlights_*` partner's
  /// partner.
  pub(crate) fn successor(
    &self,
    file_name: &str,
  ) -> Option<String> {
    let located = self.locate(file_name)?;
    let listed = &located.family.generations[located.generation].files[located.file];
    let successor = located.family.successors.get(listed)?;
    match LightSide::of(&file_name.to_ascii_lowercase()) {
      Some(LightSide::LodLights) => LightSide::partner_name(successor),
      _ => Some(successor.clone()),
    }
  }

  /// Whether the game loads the vanilla YMAP `file_name`, listed as its family lists it.
  pub(crate) fn loads(
    &self,
    file_name: &str,
  ) -> bool {
    !self.inactive.contains_key(&file_name.to_ascii_lowercase())
  }
}

/// A unit's pairs of one kind: the newer file and index of each older file and index.
type UnitPairs = HashMap<(u32, u32), (u32, u32)>;

/// Finds the items of newer generations that items of older ones stand for, loading units as it
/// needs them.
pub(crate) struct Counterparts<'a> {
  index: &'a GenerationIndex,
  cache_dir: &'a Path,
  /// Per unit key, its pairs of each kind.
  units: HashMap<String, HashMap<ItemKind, UnitPairs>>,
  /// Per unit key, its pairs of each kind from the newer item to the older.
  reversed: HashMap<String, HashMap<ItemKind, UnitPairs>>,
  /// Per unit key, its older and newer YMAP names.
  files: HashMap<String, (Vec<String>, Vec<String>)>,
}

impl<'a> Counterparts<'a> {
  pub(crate) fn new(
    index: &'a GenerationIndex,
    cache_dir: &'a Path,
  ) -> Self {
    Self {
      index,
      cache_dir,
      units: HashMap::new(),
      reversed: HashMap::new(),
      files: HashMap::new(),
    }
  }

  /// The YMAP and index of each item of another generation's YMAP the game loads that item
  /// `index` of kind `kind` of the vanilla YMAP `file_name` pairs with: of newer generations, or
  /// of older ones for a YMAP first shipped after the game build. `file_name` and the YMAPs are
  /// listed as their family lists them, a `distlodlights_*` YMAP for lights.
  pub(crate) fn loaded(
    &mut self,
    file_name: &str,
    kind: ItemKind,
    index: u32,
  ) -> Result<Vec<(String, u32)>> {
    let file = file_name.to_ascii_lowercase();
    let Some(located) = self.index.locate(&file) else {
      return Ok(Vec::new());
    };
    let backward = self.index.newer_than_build.contains(&file);
    let mut found = Vec::new();
    for unit in located
      .family
      .units
      .iter()
      .filter(|unit| located.generation == if backward { unit.newer } else { unit.older })
    {
      let key = unit.key.clone();
      self.load(&key)?;
      let (older, newer) = &self.files[&key];
      let (from, to, pairs) = if backward {
        (newer, older, &self.reversed[&key])
      } else {
        (older, newer, &self.units[&key])
      };
      let Some(from_file) = from.iter().position(|name| *name == file) else {
        continue;
      };
      if let Some(&(to_file, to_index)) =
        pairs.get(&kind).and_then(|pairs| pairs.get(&(from_file as u32, index)))
        && self.index.loads(&to[to_file as usize])
      {
        found.push((to[to_file as usize].clone(), to_index));
      }
    }
    Ok(found)
  }

  fn load(
    &mut self,
    key: &str,
  ) -> Result<()> {
    if self.units.contains_key(key) {
      return Ok(());
    }
    let unit = load_unit(self.cache_dir, key)?;
    let pairs = unit
      .pairs
      .iter()
      .map(|(kind, pairs)| {
        (*kind, pairs.iter().map(|&[of, oi, nf, ni]| ((of, oi), (nf, ni))).collect())
      })
      .collect();
    let reversed = unit
      .pairs
      .iter()
      .map(|(kind, pairs)| {
        (*kind, pairs.iter().map(|&[of, oi, nf, ni]| ((nf, ni), (of, oi))).collect())
      })
      .collect();
    let names = |files: &[UnitFile]| files.iter().map(|file| file.name.clone()).collect();
    self.files.insert(key.to_owned(), (names(&unit.older), names(&unit.newer)));
    self.units.insert(key.to_owned(), pairs);
    self.reversed.insert(key.to_owned(), reversed);
    Ok(())
  }
}

/// Where [`GenerationIndex::locate`] found a YMAP.
pub(crate) struct Located<'a> {
  pub family: &'a Family,
  pub generation: usize,
  /// Its index among the YMAPs of its generation.
  pub file: usize,
}

/// Reads the unit `key` of the vanilla cache at `cache_dir`.
pub(crate) fn load_unit(
  cache_dir: &Path,
  key: &str,
) -> Result<GenerationUnit> {
  let path = cache_dir.join(UNITS_DIR).join(format!("{key}.json"));
  let unit: GenerationUnit = crate::common::json::read_json(&path)
    .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
  if unit.algorithm_version != ALGORITHM_VERSION {
    return Err(format!("{} is of another algorithm version", path.display()).into());
  }
  Ok(unit)
}

/// Whether the cache at `output_dir` has the generation index of `revision` at `version`.
pub(super) fn index_is_current(
  output_dir: &Path,
  revision: &str,
  version: &str,
) -> bool {
  GenerationIndex::load(output_dir).ok().flatten().is_some_and(|index| {
    index.format_version == CACHE_SCHEMA
      && index.vanilla_manifest_sha256 == revision
      && index.version == version
  })
}

/// One latest vanilla YMAP the generations are formed from.
pub(super) struct SourceYmap {
  /// Its sha256.
  pub sha256: String,
  /// Its file in the vanilla archive.
  pub path: PathBuf,
  /// The index of the stage first shipping a YMAP of its name.
  pub first_stage: usize,
}

/// Writes the generation index and units of `ymaps` (keyed by lowercase file name), of which
/// `newer` are first shipped after `version`, into `staging`, keeping the units of `previous`
/// (the published cache) and building the rest; returns the index.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_generations(
  staging: &Path,
  previous: &Path,
  ymaps: &BTreeMap<String, SourceYmap>,
  inactive: &BTreeMap<String, String>,
  newer: &BTreeSet<String>,
  stage_ids: &[String],
  revision: &str,
  version: &str,
) -> Result<GenerationIndex> {
  let families = families(ymaps, stage_ids);
  let units_dir = staging.join(UNITS_DIR);
  fs::create_dir_all(&units_dir)?;
  keep_previous_units(&previous.join(UNITS_DIR), &units_dir)?;

  // The units to build, with the YMAPs they need.
  let mut missing = BTreeMap::new();
  let mut index = GenerationIndex {
    format_version: CACHE_SCHEMA,
    algorithm_version: ALGORITHM_VERSION,
    vanilla_manifest_sha256: revision.to_owned(),
    version: version.to_owned(),
    families: BTreeMap::new(),
    inactive: inactive.clone(),
    newer_than_build: newer.clone(),
  };
  for (name, generations) in families {
    let unit_files = |generation: &Generation| -> Vec<UnitFile> {
      generation
        .files
        .iter()
        .map(|file| UnitFile {
          name: file.clone(),
          sha256: ymaps[file].sha256.clone(),
          partner_sha256: partner(file)
            .and_then(|partner| Some(ymaps.get(&partner)?.sha256.clone())),
        })
        .collect()
    };
    let mut units = Vec::new();
    for (older, older_generation) in generations.iter().enumerate() {
      for (newer, newer_generation) in generations.iter().enumerate().skip(older + 1) {
        if older_generation.first_version == newer_generation.first_version {
          continue;
        }
        let (older_files, newer_files) =
          (unit_files(older_generation), unit_files(newer_generation));
        let key = unit_key(&name, &older_files, &newer_files)?;
        if !fs::is_file(units_dir.join(format!("{key}.json"))) {
          missing.insert(key.clone(), (name.clone(), older_files, newer_files));
        }
        units.push(UnitRef {
          older,
          newer,
          key,
        });
      }
    }
    index.families.insert(
      name,
      Family {
        generations,
        units,
        successors: BTreeMap::new(),
      },
    );
  }

  if !missing.is_empty() {
    tracing::info!("Pairing the items of {} vanilla YMAP generation units", missing.len());
    let needed = missing
      .values()
      .flat_map(|(_, older, newer)| older.iter().chain(newer))
      .map(|file| file.name.clone())
      .collect::<BTreeSet<_>>();
    let items = needed
      .par_iter()
      .map(|name| {
        let ymap = read_ymap(&ymaps[name].path).map_err(|error| format!("{name}: {error}"))?;
        let partner = partner(name)
          .and_then(|partner| ymaps.get(&partner))
          .map(|partner| read_ymap(&partner.path))
          .transpose()
          .map_err(|error| format!("{name} partner: {error}"))?;
        Ok((name.clone(), FileItems::of(&ymap, partner.as_ref())))
      })
      .collect::<std::result::Result<HashMap<_, _>, String>>()?;
    missing
      .par_iter()
      .map(|(key, (family, older, newer))| {
        let unit = build_unit(family, older, newer, &items);
        let path = units_dir.join(format!("{key}.json"));
        write_compact_json(&path, &unit).map_err(|error| format!("{}: {error}", path.display()))
      })
      .collect::<std::result::Result<Vec<_>, String>>()?;
  }
  for family in index.families.values_mut() {
    family.successors = successors(family, &units_dir, inactive, newer)?;
  }
  write_json(&staging.join(GENERATIONS_FILE), &index)?;
  Ok(index)
}

/// The successor of each YMAP of `family` whose items a newer generation holds, from the units
/// in `units_dir`; see [`Family::successors`].
fn successors(
  family: &Family,
  units_dir: &Path,
  inactive: &BTreeMap<String, String>,
  newer_than_build: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>> {
  // Per YMAP not loaded, the newest generation pairing its items, newer than it or, for one
  // first shipped after the game build, older, and how many each loaded YMAP there holds.
  let mut best = BTreeMap::<String, (Option<usize>, BTreeMap<String, usize>)>::new();
  let mut count = |from: &String, to: &String, generation: usize| {
    let (best_generation, counts) = best.entry(from.clone()).or_default();
    if best_generation.is_none_or(|best| generation > best) {
      *best_generation = Some(generation);
      counts.clear();
    }
    if *best_generation == Some(generation) {
      *counts.entry(to.clone()).or_default() += 1;
    }
  };
  for unit_ref in &family.units {
    let path = units_dir.join(format!("{}.json", unit_ref.key));
    let unit: GenerationUnit = crate::common::json::read_json(&path)
      .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    for &[older_file, _, newer_file, _] in unit.pairs.values().flatten() {
      let older = &unit.older[older_file as usize].name;
      let newer = &unit.newer[newer_file as usize].name;
      if newer_than_build.contains(newer) {
        if !inactive.contains_key(older) {
          count(newer, older, unit_ref.older);
        }
      } else if inactive.contains_key(older)
        && !newer_than_build.contains(older)
        && !inactive.contains_key(newer)
      {
        count(older, newer, unit_ref.newer);
      }
    }
  }
  Ok(
    best
      .into_iter()
      .filter_map(|(older, (_, counts))| {
        // The most items, then the first name.
        let successor = counts.into_iter().max_by(|(a, x), (b, y)| x.cmp(y).then(b.cmp(a)))?.0;
        Some((older, successor))
      })
      .collect(),
  )
}

/// Links (or else copies) every unit of `previous` into `staging`, used by this build or not, so
/// switching back to another game build finds its units.
fn keep_previous_units(
  previous: &Path,
  staging: &Path,
) -> Result<()> {
  let Ok(entries) = fs::read_dir(previous) else {
    return Ok(());
  };
  for entry in entries {
    let entry = entry?;
    let name = entry.file_name();
    if !name.to_string_lossy().ends_with(".json") || !entry.file_type()?.is_file() {
      continue;
    }
    let target = staging.join(&name);
    if fs::hard_link(entry.path(), &target).is_err() {
      fs::copy(entry.path(), &target)?;
    }
  }
  Ok(())
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

/// The `lodlights_*` partner of a `distlodlights_*` YMAP name.
fn partner(name: &str) -> Option<String> {
  (LightSide::of(name) == Some(LightSide::DistantLodLights))
    .then(|| LightSide::partner_name(name))
    .flatten()
}

fn unit_key(
  family: &str,
  older: &[UnitFile],
  newer: &[UnitFile],
) -> Result<String> {
  let bytes = serde_json::to_vec(&(ALGORITHM_VERSION, family, older, newer))?;
  Ok(hex::encode(Sha256::digest(bytes)))
}

/// The families of `ymaps` with more than one generation, each generation's YMAPs sorted and the
/// generations ordered by the stage first shipping one of their YMAPs; see the module docs.
fn families(
  ymaps: &BTreeMap<String, SourceYmap>,
  stage_ids: &[String],
) -> BTreeMap<String, Vec<Generation>> {
  let stems = ymaps.keys().map(|name| stem(name)).collect::<BTreeSet<_>>();
  let known = known_prefixes(&stems);
  let mut by_family = BTreeMap::<String, BTreeMap<String, (usize, Vec<String>)>>::new();
  for (name, ymap) in ymaps {
    if LightSide::of(name) == Some(LightSide::LodLights) && partner_of_lod(name, ymaps) {
      continue;
    }
    let (prefix, rest) = split_prefix(stem(name), &stems, &known);
    let family = rest.trim_end_matches(|c: char| c.is_ascii_digit()).to_owned();
    let generation = by_family.entry(family).or_default().entry(prefix.to_owned()).or_default();
    if generation.1.is_empty() || ymap.first_stage < generation.0 {
      generation.0 = ymap.first_stage;
    }
    generation.1.push(name.clone());
  }
  by_family
    .into_iter()
    .filter(|(_, generations)| generations.len() > 1)
    .map(|(family, generations)| {
      let mut generations = generations
        .into_iter()
        .map(|(prefix, (first_stage, mut files))| {
          files.sort();
          (first_stage, prefix, files)
        })
        .collect::<Vec<_>>();
      generations.sort();
      let generations = generations
        .into_iter()
        .map(|(first_stage, prefix, files)| Generation {
          prefix,
          first_version: stage_ids[first_stage].clone(),
          files,
        })
        .collect();
      (family, generations)
    })
    .collect()
}

/// Whether the `lodlights_*` YMAP `name` has its `distlodlights_*` partner in `ymaps`, which
/// stands for both.
fn partner_of_lod(
  name: &str,
  ymaps: &BTreeMap<String, SourceYmap>,
) -> bool {
  LightSide::partner_name(name).is_some_and(|partner| ymaps.contains_key(&partner))
}

fn stem(name: &str) -> &str {
  name.strip_suffix(".ymap").unwrap_or(name)
}

/// The prefix of the YMAP `stem` and the rest of its name, the prefix empty for unprefixed
/// YMAPs: the shortest prefix leaving another YMAP's stem, or else the shortest of `known`.
fn split_prefix<'a>(
  stem: &'a str,
  stems: &BTreeSet<&str>,
  known: &BTreeSet<String>,
) -> (&'a str, &'a str) {
  counterpart_split(stem, stems).unwrap_or_else(|| {
    underscores(stem)
      .find(|&at| known.contains(&stem[..at]))
      .map_or(("", stem), |at| (&stem[..at], &stem[at + 1..]))
  })
}

/// The shortest split of `stem` at an underscore leaving another YMAP's stem.
fn counterpart_split<'a>(
  stem: &'a str,
  stems: &BTreeSet<&str>,
) -> Option<(&'a str, &'a str)> {
  underscores(stem)
    .find(|&at| stems.contains(&stem[at + 1..]))
    .map(|at| (&stem[..at], &stem[at + 1..]))
}

/// The prefixes [`counterpart_split`] finds for at least [`MIN_PREFIX_FILES`] stems.
fn known_prefixes(stems: &BTreeSet<&str>) -> BTreeSet<String> {
  let mut counts = BTreeMap::<&str, usize>::new();
  for stem in stems {
    if let Some((prefix, _)) = counterpart_split(stem, stems) {
      *counts.entry(prefix).or_default() += 1;
    }
  }
  counts
    .into_iter()
    .filter(|(_, count)| *count >= MIN_PREFIX_FILES)
    .map(|(prefix, _)| prefix.to_owned())
    .collect()
}

/// The positions of the underscores of `stem` that have something on both sides.
fn underscores(stem: &str) -> impl Iterator<Item = usize> + '_ {
  stem.match_indices('_').map(|(at, _)| at).filter(move |&at| at > 0 && at + 1 < stem.len())
}

/// An item as units pair it: where it is and what identifies it beyond that.
#[derive(Debug, Clone, Copy)]
struct Item {
  position: [f32; 3],
  identity: u64,
  /// An entity's non-zero GUID, which pairs it before its position and `identity` do.
  guid: Option<u32>,
}

/// The items of one YMAP (with its LOD light partner) by kind.
#[derive(Debug, Default)]
struct FileItems(BTreeMap<ItemKind, Vec<Item>>);

impl FileItems {
  fn of(
    ymap: &Ymap,
    lod_partner: Option<&Ymap>,
  ) -> Self {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let identity = |value: &dyn Fn(&mut DefaultHasher)| {
      let mut hasher = DefaultHasher::new();
      value(&mut hasher);
      hasher.finish()
    };
    let position =
      |position: &crate::common::position::Position| [position.x, position.y, position.z];
    let mut items = BTreeMap::new();
    items.insert(
      ItemKind::Entities,
      ymap
        .entities
        .iter()
        .map(|entity| Item {
          position: position(&entity.position),
          identity: identity(&|hasher| entity.archetype_name.to_ascii_lowercase().hash(hasher)),
          guid: (entity.guid != 0).then_some(entity.guid),
        })
        .collect(),
    );
    let lod_lights = lod_partner.map(|partner| partner.lod_lights.as_slice()).unwrap_or_default();
    let paired = lod_lights.len() == ymap.distant_lod_lights.items.len();
    items.insert(
      ItemKind::Lights,
      ymap
        .distant_lod_lights
        .items
        .iter()
        .enumerate()
        .map(|(index, light)| Item {
          position: parse_position(&light.position),
          identity: identity(&|hasher| {
            light.rgbi.hash(hasher);
            // Lights of an unpaired partner are left out, as their indices do not line up.
            if paired {
              lod_lights[index].hash.hash(hasher);
            }
          }),
          guid: None,
        })
        .collect(),
    );
    items.insert(
      ItemKind::CarGenerators,
      ymap
        .car_generators
        .iter()
        .map(|generator| Item {
          position: position(&generator.position),
          identity: identity(&|hasher| generator.car_model.to_ascii_lowercase().hash(hasher)),
          guid: None,
        })
        .collect(),
    );
    items.insert(
      ItemKind::BoxOccluders,
      ymap
        .box_occluders
        .iter()
        .map(|occluder| Item {
          position: [
            occluder.i_center_x as f32,
            occluder.i_center_y as f32,
            occluder.i_center_z as f32,
          ],
          identity: identity(&|hasher| {
            (
              occluder.i_cos_z,
              occluder.i_sin_z,
              occluder.i_length,
              occluder.i_width,
              occluder.i_height,
            )
              .hash(hasher);
          }),
          guid: None,
        })
        .collect(),
    );
    items.insert(
      ItemKind::TimeCycleModifiers,
      ymap
        .time_cycle_modifiers
        .iter()
        .map(|modifier| Item {
          position: position(&modifier.min_extents),
          identity: identity(&|hasher| modifier.name.to_ascii_lowercase().hash(hasher)),
          guid: None,
        })
        .collect(),
    );
    Self(items)
  }
}

/// A distant LOD light position, `"x y z"`; unreadable ones are placed out of reach.
fn parse_position(text: &str) -> [f32; 3] {
  let mut values = text.split_whitespace().map(|value| value.parse::<f32>().unwrap_or(f32::NAN));
  [(); 3].map(|()| values.next().unwrap_or(f32::NAN))
}

fn build_unit(
  family: &str,
  older: &[UnitFile],
  newer: &[UnitFile],
  items: &HashMap<String, FileItems>,
) -> GenerationUnit {
  let mut unit = GenerationUnit {
    algorithm_version: ALGORITHM_VERSION,
    family: family.to_owned(),
    older: older.to_vec(),
    newer: newer.to_vec(),
    pairs: ByKind::default(),
    unpaired: ByKind::default(),
  };
  let collect = |files: &[UnitFile], kind: ItemKind| {
    files
      .iter()
      .enumerate()
      .flat_map(|(file, unit_file)| {
        items[&unit_file.name]
          .0
          .get(&kind)
          .into_iter()
          .flatten()
          .enumerate()
          .map(move |(index, item)| (file as u32, index as u32, *item))
      })
      .collect::<Vec<_>>()
  };
  for kind in ItemKind::ALL {
    let (before, after) = (collect(older, kind), collect(newer, kind));
    if before.is_empty() || after.is_empty() {
      continue;
    }
    let (pairs, counts) = pair_items(&before, &after);
    if !pairs.is_empty() {
      unit.pairs.insert(kind, pairs);
    }
    if counts != UnpairedCounts::default() {
      unit.unpaired.insert(kind, counts);
    }
  }
  unit
}

/// Pairs each item of `before` with the one item of `after` that fits it, when no other item of
/// `before` pairs with it too. An item with a GUID that items of `after` have fits the one of them,
/// or else the one of them within [`TOLERANCE`] of its identity; any other item fits the one of
/// `after` of its identity within [`TOLERANCE`].
fn pair_items(
  before: &[(u32, u32, Item)],
  after: &[(u32, u32, Item)],
) -> (Vec<[u32; 4]>, UnpairedCounts) {
  let cell = |item: &Item| item.position.map(|value| value.floor() as i64);
  let finite = |item: &Item| item.position.iter().all(|value| value.is_finite());
  let near = |item: &Item, other: &Item| {
    finite(other)
      && (0..3).all(|axis| (item.position[axis] - other.position[axis]).abs() <= TOLERANCE)
  };
  let mut grid = HashMap::<(u64, [i64; 3]), Vec<usize>>::new();
  let mut by_guid = HashMap::<u32, Vec<usize>>::new();
  for (index, (.., item)) in after.iter().enumerate() {
    if finite(item) {
      grid.entry((item.identity, cell(item))).or_default().push(index);
    }
    if let Some(guid) = item.guid {
      by_guid.entry(guid).or_default().push(index);
    }
  }
  let mut counts = UnpairedCounts::default();
  let mut chosen = Vec::new();
  let mut claims = HashMap::<usize, usize>::new();
  for (before_index, (.., item)) in before.iter().enumerate() {
    let same_guid = item.guid.and_then(|guid| by_guid.get(&guid));
    let found = match same_guid.map(Vec::as_slice) {
      Some(&[candidate]) => vec![candidate],
      Some(candidates) => candidates
        .iter()
        .copied()
        .filter(|&candidate| {
          let other = &after[candidate].2;
          other.identity == item.identity && finite(item) && near(item, other)
        })
        .collect(),
      None if !finite(item) => Vec::new(),
      None => {
        let [x, y, z] = cell(item);
        let mut found = Vec::new();
        for dx in -1..=1 {
          for dy in -1..=1 {
            for dz in -1..=1 {
              for &candidate in
                grid.get(&(item.identity, [x + dx, y + dy, z + dz])).into_iter().flatten()
              {
                if near(item, &after[candidate].2) {
                  found.push(candidate);
                }
              }
            }
          }
        }
        found
      }
    };
    match found[..] {
      [] => counts.missing += 1,
      [candidate] => {
        *claims.entry(candidate).or_default() += 1;
        chosen.push((before_index, candidate));
      }
      _ => counts.ambiguous += 1,
    }
  }
  let mut pairs = Vec::new();
  for (before_index, candidate) in chosen {
    if claims[&candidate] > 1 {
      counts.ambiguous += 1;
      continue;
    }
    let (older_file, older_index, _) = before[before_index];
    let (newer_file, newer_index, _) = after[candidate];
    pairs.push([older_file, older_index, newer_file, newer_index]);
  }
  (pairs, counts)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn source(first_stage: usize) -> SourceYmap {
    SourceYmap {
      sha256: String::new(),
      path: PathBuf::new(),
      first_stage,
    }
  }

  #[test]
  fn families_group_prefixed_generations_and_light_blocks() {
    let mut ymaps = BTreeMap::new();
    for (name, stage) in [
      ("distlodlights_small000.ymap", 0),
      ("lodlights_small000.ymap", 0),
      ("vw_distlodlights_small000.ymap", 2),
      ("vw_lodlights_small000.ymap", 2),
      ("cs1_01_strm_0.ymap", 0),
      ("hei_cs1_01_strm_0.ymap", 1),
      ("unrelated.ymap", 0),
    ] {
      ymaps.insert(name.to_owned(), source(stage));
    }
    // A prefix shipped for many YMAPs also covers new blocks without an unprefixed one.
    for index in 1..=MIN_PREFIX_FILES {
      ymaps.insert(format!("m25_2_x{index}.ymap"), source(3));
      ymaps.insert(format!("x{index}.ymap"), source(0));
    }
    ymaps.insert("m25_2_distlodlights_small063.ymap".into(), source(3));
    let stages = ["0000-base", "0001-heist", "0002-vinewood", "0003-m25"].map(String::from);
    let families = families(&ymaps, &stages);

    let lights = &families["distlodlights_small"];
    let summary =
      lights.iter().map(|it| (it.prefix.as_str(), it.files.clone())).collect::<Vec<_>>();
    assert_eq!(
      summary,
      [
        ("", vec!["distlodlights_small000.ymap".to_owned()]),
        ("vw", vec!["vw_distlodlights_small000.ymap".to_owned()]),
        ("m25_2", vec!["m25_2_distlodlights_small063.ymap".to_owned()]),
      ]
    );
    assert_eq!(lights[1].first_version, "0002-vinewood");
    let maps = &families["cs1_01_strm_"];
    assert_eq!(maps.iter().map(|it| it.prefix.as_str()).collect::<Vec<_>>(), ["", "hei"]);
    assert!(!families.keys().any(|family| family.starts_with("unrelated")));
  }

  #[test]
  fn items_pair_only_when_exactly_one_fits_each_way() {
    let item = |x: f32, identity: u64| Item {
      position: [x, 0.0, 0.0],
      identity,
      guid: None,
    };
    let before = [
      (0, 0, item(1.0, 1)),
      (0, 1, item(5.0, 1)),
      (0, 2, item(9.0, 1)),
      (1, 0, item(20.0, 1)),
      (1, 1, item(20.0, 1)),
    ];
    let after =
      [(0, 3, item(9.0, 1)), (0, 0, item(1.005, 1)), (0, 1, item(5.0, 2)), (1, 0, item(20.0, 1))];
    let (pairs, counts) = pair_items(&before, &after);
    assert_eq!(pairs, [[0, 0, 0, 0], [0, 2, 0, 3]]);
    // Item 1 has another identity there; both items at x = 20 claim the same one.
    assert_eq!(
      counts,
      UnpairedCounts {
        missing: 1,
        ambiguous: 2,
      }
    );
  }

  #[test]
  fn entities_pair_by_guid_before_position_and_archetype() {
    let entity = |x: f32, identity: u64, guid: u32| Item {
      position: [x, 0.0, 0.0],
      identity,
      guid: Some(guid),
    };
    let before = [
      (0, 0, entity(1.0, 1, 10)),
      (0, 1, entity(5.0, 2, 11)),
      (0, 2, entity(9.0, 3, 12)),
      (0, 3, entity(13.0, 4, 13)),
      (0, 4, entity(17.0, 5, 14)),
    ];
    let after = [
      // Renamed under its GUID.
      (0, 0, entity(1.0, 6, 10)),
      // Moved under its GUID.
      (0, 1, entity(50.0, 2, 11)),
      // Two with its GUID: the one at its position with its archetype fits.
      (0, 2, entity(9.0, 7, 12)),
      (0, 3, entity(9.0, 3, 12)),
      // A new GUID at its position with its archetype.
      (0, 4, entity(13.0, 4, 99)),
      // A new GUID at its position with another archetype.
      (0, 5, entity(17.0, 8, 98)),
    ];
    let (pairs, counts) = pair_items(&before, &after);
    assert_eq!(pairs, [[0, 0, 0, 0], [0, 1, 0, 1], [0, 2, 0, 3], [0, 3, 0, 4]]);
    assert_eq!(
      counts,
      UnpairedCounts {
        missing: 1,
        ambiguous: 0,
      }
    );
  }
}
