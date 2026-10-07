use std::{
  fs,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use super::incremental::IncrementalMerge;
use crate::core::source_cache::{self, BuildSourceCache};

pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct VanillaStreamFiles {
  ymap: Vec<PathBuf>,
  ybn: Vec<PathBuf>,
}

struct Staging(PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

/// Updates prerequisites, dispatches latest vanilla files by format, and publishes merged output.
pub fn run(
  source_dir: &Path,
  vanilla_dir: &Path,
  vanilla_cache_dir: &Path,
  source_cache_dir: &Path,
  output_dir: &Path,
  force: bool,
) -> Result<()> {
  let source_dir = source_dir.canonicalize()?;
  let vanilla_dir = vanilla_dir.canonicalize()?;
  BuildSourceCache {
    source_dir: source_dir.clone(),
    output_dir: source_cache_dir.to_path_buf(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.to_path_buf(),
    force: false,
  }
  .run()?;
  let vanilla_cache_dir = vanilla_cache_dir.canonicalize()?;
  let source_cache_dir = source_cache_dir.canonicalize()?;
  let merge_inputs = source_cache::load_merge_inputs(&source_cache_dir)?;
  let latest_files = dispatch_vanilla_stream_files(&vanilla_cache_dir.join("latest"))?;
  let output_dir = prepare_output_path(
    output_dir,
    &[&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir],
  )?;
  let staging = Staging(create_staging_directory(&output_dir)?);
  let changed = IncrementalMerge {
    source_dir: &source_dir,
    vanilla_cache: &vanilla_cache_dir,
    source_cache: &source_cache_dir,
    output: &output_dir,
    staging: &staging.0,
    ybn: &latest_files.ybn,
    ymap: &latest_files.ymap,
    inputs: &merge_inputs,
    force,
  }
  .run()?;
  if !changed {
    return Ok(());
  }
  publish_directory(&staging.0, &output_dir)?;
  log::info!("Published merged stream files to {}", output_dir.display());
  Ok(())
}

fn dispatch_vanilla_stream_files(latest_dir: &Path) -> Result<VanillaStreamFiles> {
  let mut files = VanillaStreamFiles {
    ymap: Vec::new(),
    ybn: Vec::new(),
  };
  for entry in WalkDir::new(latest_dir).follow_links(false) {
    let entry = entry?;
    let path = entry.path();
    if !path.is_file() {
      continue;
    }
    match path.extension().and_then(|extension| extension.to_str()) {
      Some(extension) if extension.eq_ignore_ascii_case("ymap") => {
        files.ymap.push(path.to_path_buf());
      }
      Some(extension) if extension.eq_ignore_ascii_case("ybn") => {
        files.ybn.push(path.to_path_buf());
      }
      Some(extension) => log::debug!(
        "Skipping unsupported vanilla stream extension .{extension}: {}",
        path.display()
      ),
      None => log::debug!("Skipping extensionless vanilla stream file: {}", path.display()),
    }
  }
  files.ymap.sort();
  files.ybn.sort();
  Ok(files)
}

fn prepare_output_path(
  output: &Path,
  inputs: &[&Path],
) -> Result<PathBuf> {
  let absolute =
    if output.is_absolute() { output.to_path_buf() } else { std::env::current_dir()?.join(output) };
  let parent = absolute.parent().ok_or("Merge output has no parent")?;
  fs::create_dir_all(parent)?;
  let output = parent.canonicalize()?.join(absolute.file_name().ok_or("Merge output has no name")?);
  let current_dir = std::env::current_dir()?.canonicalize()?;
  if output == current_dir || current_dir.starts_with(&output) {
    return Err("Refusing to replace the workspace directory or one of its parents".into());
  }
  for input in inputs {
    if output == *input || output.starts_with(input) || input.starts_with(&output) {
      return Err(format!("Merge output overlaps input {}", input.display()).into());
    }
  }
  if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err(format!("Refusing to replace symlink output {}", output.display()).into());
  }
  if output.exists() && !output.is_dir() {
    return Err(format!("Merge output is not a directory: {}", output.display()).into());
  }
  Ok(output)
}

fn create_staging_directory(output: &Path) -> Result<PathBuf> {
  let parent = output.parent().ok_or("Merge output has no parent")?;
  let name = output.file_name().ok_or("Merge output has no name")?.to_string_lossy();
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
  if backup.exists() {
    return Err(format!("Merge backup already exists: {}", backup.display()).into());
  }
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
  if existed && let Err(error) = fs::remove_dir_all(backup) {
    log::warn!("Could not remove previous merge output: {error}");
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::vanilla::{
    CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
  };
  use sha2::{Digest, Sha256};
  use std::collections::BTreeMap;

  #[test]
  #[ignore = "requires local vanilla YMAP schemas"]
  fn incremental_ymap_merge_caches_noop_results_and_removes_obsolete_outputs() {
    use crate::core::format::gamefile::{
      meta_resource::{MetaResource, MetaSchemaCatalog},
      meta_xml::ymap_to_model_with_entities,
      resource_file::Rsc7Resource,
    };
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
    let sample = fs::read_dir(local)
      .unwrap()
      .map(|entry| entry.unwrap().path())
      .find(|path| path.file_name().unwrap().to_string_lossy().contains("occl"))
      .unwrap();
    let bytes = fs::read(sample).unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&meta);
    let (model, entities) = ymap_to_model_with_entities(&bytes, &catalog.hash_names).unwrap();
    let root = std::env::temp_dir().join(format!("incremental_ymap_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let source = root.join("source");
    let resource = source.join("resource");
    let vanilla = root.join("vanilla");
    let cache = root.join("vanilla-cache");
    let source_cache = root.join("source-cache");
    let output = root.join("merged");
    fs::create_dir_all(resource.join("stream")).unwrap();
    fs::write(resource.join("fxmanifest.lua"), []).unwrap();
    let object = "0000-base/ymap/map.ymap";
    fs::create_dir_all(vanilla.join("0000-base/ymap")).unwrap();
    fs::write(vanilla.join(object), &bytes).unwrap();
    write_json(
      &vanilla.join("cache_info.json"),
      &VanillaCacheManifest {
        format_version: 1,
        game_dir: root.join("game"),
        versions: vec![CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          unchanged: 0,
          changes: BTreeMap::from([(
            "map.ymap".into(),
            FileChange {
              previous_sha256: None,
              file: CachedFile {
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                object: object.into(),
                source: "base.rpf/map.ymap".into(),
              },
            },
          )]),
        }],
      },
    )
    .unwrap();
    let mut unsupported = model.clone();
    unsupported.flags ^= 1;
    let write_source = |model: &crate::core::format::ymap::model::Ymap| {
      fs::write(
        resource.join("stream/map.ymap"),
        crate::core::format::ymap::binary::write_ymap(model, &entities, &catalog).unwrap(),
      )
      .unwrap();
    };
    let read_cache = || {
      serde_json::from_reader::<_, super::super::incremental::MergeMetadata>(
        fs::File::open(output.join("merge_cache_info.json")).unwrap(),
      )
      .unwrap()
    };
    write_source(&unsupported);
    run(&source, &vanilla, &cache, &source_cache, &output, false).unwrap();
    let initial = read_cache();
    assert!(initial.files["map.ymap"].output.is_none());
    assert!(!output.join("ymap/map.ymap").exists());
    run(&source, &vanilla, &cache, &source_cache, &output, false).unwrap();
    assert_eq!(read_cache().files["map.ymap"].merged_at, initial.files["map.ymap"].merged_at);
    let mut edited = model.clone();
    let bit = (!edited.content_flags).trailing_zeros();
    assert!(bit < 32);
    edited.content_flags |= 1u32 << bit;
    write_source(&edited);
    run(&source, &vanilla, &cache, &source_cache, &output, false).unwrap();
    assert!(output.join("ymap/map.ymap").is_file());
    let actual = fs::read(output.join("ymap/map.ymap")).unwrap();
    assert_eq!(
      ymap_to_model_with_entities(&actual, &catalog.hash_names).unwrap().0.content_flags,
      edited.content_flags
    );
    assert_eq!(fs::read_to_string(output.join("_omit.txt")).unwrap(), "resource/stream/map.ymap");
    let changed = read_cache();
    run(&source, &vanilla, &cache, &source_cache, &output, false).unwrap();
    assert_eq!(read_cache().files["map.ymap"].merged_at, changed.files["map.ymap"].merged_at);
    write_source(&unsupported);
    run(&source, &vanilla, &cache, &source_cache, &output, false).unwrap();
    assert!(!output.join("ymap/map.ymap").exists());
    assert!(read_cache().files["map.ymap"].output.is_none());
    assert!(fs::read_to_string(output.join("_omit.txt")).unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn merge_dispatches_latest_vanilla_ybn_and_writes_omit_list() {
    let root = std::env::temp_dir().join(format!("merge_latest_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let source_dir = root.join("source");
    let resource_dir = source_dir.join("resource_a");
    let second_resource_dir = source_dir.join("resource_b");
    let source_stream = resource_dir.join("stream");
    let second_source_stream = second_resource_dir.join("stream");
    let vanilla_dir = root.join("vanilla");
    let vanilla_cache_dir = root.join("vanilla-cache");
    let source_cache_dir = root.join("source-cache");
    let output_dir = root.join("merged");
    fs::create_dir_all(&source_stream).unwrap();
    fs::create_dir_all(&second_source_stream).unwrap();
    fs::write(resource_dir.join("fxmanifest.lua"), []).unwrap();
    fs::write(second_resource_dir.join("fxmanifest.lua"), []).unwrap();
    let xml = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
    ));
    let bytes = crate::core::format::ybn::xml::xml_to_ybn(xml).unwrap();
    fs::write(source_stream.join("collision.ybn"), &bytes).unwrap();
    fs::write(second_source_stream.join("collision.ybn"), &bytes).unwrap();

    let object = "0000-base/ybn/collision.ybn";
    let vanilla_file = vanilla_dir.join(object);
    fs::create_dir_all(vanilla_file.parent().unwrap()).unwrap();
    fs::write(&vanilla_file, &bytes).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    write_json(
      &vanilla_dir.join("cache_info.json"),
      &VanillaCacheManifest {
        format_version: 1,
        game_dir: root.join("game"),
        versions: vec![CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          changes: BTreeMap::from([(
            "collision.ybn".into(),
            FileChange {
              previous_sha256: None,
              file: CachedFile {
                sha256,
                object: object.into(),
                source: "base.rpf/collision.ybn".into(),
              },
            },
          )]),
          unchanged: 0,
        }],
      },
    )
    .unwrap();

    let independent_object = "0000-base/ybn/independent.ybn";
    fs::write(vanilla_dir.join(independent_object), &bytes).unwrap();
    for stream in [&source_stream, &second_source_stream] {
      fs::write(stream.join("independent.ybn"), &bytes).unwrap();
    }
    let mut manifest: VanillaCacheManifest =
      serde_json::from_reader(fs::File::open(vanilla_dir.join("cache_info.json")).unwrap())
        .unwrap();
    manifest.versions[0].changes.insert(
      "independent.ybn".into(),
      FileChange {
        previous_sha256: None,
        file: CachedFile {
          sha256: format!("{:x}", Sha256::digest(&bytes)),
          object: independent_object.into(),
          source: "base.rpf/independent.ybn".into(),
        },
      },
    );
    write_json(&vanilla_dir.join("cache_info.json"), &manifest).unwrap();

    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();

    assert!(vanilla_cache_dir.join("latest/ybn/collision.ybn").is_file());
    assert!(source_cache_dir.join("source_cache_info.json").is_file());
    let merged =
      crate::core::format::ybn::read_ybn(&fs::read(output_dir.join("ybn/collision.ybn")).unwrap())
        .unwrap();
    let source = crate::core::format::ybn::read_ybn(&bytes).unwrap();
    let diff = crate::core::format::ybn::diff::YbnDiff::extract_from(&source, &merged).unwrap();
    assert!(diff.bound_diffs.is_empty());
    assert!(diff.polygon_diffs.is_empty());
    assert_eq!(
      fs::read_to_string(output_dir.join("_omit.txt")).unwrap().trim(),
      "resource_a/stream/collision.ybn\nresource_a/stream/independent.ybn\nresource_b/stream/collision.ybn\nresource_b/stream/independent.ybn"
    );
    let deployment = crate::core::deploy::Deploy {
      merged_dir: output_dir.clone(),
      source_cache_dir: source_cache_dir.clone(),
      output_dir: root.join("deployed"),
      force: false,
    };
    assert_eq!(deployment.run().unwrap().copied, 2);
    let moved_files = "resource_a/stream/collision.ybn\nresource_a/stream/independent.ybn\nresource_b/stream/collision.ybn\nresource_b/stream/independent.ybn";
    assert_eq!(fs::read_to_string(root.join("deployed/files.txt")).unwrap(), moved_files);
    let list_modified = fs::metadata(root.join("deployed/files.txt")).unwrap().modified().unwrap();
    assert_eq!(
      fs::read(root.join("deployed/stream/ybn/merged/collision.ybn")).unwrap(),
      fs::read(output_dir.join("ybn/collision.ybn")).unwrap(),
    );
    assert!(!root.join("deployed/stream/ybn/clone").exists());
    assert_eq!(deployment.run().unwrap().copied, 0);
    assert_eq!(
      fs::metadata(root.join("deployed/files.txt")).unwrap().modified().unwrap(),
      list_modified
    );
    let read_cache = || {
      serde_json::from_reader::<_, super::super::incremental::MergeMetadata>(
        fs::File::open(output_dir.join("merge_cache_info.json")).unwrap(),
      )
      .unwrap()
    };
    let initial = read_cache();
    assert_eq!(initial.files.len(), 2);
    let independent = output_dir.join("ybn/independent.ybn");
    let modified = fs::metadata(&independent).unwrap().modified().unwrap();
    let cache_modified =
      fs::metadata(output_dir.join("merge_cache_info.json")).unwrap().modified().unwrap();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();
    assert_eq!(
      fs::metadata(output_dir.join("merge_cache_info.json")).unwrap().modified().unwrap(),
      cache_modified
    );
    fs::File::options()
      .write(true)
      .open(source_cache_dir.join("resources/resource_a/independent.ybn"))
      .unwrap()
      .set_times(
        fs::FileTimes::new()
          .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(10)),
      )
      .unwrap();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();
    assert_eq!(
      read_cache().files["independent.ybn"].merged_at,
      initial.files["independent.ybn"].merged_at
    );
    assert_eq!(fs::metadata(&independent).unwrap().modified().unwrap(), modified);
    let mut changed_source = bytes.clone();
    changed_source.extend_from_slice(b"changed source fingerprint");
    crate::core::format::ybn::read_ybn(&changed_source).unwrap();
    let cached_source = source_cache_dir.join("resources/resource_a/collision.ybn");
    fs::write(&cached_source, &changed_source).unwrap();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();
    let changed = read_cache();
    assert_ne!(changed.files["collision.ybn"].merged_at, initial.files["collision.ybn"].merged_at);
    assert_eq!(
      changed.files["independent.ybn"].merged_at,
      initial.files["independent.ybn"].merged_at
    );
    assert_eq!(fs::metadata(&independent).unwrap().modified().unwrap(), modified);
    fs::write(output_dir.join("ybn/collision.ybn"), b"broken output").unwrap();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();
    crate::core::format::ybn::read_ybn(&fs::read(output_dir.join("ybn/collision.ybn")).unwrap())
      .unwrap();
    assert_eq!(
      read_cache().files["independent.ybn"].merged_at,
      initial.files["independent.ybn"].merged_at
    );
    let before_vanilla = read_cache();
    let mut updated_vanilla = bytes.clone();
    updated_vanilla.extend_from_slice(b"changed vanilla fingerprint");
    fs::write(&vanilla_file, &updated_vanilla).unwrap();
    manifest.versions[0].changes.get_mut("collision.ybn").unwrap().file.sha256 =
      format!("{:x}", Sha256::digest(&updated_vanilla));
    write_json(&vanilla_dir.join("cache_info.json"), &manifest).unwrap();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();
    let after_vanilla = read_cache();
    assert_ne!(
      after_vanilla.files["collision.ybn"].merged_at,
      before_vanilla.files["collision.ybn"].merged_at
    );
    assert_eq!(
      after_vanilla.files["independent.ybn"].merged_at,
      before_vanilla.files["independent.ybn"].merged_at
    );
    fs::remove_file(&independent).unwrap();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();
    assert!(independent.is_file());
    let before_force = read_cache();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, true)
      .unwrap();
    let forced = read_cache();
    assert!(
      forced
        .files
        .iter()
        .all(|(name, record)| record.merged_at != before_force.files[name].merged_at)
    );
    let preserved = fs::read(output_dir.join("merge_cache_info.json")).unwrap();
    let preserved_output = fs::read(output_dir.join("ybn/collision.ybn")).unwrap();
    fs::write(&cached_source, b"invalid source file").unwrap();
    assert!(
      run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
        .is_err()
    );
    assert_eq!(fs::read(output_dir.join("merge_cache_info.json")).unwrap(), preserved);
    assert_eq!(fs::read(output_dir.join("ybn/collision.ybn")).unwrap(), preserved_output);
    fs::write(cached_source, changed_source).unwrap();
    fs::remove_dir_all(&second_resource_dir).unwrap();
    run(&source_dir, &vanilla_dir, &vanilla_cache_dir, &source_cache_dir, &output_dir, false)
      .unwrap();
    assert!(read_cache().files.is_empty());
    assert!(!output_dir.join("ybn/collision.ybn").exists());
    assert!(!independent.exists());
    assert!(fs::read_to_string(output_dir.join("_omit.txt")).unwrap().is_empty());
    assert_eq!(deployment.run().unwrap().copied, 2);
    assert_eq!(
      fs::read_to_string(root.join("deployed/files.txt")).unwrap(),
      "resource_a/stream/collision.ybn\nresource_a/stream/independent.ybn"
    );
    assert!(root.join("deployed/stream/ybn/clone/resource_a/collision.ybn").is_file());
    fs::remove_dir_all(root).unwrap();
  }
}
