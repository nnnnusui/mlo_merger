//! Resource stream scanning, version inference and final diff reports.

use super::{
  Result,
  comparison::{ModelState, best_candidate, distance},
  io::{cache_path, content_hash},
  types::{CandidateScore, FileReport, ResourceReport, Variant},
  vanilla::NativeVariants,
};
use crate::core::{
  gtav_cache::{GtavCacheManifest, write_json},
  merge::YmapDiff,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn stream_files(resource: &Path) -> Result<Vec<PathBuf>> {
  let mut files = Vec::new();
  for name in ["stream", "streams"] {
    let directory = resource.join(name);
    if !directory.is_dir() {
      continue;
    }
    for entry in WalkDir::new(&directory).follow_links(false) {
      let entry = entry?;
      if entry.file_type().is_file() {
        files.push(entry.into_path());
      }
    }
  }
  files.sort();
  Ok(files)
}

struct MatchedFile {
  source: PathBuf,
  relative: String,
  sha256: String,
  model: ModelState,
  variants: Vec<Variant>,
  best: usize,
  scores: Vec<CandidateScore>,
}

pub(super) fn generate_resource(
  resource: &Path,
  id: &str,
  generated_at: &str,
  output: &Path,
  manifest: &GtavCacheManifest,
  histories: &BTreeMap<String, Vec<Variant>>,
  provider: &mut NativeVariants<'_>,
) -> Result<ResourceReport> {
  log::info!("Inferring vanilla version for {id}");
  let files = stream_files(resource)?;
  let scanned_files = files.len();
  let mut unmatched = Vec::new();
  let mut matched = Vec::new();
  for source in files {
    let relative = source.strip_prefix(resource)?.to_string_lossy().replace('\\', "/");
    let name = source
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or("Stream filename is not UTF-8")?
      .to_ascii_lowercase();
    let Some(variants) = histories.get(&name) else {
      log::debug!("No vanilla counterpart: {id}/{relative}");
      unmatched.push(relative);
      continue;
    };
    if !name.ends_with(".ymap") {
      return Err(format!("Matched native type is not supported yet: {name}").into());
    }
    let model = ModelState::new(
      (provider.reader)(&source)
        .map_err(|error| format!("Reading MLO {}: {error}", source.display()))?,
    )?;
    let mut scores = Vec::new();
    for variant in variants {
      let vanilla = provider.model(&variant.file)?;
      let difference_count = distance(&vanilla.comparison, &model.comparison);
      log::info!("Candidate {relative}: {} -> {difference_count} model changes", variant.version);
      scores.push(CandidateScore {
        version: variant.version.clone(),
        sha256: variant.file.sha256.clone(),
        difference_count,
      });
    }
    let best = best_candidate(&scores, variants)?;
    log::info!(
      "Best {relative}: {} ({} changes)",
      variants[best].version,
      scores[best].difference_count
    );
    matched.push(MatchedFile {
      sha256: content_hash(&source)?,
      source,
      relative,
      model,
      variants: variants.clone(),
      best,
      scores,
    });
  }
  let selected = matched.iter().map(|file| file.variants[file.best].index).max();
  let selected_version = selected.map(|index| manifest.versions[index].id.clone());
  if let Some(version) = &selected_version {
    log::info!("Selected vanilla version for {id}: {version}");
  } else {
    log::warn!("No vanilla-matching YMAPs in {id}");
  }
  let directory = cache_path(output, id)?;
  fs::create_dir_all(directory.join("ymap"))?;
  let mut reports = Vec::new();
  for file in matched {
    let baseline = file
      .variants
      .iter()
      .rev()
      .find(|variant| Some(variant.index) <= selected)
      .ok_or("Selected stage precedes file creation")?;
    let vanilla = provider.model(&baseline.file)?;
    log::info!(
      "Generating diff {} against content from {}",
      file.source.display(),
      baseline.version
    );
    let diff = YmapDiff::extract_from(&vanilla.model, &file.model.model);
    let artifact = format!("ymap/{}.diff.json", file.relative);
    let destination = cache_path(&directory, &artifact)?;
    fs::create_dir_all(destination.parent().ok_or("Diff artifact has no parent")?)?;
    write_json(&destination, &diff)?;
    reports.push(FileReport {
      input: file.relative,
      input_sha256: file.sha256,
      best_version: file.variants[file.best].version.clone(),
      best_difference_count: file.scores[file.best].difference_count,
      candidates: file.scores,
      baseline_content_version: baseline.version.clone(),
      baseline_sha256: baseline.file.sha256.clone(),
      baseline_source: baseline.file.source.clone(),
      difference_count: distance(&vanilla.comparison, &file.model.comparison),
      diff: artifact,
    });
  }
  let report = ResourceReport {
    resource: id.into(),
    source: resource.into(),
    generated_at: generated_at.into(),
    vanilla_version: selected_version,
    scanned_files,
    unmatched_files: unmatched,
    files: reports,
  };
  write_json(&directory.join("resource_info.json"), &report)?;
  Ok(report)
}
