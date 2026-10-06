//! Derived latest-file and YMAP relationship indexes for raw vanilla history.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::{BufReader, BufWriter, Write},
  path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use crate::core::vanilla::VanillaCacheManifest;
use crate::core::{
  format::gamefile::{
    meta_resource::{MetaResource, jenk_hash},
    resource_file::Rsc7Resource,
  },
  format::ymap::diff::reference_hash,
  vanilla::{CachedFile, read_ymap},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const CACHE_SCHEMA: u32 = 2;

struct Staging(PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

/// Builds a derived cache from versioned native files in the vanilla archive.
#[derive(Debug, Clone)]
pub struct BuildVanillaCache {
  /// Directory containing the raw vanilla cache manifest and stage artifacts.
  pub vanilla_dir: PathBuf,
  /// Destination for latest files, relationship indexes and derived metadata.
  pub output_dir: PathBuf,
  /// Optional final vanilla stage ID, stage label, or numeric stage position.
  pub through_version: Option<String>,
  /// Rebuilds this stage even when its inputs and outputs are current.
  pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DerivedManifest {
  format_version: u32,
  vanilla_manifest_sha256: String,
  latest_version: String,
  vanilla_input_timestamps: BTreeMap<String, SourceTimestamp>,
  files: BTreeMap<String, LatestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourceTimestamp {
  modified_seconds: u64,
  modified_nanos: u32,
  size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LatestFile {
  version: String,
  sha256: String,
  object: String,
  vanilla_object: String,
  source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YmapRelationshipIndex {
  format_version: u32,
  vanilla_manifest_sha256: String,
  version: String,
  children_by_parent_hash: BTreeMap<String, Vec<String>>,
}

impl BuildVanillaCache {
  /// Builds the requested vanilla-cache prefix and publishes it atomically.
  pub fn run(&self) -> Result<()> {
    let vanilla_dir = self.vanilla_dir.canonicalize()?;
    let manifest_path = vanilla_dir.join("cache_info.json");
    let manifest_bytes = fs::read(&manifest_path)?;
    let manifest: VanillaCacheManifest =
      serde_json::from_reader(BufReader::new(manifest_bytes.as_slice()))?;
    if manifest.format_version != 1 {
      return Err(format!("Unsupported vanilla archive schema {}", manifest.format_version).into());
    }
    let through_index = resolve_through_version(&manifest, self.through_version.as_deref())?;
    let latest_version = manifest.versions[through_index].id.clone();
    let revision = format!("{:x}", Sha256::digest(&manifest_bytes));
    let current = resolve_files(&manifest, through_index)?;
    let input_timestamps = source_timestamps(&vanilla_dir, &current)?;
    let output_dir = prepare_output_path(&self.output_dir, &vanilla_dir)?;

    if !self.force
      && cache_is_current(&output_dir, &vanilla_dir, &revision, &latest_version, &input_timestamps)?
    {
      log::info!("Vanilla derived cache is current through {latest_version}");
      return Ok(());
    }

    let staging = Staging(create_staging_directory(&output_dir)?);
    Self::build(
      &vanilla_dir,
      &staging.0,
      &output_dir,
      &latest_version,
      &revision,
      &current,
      &input_timestamps,
    )?;
    publish_directory(&staging.0, &output_dir)?;
    log::info!("Built vanilla derived cache through {latest_version}");
    Ok(())
  }

  fn build(
    vanilla_dir: &Path,
    staging: &Path,
    output_dir: &Path,
    latest_version: &str,
    revision: &str,
    current: &BTreeMap<String, (String, CachedFile)>,
    input_timestamps: &BTreeMap<String, SourceTimestamp>,
  ) -> Result<()> {
    let latest_dir = staging.join("latest");
    fs::create_dir_all(latest_dir.join("ymap"))?;
    fs::create_dir_all(latest_dir.join("ybn"))?;
    let mut latest_files = BTreeMap::new();
    let mut parent_by_child = BTreeMap::new();
    for (name, (version, file)) in current {
      let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
      if extension != "ymap" && extension != "ybn" {
        continue;
      }
      let source = existing_artifact_path(vanilla_dir, file)?;
      if extension == "ymap" {
        parent_by_child.insert(name.to_ascii_lowercase(), ymap_parent_hash(&source)?);
      }
      let destination_relative = format!("latest/{extension}/{name}");
      let destination = staging.join(&destination_relative);
      let final_destination = output_dir.join(&destination_relative);
      let link_target = relative_link_target(
        final_destination.parent().ok_or("Latest output has no parent")?,
        &source,
      );
      create_file_symlink(&link_target, &destination)?;
      latest_files.insert(
        name.clone(),
        LatestFile {
          version: version.clone(),
          sha256: file.sha256.clone(),
          object: destination_relative,
          vanilla_object: file.object.clone(),
          source: file.source.clone(),
        },
      );
    }

    write_json(
      &staging.join("cache_info.json"),
      &DerivedManifest {
        format_version: CACHE_SCHEMA,
        vanilla_manifest_sha256: revision.to_owned(),
        latest_version: latest_version.to_owned(),
        vanilla_input_timestamps: input_timestamps.clone(),
        files: latest_files,
      },
    )?;
    write_json(
      &staging.join("ymap_relationships.json"),
      &YmapRelationshipIndex {
        format_version: CACHE_SCHEMA,
        vanilla_manifest_sha256: revision.to_owned(),
        version: latest_version.to_owned(),
        children_by_parent_hash: children_by_parent_hash(&parent_by_child),
      },
    )?;
    Ok(())
  }
}

fn children_by_parent_hash(
  parent_by_child: &BTreeMap<String, u32>
) -> BTreeMap<String, Vec<String>> {
  let mut children = BTreeMap::new();
  for (child, parent_hash) in parent_by_child {
    if *parent_hash != 0 {
      children.entry(format!("{parent_hash:08x}")).or_insert_with(Vec::new).push(child.clone());
    }
  }
  children
}

fn ymap_parent_hash(path: &Path) -> Result<u32> {
  let bytes = fs::read(path)?;
  if !bytes.starts_with(b"RSC7") {
    let ymap = read_ymap(path)?;
    return Ok(reference_hash(&ymap.parent));
  }
  let resource = Rsc7Resource::decode(&bytes)?;
  let meta = MetaResource::parse(&resource)?;
  let root_index = usize::try_from(meta.root_block_index - 1)?;
  let root = meta.data_blocks.get(root_index).ok_or("YMAP META root block is missing")?;
  let schema = meta
    .structures
    .iter()
    .find(|schema| schema.name_hash == root.structure_name_hash)
    .ok_or("YMAP META root schema is missing")?;
  let parent_name_hash = jenk_hash("parent");
  let field = schema
    .entries
    .iter()
    .find(|field| field.name_hash == parent_name_hash)
    .ok_or("YMAP META root has no parent field")?;
  let offset = field.data_offset as usize;
  match field.data_type {
    0x4a | 0x14 | 0x15 => {
      let bytes = root.data.get(offset..offset + 4).ok_or("YMAP parent field is truncated")?;
      Ok(u32::from_le_bytes(bytes.try_into()?))
    }
    0x44 => {
      let pointer = root.data.get(offset..offset + 10).ok_or("YMAP parent pointer is truncated")?;
      let address = u64::from_le_bytes(pointer[..8].try_into()?);
      let block_id = (address & 0xfff) as usize;
      if block_id == 0 {
        return Ok(0);
      }
      let string_offset = ((address >> 12) & 0xfffff) as usize;
      let length = u16::from_le_bytes(pointer[8..10].try_into()?) as usize;
      let block = meta
        .data_blocks
        .get(block_id - 1)
        .ok_or("YMAP parent string points to a missing META block")?;
      let bytes = block
        .data
        .get(string_offset..string_offset + length)
        .ok_or("YMAP parent string is truncated")?;
      let parent = String::from_utf8_lossy(bytes).trim_end_matches('\0').to_owned();
      Ok(reference_hash(&parent))
    }
    0x40 => {
      let length = field.reference_key as usize;
      let bytes = root.data.get(offset..offset + length).ok_or("YMAP parent text is truncated")?;
      let parent = String::from_utf8_lossy(bytes).trim_end_matches('\0').to_owned();
      Ok(reference_hash(&parent))
    }
    data_type => Err(format!("Unsupported YMAP parent field type 0x{data_type:02x}").into()),
  }
}

fn resolve_through_version(
  manifest: &VanillaCacheManifest,
  requested: Option<&str>,
) -> Result<usize> {
  if manifest.versions.is_empty() {
    return Err("Vanilla archive has no stages".into());
  }
  let Some(requested) = requested.map(str::trim).filter(|requested| !requested.is_empty()) else {
    return Ok(manifest.versions.len() - 1);
  };
  let requested = requested.to_ascii_lowercase();
  let position = manifest.versions.iter().position(|version| {
    version.id.eq_ignore_ascii_case(&requested)
      || version.id.split_once('-').is_some_and(|(_, label)| label.eq_ignore_ascii_case(&requested))
      || requested.parse::<usize>().ok().is_some_and(|number| {
        version.id.split_once('-').and_then(|(index, _)| index.parse::<usize>().ok())
          == Some(number)
      })
  });
  position
    .ok_or_else(|| format!("Unknown vanilla stage or unsupported gamebuild: {requested}").into())
}

fn resolve_files(
  manifest: &VanillaCacheManifest,
  through_index: usize,
) -> Result<BTreeMap<String, (String, CachedFile)>> {
  let mut files: BTreeMap<String, (String, CachedFile)> = BTreeMap::new();
  let mut previous_version: Option<&str> = None;
  let mut seen_versions = std::collections::HashSet::new();
  for version in manifest.versions.iter().take(through_index + 1) {
    if version.parent.as_deref() != previous_version || !seen_versions.insert(&version.id) {
      return Err(format!("Invalid vanilla stage chain at {}", version.id).into());
    }
    previous_version = Some(&version.id);
    let mut names_in_stage = std::collections::HashSet::new();
    for (name, change) in &version.changes {
      let normalized = name.to_ascii_lowercase();
      if !names_in_stage.insert(normalized.clone()) {
        return Err(format!("Duplicate vanilla filename {name} at {}", version.id).into());
      }
      match files.get(&normalized) {
        Some((_, previous))
          if change.previous_sha256.as_deref() != Some(previous.sha256.as_str()) =>
        {
          return Err(format!("Invalid vanilla content chain for {name} at {}", version.id).into());
        }
        None if change.previous_sha256.is_some() => {
          return Err(format!("Unexpected previous hash for {name} at {}", version.id).into());
        }
        _ => {}
      }
      files.insert(normalized, (version.id.clone(), change.file.clone()));
    }
  }
  Ok(files)
}

fn source_timestamps(
  vanilla_dir: &Path,
  current: &BTreeMap<String, (String, CachedFile)>,
) -> Result<BTreeMap<String, SourceTimestamp>> {
  let mut paths = BTreeSet::from(["cache_info.json".to_owned()]);
  for (name, (_, file)) in current {
    if is_supported_stream_file(name) {
      paths.insert(file.object.clone());
    }
  }
  paths
    .into_iter()
    .map(|relative| {
      let path = checked_source_path(vanilla_dir, &relative)?;
      let metadata = fs::metadata(path)?;
      let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
      Ok((
        relative,
        SourceTimestamp {
          modified_seconds: modified.as_secs(),
          modified_nanos: modified.subsec_nanos(),
          size: metadata.len(),
        },
      ))
    })
    .collect()
}

fn is_supported_stream_file(name: &str) -> bool {
  let extension =
    Path::new(name).extension().and_then(|extension| extension.to_str()).unwrap_or_default();
  extension.eq_ignore_ascii_case("ymap") || extension.eq_ignore_ascii_case("ybn")
}

fn existing_artifact_path(
  vanilla_dir: &Path,
  file: &CachedFile,
) -> Result<PathBuf> {
  let path = checked_source_path(vanilla_dir, &file.object)?;
  if !path.is_file() {
    return Err(format!("Vanilla artifact is missing: {}", path.display()).into());
  }
  Ok(path)
}

fn checked_source_path(
  vanilla_dir: &Path,
  object: &str,
) -> Result<PathBuf> {
  let relative = Path::new(object);
  if object.is_empty()
    || object.contains(['\\', ':'])
    || relative.components().any(|component| !matches!(component, Component::Normal(_)))
  {
    return Err(format!("Invalid vanilla artifact path: {object}").into());
  }
  Ok(vanilla_dir.join(relative))
}

fn cache_is_current(
  output_dir: &Path,
  vanilla_dir: &Path,
  revision: &str,
  latest_version: &str,
  input_timestamps: &BTreeMap<String, SourceTimestamp>,
) -> Result<bool> {
  let manifest_path = output_dir.join("cache_info.json");
  let relationships_path = output_dir.join("ymap_relationships.json");
  if !manifest_path.is_file() || !relationships_path.is_file() {
    return Ok(false);
  }
  let Ok(manifest_file) = fs::File::open(manifest_path) else {
    return Ok(false);
  };
  let Ok(manifest) = serde_json::from_reader::<_, DerivedManifest>(BufReader::new(manifest_file))
  else {
    return Ok(false);
  };
  if manifest.format_version != CACHE_SCHEMA
    || manifest.vanilla_manifest_sha256 != revision
    || manifest.latest_version != latest_version
    || manifest.vanilla_input_timestamps != *input_timestamps
  {
    return Ok(false);
  }
  let Ok(relationships_file) = fs::File::open(relationships_path) else {
    return Ok(false);
  };
  let Ok(relationships) =
    serde_json::from_reader::<_, YmapRelationshipIndex>(BufReader::new(relationships_file))
  else {
    return Ok(false);
  };
  if relationships.format_version != CACHE_SCHEMA
    || relationships.vanilla_manifest_sha256 != revision
    || relationships.version != latest_version
  {
    return Ok(false);
  }
  if !output_dir.join("latest/ymap").is_dir() || !output_dir.join("latest/ybn").is_dir() {
    return Ok(false);
  }
  for file in manifest.files.values() {
    let link = output_dir.join(&file.object);
    if !fs::symlink_metadata(&link).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Ok(false);
    }
    let source = checked_source_path(vanilla_dir, &file.vanilla_object)?;
    let expected_target =
      relative_link_target(link.parent().ok_or("Latest symlink has no parent")?, &source);
    if fs::read_link(link)? != expected_target {
      return Ok(false);
    }
  }
  Ok(true)
}

fn relative_link_target(
  link_parent: &Path,
  source: &Path,
) -> PathBuf {
  let from: Vec<_> = link_parent.components().collect();
  let to: Vec<_> = source.components().collect();
  let common = from.iter().zip(&to).take_while(|(left, right)| left == right).count();
  if common == 0 {
    return source.to_path_buf();
  }
  let mut relative = PathBuf::new();
  for component in from.iter().skip(common) {
    if matches!(component, Component::Normal(_)) {
      relative.push("..");
    }
  }
  for component in to.iter().skip(common) {
    relative.push(component.as_os_str());
  }
  relative
}

#[cfg(unix)]
fn create_file_symlink(
  target: &Path,
  link: &Path,
) -> Result<()> {
  std::os::unix::fs::symlink(target, link)?;
  Ok(())
}

#[cfg(windows)]
fn create_file_symlink(
  target: &Path,
  link: &Path,
) -> Result<()> {
  std::os::windows::fs::symlink_file(target, link)?;
  Ok(())
}

fn prepare_output_path(
  output: &Path,
  vanilla_dir: &Path,
) -> Result<PathBuf> {
  let absolute =
    if output.is_absolute() { output.to_path_buf() } else { std::env::current_dir()?.join(output) };
  let current_dir = std::env::current_dir()?.canonicalize()?;
  if absolute == current_dir
    || current_dir.starts_with(&absolute)
    || absolute.starts_with(vanilla_dir)
    || vanilla_dir.starts_with(&absolute)
  {
    return Err("Vanilla-cache output must not overlap the workspace or vanilla input".into());
  }
  if fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err(format!("Refusing to replace symlink output {}", absolute.display()).into());
  }
  if absolute.exists() && !absolute.is_dir() {
    return Err(format!("Vanilla-cache output is not a directory: {}", absolute.display()).into());
  }
  let parent = absolute.parent().ok_or("Vanilla-cache output has no parent")?;
  fs::create_dir_all(parent)?;
  let parent = parent.canonicalize()?;
  Ok(parent.join(absolute.file_name().ok_or("Vanilla-cache output has no name")?))
}

fn create_staging_directory(output_dir: &Path) -> Result<PathBuf> {
  let parent = output_dir.parent().ok_or("Vanilla-cache output has no parent")?;
  let name = output_dir.file_name().ok_or("Vanilla-cache output has no name")?.to_string_lossy();
  let staging = parent.join(format!(".{name}.staging-{}", std::process::id()));
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
    std::process::id()
  ));
  let existed = output.exists();
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

fn write_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let mut writer = BufWriter::new(fs::File::create(path)?);
  serde_json::to_writer_pretty(&mut writer, value)?;
  writer.flush()?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    format::gamefile::meta_resource::{MetaDataBlock, MetaStructureEntry, MetaStructureInfo},
    vanilla::{CacheVersion, FileChange, read_ymap, write_json},
  };
  use std::collections::BTreeMap;

  #[test]
  fn builds_latest_files_through_selected_stage_and_reuses_valid_output() {
    let root = std::env::temp_dir().join(format!("derived_vanilla_cache_{}", std::process::id()));
    let vanilla = root.join("vanilla");
    let output = root.join("cache");
    fs::create_dir_all(vanilla.join("0000-base/ybn")).unwrap();
    fs::create_dir_all(vanilla.join("0001-patch/ybn")).unwrap();
    fs::write(vanilla.join("0000-base/ybn/map.ybn"), b"base").unwrap();
    fs::write(vanilla.join("0001-patch/ybn/map.ybn"), b"patch").unwrap();
    let artifact = |stage: &str, content: &[u8]| CachedFile {
      sha256: format!("{:x}", Sha256::digest(content)),
      object: format!("{stage}/ybn/map.ybn"),
      source: format!("{stage}.rpf/map.ybn"),
    };
    let manifest = VanillaCacheManifest {
      format_version: 1,
      game_dir: "game".into(),
      versions: vec![
        CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          changes: BTreeMap::from([(
            "map.ybn".into(),
            FileChange {
              previous_sha256: None,
              file: artifact("0000-base", b"base"),
            },
          )]),
          unchanged: 0,
        },
        CacheVersion {
          id: "0001-patch".into(),
          parent: Some("0000-base".into()),
          archives: vec![],
          changes: BTreeMap::from([(
            "map.ybn".into(),
            FileChange {
              previous_sha256: Some(format!("{:x}", Sha256::digest(b"base"))),
              file: artifact("0001-patch", b"patch"),
            },
          )]),
          unchanged: 0,
        },
      ],
    };
    write_json(&vanilla.join("cache_info.json"), &manifest).unwrap();

    BuildVanillaCache {
      vanilla_dir: vanilla.clone(),
      output_dir: output.clone(),
      through_version: Some("base".into()),
      force: false,
    }
    .run()
    .unwrap();
    assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"base");
    let relationships: YmapRelationshipIndex =
      serde_json::from_reader(fs::File::open(output.join("ymap_relationships.json")).unwrap())
        .unwrap();
    assert_eq!(relationships.version, "0000-base");
    assert!(relationships.children_by_parent_hash.is_empty());
    let latest_link = output.join("latest/ybn/map.ybn");
    assert!(fs::symlink_metadata(&latest_link).unwrap().file_type().is_symlink());
    let target = fs::read_link(&latest_link).unwrap();
    assert!(!target.is_absolute());
    assert_eq!(
      latest_link.parent().unwrap().join(target).canonicalize().unwrap(),
      vanilla.join("0000-base/ybn/map.ybn").canonicalize().unwrap()
    );
    let derived: DerivedManifest =
      serde_json::from_reader(fs::File::open(output.join("cache_info.json")).unwrap()).unwrap();
    assert_eq!(derived.latest_version, "0000-base");
    let timestamp_key = "0000-base/ybn/map.ybn";
    let source_file = vanilla.join(timestamp_key);
    let changed_time =
      fs::metadata(&source_file).unwrap().modified().unwrap() + std::time::Duration::from_secs(2);
    fs::File::options()
      .write(true)
      .open(&source_file)
      .unwrap()
      .set_times(fs::FileTimes::new().set_modified(changed_time))
      .unwrap();

    BuildVanillaCache {
      vanilla_dir: root.join("vanilla"),
      output_dir: output.clone(),
      through_version: Some("base".into()),
      force: false,
    }
    .run()
    .unwrap();
    let refreshed: DerivedManifest =
      serde_json::from_reader(fs::File::open(output.join("cache_info.json")).unwrap()).unwrap();
    assert_ne!(
      derived.vanilla_input_timestamps[timestamp_key],
      refreshed.vanilla_input_timestamps[timestamp_key]
    );
    assert_eq!(
      refreshed.vanilla_input_timestamps[timestamp_key],
      source_timestamps(&vanilla, &resolve_files(&manifest, 0).unwrap()).unwrap()[timestamp_key]
    );
    assert!(fs::symlink_metadata(&latest_link).unwrap().file_type().is_symlink());

    BuildVanillaCache {
      vanilla_dir: root.join("vanilla"),
      output_dir: output.clone(),
      through_version: Some("0000".into()),
      force: false,
    }
    .run()
    .unwrap();
    assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"base");

    fs::remove_file(output.join("latest/ybn/map.ybn")).unwrap();
    BuildVanillaCache {
      vanilla_dir: root.join("vanilla"),
      output_dir: output.clone(),
      through_version: Some("base".into()),
      force: false,
    }
    .run()
    .unwrap();
    assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"base");

    BuildVanillaCache {
      vanilla_dir: root.join("vanilla"),
      output_dir: output.clone(),
      through_version: None,
      force: false,
    }
    .run()
    .unwrap();
    assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"patch");
    let relationships: YmapRelationshipIndex =
      serde_json::from_reader(fs::File::open(output.join("ymap_relationships.json")).unwrap())
        .unwrap();
    assert_eq!(relationships.version, "0001-patch");
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn relationship_index_groups_children_by_parent_and_omits_root_maps() {
    let parent_hash = reference_hash("hei_ch1_lod");
    let children = children_by_parent_hash(&BTreeMap::from([
      ("hei_ch1_11.ymap".into(), parent_hash),
      ("root.ymap".into(), 0),
      ("second_child.ymap".into(), parent_hash),
    ]));
    assert_eq!(children[&format!("{parent_hash:08x}")], ["hei_ch1_11.ymap", "second_child.ymap"]);
    assert_eq!(children.len(), 1);
  }

  #[test]
  fn reads_parent_hash_directly_from_rsc7_meta_root() {
    let expected = reference_hash("hei_ch1_lod");
    let root_name_hash = jenk_hash("CMapData");
    let meta = MetaResource {
      root_block_index: 1,
      structures: vec![MetaStructureInfo {
        name_hash: root_name_hash,
        structure_key: 0,
        unknown_8: 0,
        unknown_12: 0,
        unknown_28: 0,
        structure_size: 4,
        entries: vec![MetaStructureEntry {
          name_hash: jenk_hash("parent"),
          data_offset: 0,
          data_type: 0x4a,
          unknown: 0,
          reference_type_index: -1,
          reference_key: 0,
        }],
      }],
      enums: vec![],
      data_blocks: vec![MetaDataBlock {
        structure_name_hash: root_name_hash,
        data: expected.to_le_bytes().to_vec(),
      }],
      name: None,
    };
    let path = std::env::temp_dir().join(format!("direct_parent_{}.ymap", std::process::id()));
    fs::write(&path, meta.to_rsc7(2).unwrap().encode().unwrap()).unwrap();
    assert_eq!(ymap_parent_hash(&path).unwrap(), expected);
    fs::remove_file(path).unwrap();
  }

  #[test]
  #[ignore = "requires generated asset/vanilla-cache latest YMAP links"]
  fn direct_parent_reader_matches_latest_ymap_model() {
    let path =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap/airfield.ymap");
    let ymap = read_ymap(&path).unwrap();
    assert_eq!(ymap_parent_hash(&path).unwrap(), reference_hash(&ymap.parent));
  }
}
