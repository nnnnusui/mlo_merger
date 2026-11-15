//! Derived clothing counts per vanilla stage: how many drawables, textures and props each
//! stage's `CPedVariationInfo` YMTs define.
//!
//! Each DLC ships its clothing in its own YMTs (freemode Peds get one per DLC), so a stage's
//! counts are that DLC's clothing. A replaced file is counted whole at the stage replacing it.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::json::{read_json, write_json};
use crate::core::format::gamefile::{
  meta::{MetaResource, jenk_hash},
  resource::Resource,
  struct_reader::{StructReader, field},
};
use crate::core::vanilla::load_manifest_with_revision;
use crate::platform::{fs, process};

#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Bump when the counted fields or the cache layout change.
const CACHE_SCHEMA: u32 = 1;

/// Component slots in `availComp` order, as the game numbers them.
const COMPONENTS: [&str; 12] =
  ["HEAD", "BERD", "HAIR", "UPPR", "LOWR", "HAND", "FEET", "TEEF", "ACCS", "TASK", "DECL", "JBIB"];

/// Prop anchor names of `eAnchorPoints`, resolved by hash when anchors are counted.
const ANCHORS: [&str; 14] = [
  "ANCHOR_HEAD",
  "ANCHOR_EYES",
  "ANCHOR_EARS",
  "ANCHOR_MOUTH",
  "ANCHOR_LEFT_HAND",
  "ANCHOR_RIGHT_HAND",
  "ANCHOR_LEFT_WRIST",
  "ANCHOR_RIGHT_WRIST",
  "ANCHOR_HIP",
  "ANCHOR_LEFT_FOOT",
  "ANCHOR_RIGHT_FOOT",
  "ANCHOR_PH_L_HAND",
  "ANCHOR_PH_R_HAND",
  "NUM_ANCHORS",
];

/// Builds the clothing-counts cache from a clothing archive.
#[derive(Debug, Clone)]
pub struct BuildClothingCache {
  /// Clothing archive directory, normally `asset/vanilla-clothing`.
  pub clothing_dir: PathBuf,
  /// Output directory, normally `asset/vanilla-clothing-cache`.
  pub output_dir: PathBuf,
  /// Rebuilds even when the recorded archive digest is current.
  pub force: bool,
}

/// Freshness record tying the cache to the clothing archive it was counted from.
#[derive(Debug, Serialize, Deserialize)]
struct ClothingCacheInfo {
  format_version: u32,
  clothing_manifest_sha256: String,
  latest_version: String,
}

/// Counts summed over one file or one stage.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
  pub drawables: usize,
  pub textures: usize,
  pub props: usize,
  pub prop_textures: usize,
}

/// One component's drawable and texture counts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentCounts {
  pub drawables: usize,
  pub textures: usize,
}

/// One YMT's clothing counts.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCounts {
  /// The root structure's name, such as `mp_m_freemode_01`; PSO files record none.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ped_name: Option<String>,
  /// Drawable and texture counts by component slot (`HEAD`, `UPPR`, …).
  pub components: BTreeMap<String, ComponentCounts>,
  /// Prop counts by anchor (`ANCHOR_HEAD`, …).
  pub props: BTreeMap<String, usize>,
  pub totals: Totals,
}

/// One stage's files and their summed counts.
#[derive(Debug, Serialize, Deserialize)]
struct VersionCounts {
  files: BTreeMap<String, FileCounts>,
  totals: Totals,
}

/// One stage's row in `summary.json`.
#[derive(Debug, Serialize)]
struct SummaryRow {
  version: String,
  files: usize,
  drawables: usize,
  textures: usize,
  props: usize,
  prop_textures: usize,
}

impl BuildClothingCache {
  /// Counts every stage's changed YMTs and publishes the cache, reusing a current one.
  ///
  /// ```no_run
  /// mlo_merger::core::clothing::cache::BuildClothingCache {
  ///   clothing_dir: "asset/vanilla-clothing".into(),
  ///   output_dir: "asset/vanilla-clothing-cache".into(),
  ///   force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<()> {
    let (manifest, revision) = load_manifest_with_revision(&self.clothing_dir)?;
    let latest_version =
      manifest.versions.last().ok_or("Clothing archive has no stages")?.id.clone();
    let output = prepare_output_path(&self.output_dir, &self.clothing_dir)?;
    if !self.force
      && let Ok(existing) = read_json::<ClothingCacheInfo>(output.join("cache_info.json"))
      && existing.format_version == CACHE_SCHEMA
      && existing.clothing_manifest_sha256 == revision
    {
      tracing::info!("Clothing counts cache is current through {}", existing.latest_version);
      return Ok(());
    }
    let names = anchor_names();
    let mut summary = Vec::new();
    let staging = create_staging_directory(&output)?;
    let staging = Staging(staging);
    fs::create_dir(staging.0.join("counts"))?;
    for version in &manifest.versions {
      let files = version
        .changes
        .par_iter()
        .map(|(name, change)| {
          let path = self.clothing_dir.join(&change.file.object);
          let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
          let counts = count_file(&bytes, &names)
            .map_err(|error| format!("{}: {error}", change.file.object))?;
          Ok((name.clone(), counts))
        })
        .collect::<std::result::Result<BTreeMap<_, _>, String>>()?;
      let totals = files.values().fold(Totals::default(), |sum, counts| sum + counts.totals);
      summary.push(SummaryRow {
        version: version.id.clone(),
        files: files.len(),
        drawables: totals.drawables,
        textures: totals.textures,
        props: totals.props,
        prop_textures: totals.prop_textures,
      });
      if !files.is_empty() {
        write_json(
          &staging.0.join("counts").join(format!("{}.json", version.id)),
          &VersionCounts {
            files,
            totals,
          },
        )?;
      }
    }
    write_json(&staging.0.join("summary.json"), &summary)?;
    write_json(
      &staging.0.join("cache_info.json"),
      &ClothingCacheInfo {
        format_version: CACHE_SCHEMA,
        clothing_manifest_sha256: revision,
        latest_version: latest_version.clone(),
      },
    )?;
    publish_directory(&staging.0, &output)?;
    tracing::info!(
      "Counted clothing for {} stages through {latest_version} into {}",
      manifest.versions.len(),
      output.display()
    );
    Ok(())
  }
}

