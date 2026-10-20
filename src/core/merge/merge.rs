use std::{
  fs,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use super::incremental::IncrementalMerge;
use crate::core::overwrite::Overwrites;
use crate::core::source_cache::{self, MergeInputs};

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

/// Merges supported source changes into latest vanilla files.
#[derive(Debug, Clone)]
pub struct Merge {
  /// Root containing source resources.
  pub source_dir: PathBuf,
  /// Derived latest vanilla files.
  pub vanilla_cache_dir: PathBuf,
  /// Source inventory and cached stream files.
  pub source_cache_dir: PathBuf,
  /// Destination for staged merged output.
  pub output_dir: PathBuf,
  /// User edits of the source diffs (`resources/<resource name>.json`); `None` applies none.
  pub overwrite_dir: Option<PathBuf>,
  /// Rebuilds merge groups regardless of freshness.
  pub force: bool,
}

impl Merge {
  /// Merges changed groups from the current vanilla and source caches and publishes output
  /// atomically, then brings the output's stream-file bounds index (`stream_file_rtree.json`)
  /// up to date.
  ///
  /// Both caches must already be built; this does not update them.
  ///
  /// ```no_run
  /// mlo_merger::core::merge::Merge {
  ///   source_dir: "asset/source".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  ///   source_cache_dir: "asset/source-cache".into(), output_dir: "asset/merged".into(),
  ///   overwrite_dir: Some("asset/overwrite".into()), force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<()> {
    crate::core::config::matching::with_config(|| self.run_inner())?
  }

  fn run_inner(&self) -> Result<()> {
    let source_dir = self.source_dir.canonicalize()?;
    let vanilla_cache_dir = self.vanilla_cache_dir.canonicalize()?;
    let source_cache_dir = self.source_cache_dir.canonicalize()?;
    let merge_inputs = source_cache::load_merge_inputs(&source_cache_dir)?;
    let overwrites = match &self.overwrite_dir {
      Some(directory) => Overwrites::load(directory)?,
      None => Overwrites::default(),
    };
    warn_unused_overwrites(&overwrites, &merge_inputs);
    let latest_files = dispatch_vanilla_stream_files(&vanilla_cache_dir.join("latest"))?;
    let output_dir = prepare_output_path(&self.output_dir)?;
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
      overwrites: &overwrites,
      force: self.force,
    }
    .run()?;
    if changed {
      publish_directory(&staging.0, &output_dir)?;
      tracing::info!("Published merged stream files to {}", output_dir.display());
    }
    crate::core::find::rtree::update_store_index(
      crate::core::find::rtree::Store::Merged,
      &output_dir,
    )?;
    Ok(())
  }
}

/// Warns of overwrites naming a stream file that is not merged.
fn warn_unused_overwrites(
  overwrites: &Overwrites,
  inputs: &MergeInputs,
) {
  let merged = inputs
    .ymap
    .iter()
    .filter_map(|source| {
      let resource = Path::new(&source.resource).file_name()?.to_str()?.to_ascii_lowercase();
      Some((resource, source.stream_path.replace('\\', "/").to_ascii_lowercase()))
    })
    .collect::<std::collections::BTreeSet<_>>();
  for (resource, stream_path) in overwrites.keys() {
    if !merged.contains(&(resource.to_string(), stream_path.to_string())) {
      tracing::warn!("Overwrite of {stream_path} ({resource}) names no merged source YMAP");
    }
  }
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
      Some(extension) => tracing::debug!(
        "Skipping unsupported vanilla stream extension .{extension}: {}",
        path.display()
      ),
      None => tracing::debug!("Skipping extensionless vanilla stream file: {}", path.display()),
    }
  }
  files.ymap.sort();
  files.ybn.sort();
  Ok(files)
}

fn prepare_output_path(output: &Path) -> Result<PathBuf> {
  let absolute =
    if output.is_absolute() { output.to_path_buf() } else { std::env::current_dir()?.join(output) };
  let parent = absolute.parent().ok_or("Merge output has no parent")?;
  fs::create_dir_all(parent)?;
  let output = parent.canonicalize()?.join(absolute.file_name().ok_or("Merge output has no name")?);
  let current_dir = std::env::current_dir()?.canonicalize()?;
  if output == current_dir || current_dir.starts_with(&output) {
    return Err("Refusing to replace the workspace directory or one of its parents".into());
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
    tracing::warn!("Could not remove previous merge output: {error}");
  }
  Ok(())
}

#[cfg(test)]
#[path = "test/mod.rs"]
mod tests;
