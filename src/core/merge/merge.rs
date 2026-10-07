use std::{
  collections::BTreeSet,
  fs,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use super::{run::MergeYmap, ybn_conflicts::MergeYbnConflicts};
use crate::core::source_cache::{self, BuildSourceCache};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
  _force: bool,
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
  let ybn_omit_path = staging.0.join("ybn_omit.txt");
  MergeYbnConflicts {
    source_dir: source_cache_dir.clone(),
    vanilla_dir: vanilla_cache_dir.join("latest/ybn"),
    output_dir: staging.0.clone(),
    omitted_files_path: ybn_omit_path.clone(),
  }
  .run_with_latest_vanilla_files(
    None,
    &latest_files.ybn,
    Some(&merge_inputs.ybn.iter().map(|source| source.path.clone()).collect::<Vec<_>>()),
  )?;

  let ymap_sources = merge_inputs
    .ymap
    .iter()
    .map(|source| (source.resource.clone(), source.path.clone(), source.file_name.clone()))
    .collect::<Vec<_>>();
  let mut vanilla_ymap_names = merge_inputs.vanilla_ymaps_to_read;
  vanilla_ymap_names.extend(ymap_sources.iter().map(|(_, _, name)| name.to_ascii_lowercase()));
  let latest_ymap_by_name = latest_files
    .ymap
    .iter()
    .filter_map(|path| path.file_name()?.to_str().map(|name| (name.to_ascii_lowercase(), path)))
    .collect::<std::collections::HashMap<_, _>>();
  let mut selected_vanilla_ymaps = Vec::new();
  for name in vanilla_ymap_names {
    if let Some(path) = latest_ymap_by_name.get(&name) {
      selected_vanilla_ymaps.push((*path).clone());
    } else {
      log::warn!("Latest vanilla YMAP {name} is missing; skipping it");
    }
  }
  let ymap_output_dir = staging.0.join("ymap");
  let mut omitted = merge_inputs
    .ybn
    .iter()
    .map(|source| {
      source
        .original_path
        .strip_prefix(&source_dir)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
    })
    .collect::<std::result::Result<BTreeSet<_>, _>>()?;
  if !ymap_sources.is_empty() {
    let raw_sources = ymap_sources
      .iter()
      .map(|(resource, path, _)| (resource.clone(), path.clone()))
      .collect::<Vec<_>>();
    fs::create_dir_all(&ymap_output_dir)?;
    MergeYmap {
      vanilla_dir: vanilla_cache_dir.join("latest/ymap"),
      mod_dir: source_dir.clone(),
      mod_ymap_dir: source_dir.clone(),
      output_dir: ymap_output_dir.clone(),
      rebuild_all: true,
      blacklist_config: None,
    }
    .run_with_latest_vanilla_files(&selected_vanilla_ymaps, Some(&raw_sources))?;

    let mut generated_ymaps = BTreeSet::new();
    for entry in fs::read_dir(&ymap_output_dir)? {
      let entry = entry?;
      let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
        continue;
      };
      if !name.to_ascii_lowercase().ends_with(".ymap") {
        continue;
      }
      generated_ymaps.insert(name.to_ascii_lowercase());
    }
    for source in &merge_inputs.ymap {
      if generated_ymaps.contains(&source.file_name.to_ascii_lowercase()) {
        omitted.insert(
          source.original_path.strip_prefix(&source_dir)?.to_string_lossy().replace('\\', "/"),
        );
      }
    }
    let _ = fs::remove_file(ymap_output_dir.join("_copy_targets.txt"));
    let _ = fs::remove_file(ymap_output_dir.join("_managed_ymaps.txt"));
    let _ = fs::remove_dir_all(ymap_output_dir.join("clone"));
  }
  let _ = fs::remove_file(ybn_omit_path);
  for entry in fs::read_dir(&staging.0)? {
    let path = entry?.path();
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
      continue;
    };
    if !path.is_file() || !matches!(extension.to_ascii_lowercase().as_str(), "ymap" | "ybn") {
      continue;
    }
    let directory = staging.0.join(extension.to_ascii_lowercase());
    fs::create_dir_all(&directory)?;
    fs::rename(&path, directory.join(path.file_name().ok_or("Merged file has no name")?))?;
  }
  fs::write(staging.0.join("_omit.txt"), omitted.into_iter().collect::<Vec<_>>().join("\n"))?;
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
      "resource_a/stream/collision.ybn\nresource_b/stream/collision.ybn"
    );
    let deployment = crate::core::deploy::Deploy {
      merged_dir: output_dir.clone(),
      source_cache_dir: source_cache_dir.clone(),
      output_dir: root.join("deployed"),
      force: false,
    };
    assert_eq!(deployment.run().unwrap().copied, 1);
    assert_eq!(
      fs::read(root.join("deployed/stream/ybn/merged/collision.ybn")).unwrap(),
      fs::read(output_dir.join("ybn/collision.ybn")).unwrap(),
    );
    assert!(!root.join("deployed/stream/ybn/clone").exists());
    assert_eq!(deployment.run().unwrap().copied, 0);
    fs::remove_dir_all(root).unwrap();
  }
}
