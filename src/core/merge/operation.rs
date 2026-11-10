use crate::platform::fs;
use crate::platform::process;
use std::path::{Path, PathBuf};

use crate::platform::fs::WalkDir;

use super::incremental::IncrementalMerge;
use crate::common::changes::StoreChanges;
use crate::core::overwrite::Overwrites;
use crate::core::overwrite_cache::OverwriteWarnings;
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
  /// User edits of the merge (see [`crate::core::overwrite`]); `None` applies none.
  pub overwrite_dir: Option<PathBuf>,
  /// What the overwrites call for, built from them beforehand (see
  /// [`crate::core::overwrite_cache`]); `None` derives it here.
  pub overwrite_cache_dir: Option<PathBuf>,
  /// Rebuilds merge groups regardless of freshness.
  pub force: bool,
}

impl Merge {
  /// Merges changed groups from the current vanilla and source caches and publishes output
  /// atomically, then brings the output's stream-file bounds index (`stream_file_rtree.json`)
  /// up to date; returns the merged files it added, changed or removed, relative to the output,
  /// and the [`OverwriteWarnings`], each of which is also logged as a warning.
  ///
  /// Both caches must already be built; this does not update them.
  ///
  /// ```no_run
  /// mlo_merger::core::merge::Merge {
  ///   source_dir: "asset/source".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  ///   source_cache_dir: "asset/source-cache".into(), output_dir: "asset/merged".into(),
  ///   overwrite_dir: Some("asset/overwrite".into()),
  ///   overwrite_cache_dir: Some("asset/overwrite-cache".into()), force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<MergeSummary> {
    crate::core::config::matching::with_config(|| self.run_inner())?
  }

  fn run_inner(&self) -> Result<MergeSummary> {
    let source_dir = fs::canonicalize(&self.source_dir)?;
    let vanilla_cache_dir = fs::canonicalize(&self.vanilla_cache_dir)?;
    let source_cache_dir = fs::canonicalize(&self.source_cache_dir)?;
    let mut merge_inputs = source_cache::load_merge_inputs(&source_cache_dir, &source_dir)?;
    let overwrites = match &self.overwrite_dir {
      Some(directory) => Overwrites::load(directory)?,
      None => Overwrites::default(),
    };
    let (added_files, warnings) = match &self.overwrite_cache_dir {
      Some(directory) => {
        let (added, warnings) = crate::core::overwrite_cache::load(
          directory,
          &source_cache_dir,
          &vanilla_cache_dir,
          self.overwrite_dir.as_deref(),
        )?;
        (added.files, warnings)
      }
      None => {
        let files = crate::core::overwrite_cache::resolve_without_cache(&merge_inputs, &overwrites);
        let warnings =
          crate::core::overwrite_cache::warnings(&merge_inputs, &overwrites, &vanilla_cache_dir)?;
        (files, warnings)
      }
    };
    // Logged on every run, as a merge reusing its previous output does not look at overwrites.
    warnings.log();
    let added = super::added::plans(&added_files);
    // A stream file an overwrite ignores is left out of the merge whole.
    merge_inputs
      .ymap
      .retain(|source| !overwrites.for_source(source).is_some_and(|overwrite| overwrite.ignore));
    let latest_files = dispatch_vanilla_stream_files(&vanilla_cache_dir.join("latest"))?;
    let output_dir = prepare_output_path(&self.output_dir)?;
    let staging = Staging(create_staging_directory(&output_dir)?);
    let staged = IncrementalMerge {
      vanilla_cache: &vanilla_cache_dir,
      source_cache: &source_cache_dir,
      output: &output_dir,
      staging: &staging.0,
      ybn: &latest_files.ybn,
      ymap: &latest_files.ymap,
      inputs: &merge_inputs,
      overwrites: &overwrites,
      added: &added,
      force: self.force,
    }
    .run()?;
    if staged.is_some() {
      publish_directory(&staging.0, &output_dir)?;
      tracing::info!("Published merged stream files to {}", output_dir.display());
    }
    crate::core::find::rtree::update_store_index(
      crate::core::find::rtree::Store::Merged,
      &output_dir,
    )?;
    Ok(MergeSummary {
      changes: staged.unwrap_or_default(),
      warnings,
    })
  }
}

/// Outcome of a merge.
#[derive(Debug, Default)]
pub struct MergeSummary {
  /// Merged files added, changed or removed, relative to the output.
  pub changes: StoreChanges,
  /// What the overwrites leave undecided or no longer match, whether or not this run merged
  /// those files again.
  pub warnings: OverwriteWarnings,
}

fn dispatch_vanilla_stream_files(latest_dir: &Path) -> Result<VanillaStreamFiles> {
  let mut files = VanillaStreamFiles {
    ymap: Vec::new(),
    ybn: Vec::new(),
  };
  for entry in WalkDir::new(latest_dir).follow_links(false) {
    let entry = entry?;
    let path = entry.path();
    if !fs::is_file(path) {
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
    if output.is_absolute() { output.to_path_buf() } else { process::current_dir()?.join(output) };
  let parent = absolute.parent().ok_or("Merge output has no parent")?;
  fs::create_dir_all(parent)?;
  let output =
    fs::canonicalize(parent)?.join(absolute.file_name().ok_or("Merge output has no name")?);
  let current_dir = fs::canonicalize(&process::current_dir()?)?;
  if output == current_dir || current_dir.starts_with(&output) {
    return Err("Refusing to replace the workspace directory or one of its parents".into());
  }
  if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err(format!("Refusing to replace symlink output {}", output.display()).into());
  }
  if fs::exists(&output).unwrap_or(false) && !fs::is_dir(&output) {
    return Err(format!("Merge output is not a directory: {}", output.display()).into());
  }
  Ok(output)
}

fn create_staging_directory(output: &Path) -> Result<PathBuf> {
  let parent = output.parent().ok_or("Merge output has no parent")?;
  let name = output.file_name().ok_or("Merge output has no name")?.to_string_lossy();
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
  if fs::exists(&backup).unwrap_or(false) {
    return Err(format!("Merge backup already exists: {}", backup.display()).into());
  }
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
  if existed && let Err(error) = fs::remove_dir_all(backup) {
    tracing::warn!("Could not remove previous merge output: {error}");
  }
  Ok(())
}

#[cfg(test)]
#[path = "test/mod.rs"]
mod tests;
