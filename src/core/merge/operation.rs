use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use super::incremental::IncrementalMerge;
use crate::common::changes::StoreChanges;
use crate::core::overwrite::{
  IgnoredOverwrite, IgnoredOverwriteReason, OverwriteFingerprint, Overwrites,
};
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
  /// up to date; returns the merged files it added, changed or removed, relative to the output,
  /// and the overwrites it left out, each of which is also logged as a warning.
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
  pub fn run(&self) -> Result<MergeSummary> {
    crate::core::config::matching::with_config(|| self.run_inner())?
  }

  fn run_inner(&self) -> Result<MergeSummary> {
    let source_dir = self.source_dir.canonicalize()?;
    let vanilla_cache_dir = self.vanilla_cache_dir.canonicalize()?;
    let source_cache_dir = self.source_cache_dir.canonicalize()?;
    let merge_inputs = source_cache::load_merge_inputs(&source_cache_dir, &source_dir)?;
    let overwrites = match &self.overwrite_dir {
      Some(directory) => Overwrites::load(directory)?,
      None => Overwrites::default(),
    };
    let latest_files = dispatch_vanilla_stream_files(&vanilla_cache_dir.join("latest"))?;
    let ignored_overwrites = ignored_overwrites(&overwrites, &merge_inputs, &latest_files.ymap)?;
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
      ignored_overwrites,
    })
  }
}

/// Outcome of a merge.
#[derive(Debug, Default)]
pub struct MergeSummary {
  /// Merged files added, changed or removed, relative to the output.
  pub changes: StoreChanges,
  /// Overwrites left out because the files they name are missing or changed, whether or not
  /// this run merged those files again.
  pub ignored_overwrites: Vec<IgnoredOverwrite>,
}

/// The overwrites that do not apply to the current files, each logged as a warning.
///
/// Checked on every run, as a merge reusing its previous output does not look at overwrites.
/// A resource overwrite is compared with its merged source YMAP and the latest vanilla file of
/// that name, as the merge does; a vanilla overwrite with the latest vanilla file.
fn ignored_overwrites(
  overwrites: &Overwrites,
  inputs: &MergeInputs,
  latest_ymaps: &[PathBuf],
) -> Result<Vec<IgnoredOverwrite>> {
  let latest = latest_ymaps
    .iter()
    .filter_map(|path| Some((path.file_name()?.to_str()?.to_ascii_lowercase(), path)))
    .collect::<BTreeMap<_, _>>();
  let sources = inputs
    .ymap
    .iter()
    .filter_map(|source| {
      let resource = Path::new(&source.resource).file_name()?.to_str()?.to_ascii_lowercase();
      Some(((resource, source.stream_path.replace('\\', "/").to_ascii_lowercase()), source))
    })
    .collect::<BTreeMap<_, _>>();
  let mut ignored = Vec::new();
  for (resource, stream_path) in overwrites.keys() {
    let Some(overwrite) = overwrites.stream_file(resource, stream_path) else {
      continue;
    };
    let source = sources.get(&(resource.to_string(), stream_path.to_string()));
    let vanilla = source.and_then(|source| latest.get(&source.file_name.to_ascii_lowercase()));
    let reason = match (source, vanilla) {
      (Some(source), Some(vanilla)) => overwrite.ignored_because(
        &OverwriteFingerprint::read(&source.path)?,
        &OverwriteFingerprint::read(vanilla)?,
      ),
      _ => Some(IgnoredOverwriteReason::MissingSource),
    };
    if let Some(reason) = reason {
      ignored.push(IgnoredOverwrite {
        resource: Some(resource.to_string()),
        file: stream_path.to_string(),
        reason,
      });
    }
  }
  for (name, overwrite) in overwrites.vanilla_files() {
    let reason = match latest.get(name) {
      Some(vanilla) => (overwrite.vanilla != OverwriteFingerprint::read(vanilla)?)
        .then_some(IgnoredOverwriteReason::VanillaChanged),
      None => Some(IgnoredOverwriteReason::MissingVanilla),
    };
    if let Some(reason) = reason {
      ignored.push(IgnoredOverwrite {
        resource: None,
        file: name.clone(),
        reason,
      });
    }
  }
  for overwrite in &ignored {
    let file = match &overwrite.resource {
      Some(resource) => format!("{} ({resource})", overwrite.file),
      None => format!("vanilla {}", overwrite.file),
    };
    let reason = match overwrite.reason {
      IgnoredOverwriteReason::MissingSource => "names no merged source YMAP",
      IgnoredOverwriteReason::SourceChanged => "was written against another source file",
      IgnoredOverwriteReason::MissingVanilla => "names no latest vanilla YMAP",
      IgnoredOverwriteReason::VanillaChanged => "was written against another vanilla file",
    };
    tracing::warn!("Overwrite of {file} {reason}; not applied");
  }
  Ok(ignored)
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
