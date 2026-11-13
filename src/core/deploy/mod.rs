//! Copies merged files and the source cache's moved files into a deployable resource.

mod check;
mod files;
mod metadata;
mod plan;
#[cfg(test)]
mod tests;

use crate::core::vanilla::write_json;
use crate::platform::fs;
use crate::platform::process;
use files::{copy_file, destination, prepare_output, remove_empty_stream_directories};
use metadata::{CopiedFile, DeployMetadata, METADATA_FILE, fingerprint, read_metadata};
use plan::plan;
use std::{
  collections::BTreeMap,
  io::Write,
  path::{Path, PathBuf},
};

pub(crate) use check::check;

/// Whether `resource` is a deployment, which holds merge results rather than source files.
pub fn is_merged_resource(resource: &Path) -> bool {
  fs::is_file(resource.join(METADATA_FILE))
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Copies generated merged files and cache files not replaced by a merged basename.
#[derive(Debug, Clone)]
pub struct Deploy {
  /// Existing merged output directory.
  pub merged_dir: PathBuf,
  /// Existing source cache; only the files its inventory records are deployed.
  pub source_cache_dir: PathBuf,
  /// Destination containing `stream/` and deployment provenance.
  pub output_dir: PathBuf,
  /// Copies every selected file even when its contents are unchanged.
  pub force: bool,
}

/// Counts the file operations performed by a deployment.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DeploySummary {
  /// Files copied or repaired.
  pub copied: usize,
  /// Files whose contents already matched.
  pub skipped: usize,
  /// Previously managed files removed from the deployment.
  pub removed: usize,
}

impl Deploy {
  /// Updates managed copies without replacing the deployment directory or unrelated files.
  ///
  /// ```no_run
  /// use mlo_merger::core::deploy::Deploy;
  /// let summary = Deploy {
  ///   merged_dir: "asset/merged".into(), source_cache_dir: "asset/source-cache".into(),
  ///   output_dir: "merged_mlo".into(), force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<DeploySummary> {
    let merged = fs::canonicalize(&self.merged_dir)?;
    let cache = fs::canonicalize(&self.source_cache_dir)?;
    let output = prepare_output(&self.output_dir, &[&merged, &cache])?;
    let cached = crate::core::source_cache::cached_files(&cache)?;
    let planned = plan(&merged, &cache, &cached)?;
    let mut moved_files = cached.iter().map(|file| file.source_path.as_str()).collect::<Vec<_>>();
    moved_files.sort_unstable();
    let moved_files = moved_files.join("\n");
    let moved_files_path = destination(&output, "files.txt")?;
    let metadata_path = destination(&output, METADATA_FILE)?;
    let previous =
      fs::is_file(&metadata_path).then(|| read_metadata(&metadata_path)).transpose()?;
    for relative in planned.keys().chain(previous.iter().flat_map(|metadata| metadata.files.keys()))
    {
      if !relative.starts_with("stream/") {
        return Err("Managed deployment files must be under stream/".into());
      }
      destination(&output, relative)?;
    }
    let mut summary = DeploySummary::default();
    let mut files = BTreeMap::new();
    for (relative, (source, stored)) in &planned {
      let target = destination(&output, relative)?;
      let old = previous.as_ref().and_then(|metadata| metadata.files.get(relative));
      let input = fingerprint(
        source,
        old.filter(|file| file.source == *stored).map(|file| &file.input),
        self.force,
      )?;
      let existing = if fs::is_file(&target) {
        Some(fingerprint(&target, old.map(|file| &file.output), self.force)?)
      } else {
        None
      };
      let copied = self.force
        || existing
          .as_ref()
          .is_none_or(|file| file.sha256 != input.sha256 || file.size != input.size);
      let target_fingerprint = if copied {
        copy_file(source, &target)?;
        let actual = fingerprint(&target, None, true)?;
        if actual.sha256 != input.sha256 || actual.size != input.size {
          return Err(format!("Source changed during deployment: {}", source.display()).into());
        }
        summary.copied += 1;
        actual
      } else {
        summary.skipped += 1;
        existing.ok_or("Existing deployment fingerprint missing")?
      };
      files.insert(
        relative.clone(),
        CopiedFile {
          source: stored.clone(),
          input,
          output: target_fingerprint,
        },
      );
    }
    if let Some(previous) = &previous {
      for (relative, old) in &previous.files {
        if planned.contains_key(relative) {
          continue;
        }
        let target = destination(&output, relative)?;
        if fs::exists(&target).unwrap_or(false) {
          let actual = fingerprint(&target, Some(&old.output), false)?;
          if actual.sha256 != old.output.sha256 {
            return Err(
              format!("Refusing to remove modified deployment file: {}", target.display()).into(),
            );
          }
          fs::remove_file(target)?;
          summary.removed += 1;
        }
      }
    }
    let metadata = DeployMetadata {
      format_version: 1,
      generated_at: chrono::Utc::now().to_rfc3339(),
      files,
    };
    if previous.as_ref().is_none_or(|previous| previous.files != metadata.files) {
      let temporary = destination(&output, &format!(".deploy-cache-{}.tmp", process::id()))?;
      if fs::exists(&temporary).unwrap_or(false) {
        return Err("Deployment cache temporary file already exists".into());
      }
      write_json(&temporary, &metadata)?;
      fs::rename(temporary, metadata_path)?;
    }
    if fs::read_to_string(&moved_files_path).ok().as_deref() != Some(&moved_files) {
      let temporary = destination(&output, &format!(".files-{}.tmp", process::id()))?;
      let mut writer = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
      writer.write_all(moved_files.as_bytes())?;
      writer.flush()?;
      drop(writer);
      fs::rename(temporary, moved_files_path)?;
    }
    remove_empty_stream_directories(&output)?;
    match fs::OpenOptions::new().write(true).create_new(true).open(output.join("fxmanifest.lua")) {
      Ok(mut manifest) => {
        manifest.write_all(b"fx_version 'cerulean'\ngame 'gta5'\nversion '1.0.0'\nthis_is_a_map 'yes'\n\ndependency '/assetpacks'\n\n")?;
        manifest.flush()?;
      }
      Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
      Err(error) => return Err(error.into()),
    }
    Ok(summary)
  }
}
