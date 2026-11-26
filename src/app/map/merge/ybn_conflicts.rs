//! Merging the source YBNs named after a vanilla YBN against their latest vanilla file.
//!
//! Each source's differences are read from its source-cache diff document, or taken from the
//! file when it has none, and its overwrite's skip is applied to them; changes carried from
//! another generation ([`YbnPropagatedDiff`]) follow the sources, in the order given. A YBN only
//! one source replaces, without an overwrite, is written as that source has it.

use crate::platform::fs;
use std::{collections::BTreeMap, io, path::PathBuf};

use crate::app::map::overwrite::{OverwriteFingerprint, StreamFileOverwrite};
use crate::core::format::ybn::{
  diff::polygons::{self, YbnComparisonDiff, YbnDiffFields, YbnPropagatedDiff},
  merge_ybn_diffs, read_ybn,
};

/// A source YBN to merge: its file, its diff document and its overwrite; or, when `propagated`,
/// changes carried from another generation, `path` and `diff_path` naming their document.
#[derive(Debug, Clone)]
pub struct YbnSource {
  /// The vanilla YBN it changes, by its file name.
  pub file_name: String,
  pub path: PathBuf,
  pub diff_path: Option<PathBuf>,
  pub overwrite: Option<StreamFileOverwrite>,
  pub propagated: bool,
}

/// Merges same-named YBN resource overrides against vanilla Bounds data.
pub struct MergeYbnConflicts {
  /// Root the source paths are recorded relative to in [`Self::omitted_files_path`].
  pub source_dir: PathBuf,
  pub output_dir: PathBuf,
  /// Receives the merged sources' paths relative to [`Self::source_dir`], one per line.
  pub omitted_files_path: PathBuf,
}

impl MergeYbnConflicts {
  /// Merges each group of `source_files` sharing a file name into the vanilla file of that name
  /// among `vanilla_files`, writing it to the output directory; a group without one is skipped.
  pub fn run(
    &self,
    vanilla_files: &[PathBuf],
    source_files: &[PathBuf],
  ) -> Result<(), Box<dyn std::error::Error>> {
    let sources = source_files
      .iter()
      .map(|path| YbnSource {
        file_name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        path: path.clone(),
        diff_path: None,
        overwrite: None,
        propagated: false,
      })
      .collect::<Vec<_>>();
    self.run_sources(vanilla_files, &sources)
  }

  /// [`Self::run`] for `sources` with their diff documents and overwrites.
  pub fn run_sources(
    &self,
    vanilla_files: &[PathBuf],
    sources: &[YbnSource],
  ) -> Result<(), Box<dyn std::error::Error>> {
    crate::core::config::matching::with_config(|| self.run_configured(vanilla_files, sources))?
  }

