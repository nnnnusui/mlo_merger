use std::{
  fs,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use super::incremental::IncrementalMerge;
use crate::core::source_cache;

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

/// Merges supported source changes and resource-local edits into latest vanilla files.
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
  /// Resource-local edit documents.
  pub overwrite_dir: PathBuf,
  /// Rebuilds merge groups and overwrite preparation regardless of freshness.
  pub force: bool,
}

impl Merge {
  /// Merges changed groups from the current vanilla and source caches and publishes output
  /// atomically.
  ///
  /// Both caches must already be built; this does not update them.
  ///
  /// ```no_run
  /// mlo_merger::core::merge::Merge {
  ///   source_dir: "asset/source".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  ///   source_cache_dir: "asset/source-cache".into(), output_dir: "asset/merged".into(),
  ///   overwrite_dir: "asset/overwrite".into(), force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<()> {
    self.validate_paths()?;
    crate::core::config::matching::with_config(|| self.run_inner())?
  }

  fn validate_paths(&self) -> Result<()> {
    let paths = [
      ("vanilla-cache", &self.vanilla_cache_dir),
      ("source-cache", &self.source_cache_dir),
      ("merged", &self.output_dir),
      ("overwrite", &self.overwrite_dir),
    ]
    .into_iter()
    .map(|(name, path)| Ok((name, crate::common::path::absolute_path(path)?)))
    .collect::<std::result::Result<Vec<_>, std::io::Error>>()?;
    for (index, (name, path)) in paths.iter().enumerate() {
      for (other_name, other) in paths.iter().skip(index + 1) {
        if (*name == "overwrite" || *other_name == "overwrite")
          && (path.starts_with(other) || other.starts_with(path))
        {
          return Err("Overwrite directory overlaps a merge artifact directory".into());
        }
      }
    }
    Ok(())
  }

  fn run_inner(&self) -> Result<()> {
    let source_dir = self.source_dir.canonicalize()?;
    let vanilla_cache_dir = self.vanilla_cache_dir.canonicalize()?;
    let source_cache_dir = self.source_cache_dir.canonicalize()?;
    let mut merge_inputs = source_cache::load_merge_inputs(&source_cache_dir)?;
    let (edits, overwrite_cache) = crate::core::lod_light::PreparedEdits::load_cached(
      &source_dir,
      &source_cache_dir,
      &self.overwrite_dir,
      self.force,
    )?;
    for source in &edits.sources {
      if !merge_inputs.ymap.iter().any(|existing| existing.path == source.path) {
        merge_inputs.ymap.push(source.clone());
      }
    }
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
      force: self.force,
    }
    .run_with_edits(&edits)?;
    if !changed {
      overwrite_cache.save(&source_cache_dir)?;
      return Ok(());
    }
    publish_directory(&staging.0, &output_dir)?;
    overwrite_cache.save(&source_cache_dir)?;
    tracing::info!("Published merged stream files to {}", output_dir.display());
    Ok(())
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