impl std::ops::Add for Totals {
  type Output = Self;

  fn add(
    self,
    other: Self,
  ) -> Self {
    Self {
      drawables: self.drawables + other.drawables,
      textures: self.textures + other.textures,
      props: self.props + other.props,
      prop_textures: self.prop_textures + other.prop_textures,
    }
  }
}

/// `eAnchorPoints` entry names by hash, since clothing YMTs embed no name strings for them.
fn anchor_names() -> HashMap<u32, String> {
  ANCHORS.iter().map(|name| (jenk_hash(name), name.to_string())).collect()
}

/// Counts one YMT's `CPedVariationInfo`, whichever container holds it.
pub fn count_file(
  bytes: &[u8],
  shared_names: &HashMap<u32, String>,
) -> io::Result<FileCounts> {
  match Resource::decode(bytes)? {
    Resource::Rsc7(resource) => {
      let meta = MetaResource::parse(&resource)?;
      let names = meta.names(shared_names);
      let mut counts = count_root(&meta.root_struct(&names)?)?;
      counts.ped_name = meta.name.clone();
      Ok(counts)
    }
    Resource::Pso(resource) => {
      let names = resource.names(shared_names);
      count_root(&resource.root_struct(&names)?)
    }
  }
}

/// Counts a `CPedVariationInfo` root: `availComp` names each `aComponentData3` entry's slot.
fn count_root<R: StructReader>(root: &R) -> io::Result<FileCounts> {
  let available: Vec<u8> = root.list(field!("availComp"))?;
  let mut counts = FileCounts::default();
  for (index, component) in root.children(field!("aComponentData3"))?.iter().enumerate() {
    let name = available
      .iter()
      .position(|slot| *slot as usize == index)
      .and_then(|slot| COMPONENTS.get(slot))
      .map_or_else(|| format!("component_{index}"), |name| (*name).to_string());
    let drawables = component.children(field!("aDrawblData3"))?;
    let mut textures = 0;
    for drawable in &drawables {
      textures += drawable.children(field!("aTexData"))?.len();
    }
    counts.totals.drawables += drawables.len();
    counts.totals.textures += textures;
    counts.components.insert(
      name,
      ComponentCounts {
        drawables: drawables.len(),
        textures,
      },
    );
  }
  if let Some(info) = root.child(field!("propInfo"))? {
    for prop in info.children(field!("aPropMetaData"))? {
      counts.totals.props += 1;
      counts.totals.prop_textures += prop.children(field!("texData"))?.len();
    }
    for anchor in info.children(field!("aAnchors"))? {
      let props = anchor.list::<i64>(field!("props"))?.len();
      *counts.props.entry(anchor.get_or_default::<String>(field!("anchor"))?).or_default() += props;
    }
  }
  Ok(counts)
}

struct Staging(PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

fn prepare_output_path(
  output: &Path,
  clothing_dir: &Path,
) -> Result<PathBuf> {
  let absolute =
    if output.is_absolute() { output.to_path_buf() } else { process::current_dir()?.join(output) };
  let current_dir = fs::canonicalize(&process::current_dir()?)?;
  let clothing_dir = fs::canonicalize(clothing_dir)?;
  if absolute == current_dir
    || current_dir.starts_with(&absolute)
    || absolute.starts_with(&clothing_dir)
    || clothing_dir.starts_with(&absolute)
  {
    return Err("Clothing-cache output must not overlap the workspace or clothing archive".into());
  }
  if fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err(format!("Refusing to replace symlink output {}", absolute.display()).into());
  }
  if fs::exists(&absolute).unwrap_or(false) && !fs::is_dir(&absolute) {
    return Err(format!("Clothing-cache output is not a directory: {}", absolute.display()).into());
  }
  let parent = absolute.parent().ok_or("Clothing-cache output has no parent")?;
  fs::create_dir_all(parent)?;
  let parent = fs::canonicalize(parent)?;
  Ok(parent.join(absolute.file_name().ok_or("Clothing-cache output has no name")?))
}

fn create_staging_directory(output: &Path) -> Result<PathBuf> {
  let parent = output.parent().ok_or("Clothing-cache output has no parent")?;
  let name = output.file_name().ok_or("Clothing-cache output has no name")?.to_string_lossy();
  let staging = parent.join(format!(".{name}.staging-{}", process::id()));
  fs::create_dir(&staging)?;
  Ok(staging)
}

fn publish_directory(
  staging: &Path,
  output: &Path,
) -> Result<()> {
  let backup = output.with_file_name(format!(
    ".{}.backup-{}",
    output.file_name().unwrap_or_default().to_string_lossy(),
    process::id()
  ));
  let existed = fs::exists(output).unwrap_or(false);
  if existed {
    fs::rename(output, &backup)?;
  }
  if let Err(error) = fs::rename(staging, output) {
    if existed {
      fs::rename(&backup, output)?;
    }
    return Err(error.into());
  }
  if existed {
    fs::remove_dir_all(backup)?;
  }
  Ok(())
}