  fn run_configured(
    &self,
    vanilla_files: &[PathBuf],
    sources: &[YbnSource],
  ) -> Result<(), Box<dyn std::error::Error>> {
    let limits = crate::core::config::geometry_limits()?;
    let groups = group_by_name(sources);
    let vanilla = vanilla_by_name(vanilla_files)?;
    fs::create_dir_all(&self.output_dir)?;
    for entry in fs::read_dir(&self.output_dir)? {
      let path = entry?.path();
      if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ybn")) {
        fs::remove_file(path)?;
      }
    }
    let mut omitted = Vec::new();
    let mut merged_groups = 0;
    for (name, sources) in &groups {
      let Some(vanilla_path) = vanilla.get(name) else {
        tracing::warn!("Skipping YBN source group {name}: latest vanilla file is missing");
        continue;
      };
      let output_name = vanilla_path.file_name().ok_or("vanilla YBN filename missing")?;
      let merged = match sources.as_slice() {
        [source]
          if !source.propagated
            && source.overwrite.as_ref().is_none_or(|overwrite| overwrite.skip.is_none()) =>
        {
          fs::read(&source.path)?
        }
        _ => {
          let diffs = sources
            .iter()
            .map(|source| self.diff(vanilla_path, source))
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
          merge_ybn_diffs(&fs::read(vanilla_path)?, &diffs, limits)?
        }
      };
      fs::write(self.output_dir.join(output_name), merged)?;
      for source in sources.iter().filter(|source| !source.propagated) {
        omitted
          .push(source.path.strip_prefix(&self.source_dir)?.to_string_lossy().replace('\\', "/"));
      }
      merged_groups += 1;
      tracing::debug!("    Merged YBN {name} from {} source files", sources.len());
    }
    omitted.sort();
    omitted.dedup();
    fs::write(&self.omitted_files_path, omitted.join("\n"))?;
    tracing::debug!("Merged {merged_groups} vanilla-backed YBN source groups");
    Ok(())
  }

  /// `source`'s differences from the vanilla YBN at `vanilla_path`, after its overwrite: those
  /// of its diff document when it was taken with the active tolerance, else taken from the files.
  fn diff(
    &self,
    vanilla_path: &std::path::Path,
    source: &YbnSource,
  ) -> Result<YbnDiffFields, Box<dyn std::error::Error>> {
    let tolerance = crate::core::config::matching::current().ybn_position;
    if source.propagated {
      let path = source.diff_path.as_ref().unwrap_or(&source.path);
      return Ok(crate::common::json::read_json::<YbnPropagatedDiff>(path)?.diffs);
    }
    let document = source
      .diff_path
      .as_ref()
      .and_then(|path| crate::common::json::read_json::<YbnComparisonDiff>(path).ok())
      .filter(|document| {
        document.schema_version == YbnComparisonDiff::SCHEMA_VERSION
          && document.tolerance == tolerance
      });
    let (mut diffs, target, base) = match document {
      Some(document) => (
        document.diffs,
        OverwriteFingerprint::from(&document.target),
        OverwriteFingerprint::from(&document.base),
      ),
      None => (
        polygons::diff(
          &read_ybn(&fs::read(vanilla_path)?)?,
          &read_ybn(&fs::read(&source.path)?)?,
          tolerance,
        )?,
        OverwriteFingerprint::read(&source.path)?,
        OverwriteFingerprint::read(vanilla_path)?,
      ),
    };
    if let Some(overwrite) = &source.overwrite {
      overwrite.apply(&mut diffs, &target, &base, &source.path.display().to_string())?;
    }
    Ok(diffs)
  }
}

/// `sources` grouped by lowercase file name, each group's sources sorted by path, followed by
/// the changes carried to it in the order given.
fn group_by_name(sources: &[YbnSource]) -> BTreeMap<String, Vec<YbnSource>> {
  let mut groups = BTreeMap::<String, Vec<YbnSource>>::new();
  for source in sources {
    groups.entry(source.file_name.to_ascii_lowercase()).or_default().push(source.clone());
  }
  for sources in groups.values_mut() {
    sources.sort_by(|first, second| {
      first.propagated.cmp(&second.propagated).then_with(|| {
        if first.propagated { std::cmp::Ordering::Equal } else { first.path.cmp(&second.path) }
      })
    });
  }
  groups
}

/// The YBNs among `paths` keyed by lowercase file name; two of one name are an error.
fn vanilla_by_name(paths: &[PathBuf]) -> io::Result<BTreeMap<String, &PathBuf>> {
  let mut files = BTreeMap::new();
  for path in paths {
    if !fs::is_file(path)
      || path.extension().is_none_or(|extension| !extension.eq_ignore_ascii_case("ybn"))
    {
      continue;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
      continue;
    };
    if files.insert(name.to_ascii_lowercase(), path).is_some() {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("duplicate vanilla YBN basename: {name}"),
      ));
    }
  }
  Ok(files)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn groups_ybn_names_case_insensitively_and_keeps_hi_variants_separate() {
    let source = |path: &str| YbnSource {
      file_name: path.rsplit('/').next().unwrap().to_owned(),
      path: PathBuf::from(path),
      diff_path: None,
      overwrite: None,
      propagated: false,
    };
    let groups = group_by_name(&[
      source("resource_a/stream/sc1_18_0.ybn"),
      source("resource_b/stream/sub/SC1_18_0.YBN"),
      source("resource_a/stream/hi@sc1_18_0.ybn"),
      source("resource_b/stream/sub/hi@sc1_18_0.ybn"),
    ]);
    assert_eq!(groups["sc1_18_0.ybn"].len(), 2);
    assert_eq!(groups["hi@sc1_18_0.ybn"].len(), 2);
    assert_eq!(groups.len(), 2);
  }
}
