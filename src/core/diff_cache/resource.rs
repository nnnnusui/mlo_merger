//! Resource stream scanning, version inference and final diff reports.

use super::{
  Result,
  comparison::{ModelState, best_candidate, distance},
  io::{cache_path, content_hash},
  types::{CandidateScore, FileReport, ResourceReport, Variant},
  vanilla::NativeVariants,
};
use crate::core::{
  format::ybn::{
    diff::YbnDiff,
    model::Bound,
    read_ybn,
    xml::{xml_to_ybn, ybn_to_xml},
  },
  format::ymap::diff::YmapDiff,
  vanilla_cache::{VanillaCacheManifest, write_json},
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
  model: ResourceModel,
  variants: Vec<Variant>,
  best: usize,
  scores: Vec<CandidateScore>,
}

enum ResourceModel {
  Ymap(ModelState),
  Ybn(Bound),
}

fn history_model(bytes: &[u8]) -> Result<Bound> {
  let model = read_ybn(bytes)?;
  let stable = (|| -> Result<Vec<u8>> {
    let source_xml = ybn_to_xml(bytes)?;
    let first_binary = xml_to_ybn(&source_xml)?;
    let first_xml = ybn_to_xml(&first_binary)?;
    let second_binary = xml_to_ybn(&first_xml)?;
    let second_xml = ybn_to_xml(&second_binary)?;
    if first_binary != second_binary || first_xml != second_xml {
      return Err("YBN XML/binary conversion is not stable".into());
    }
    Ok(second_binary)
  })();
  match stable {
    Ok(binary) => Ok(read_ybn(&binary)?),
    Err(_) => Ok(model),
  }
}

fn ybn_distance(
  before: &Bound,
  after: &Bound,
) -> Result<usize> {
  match YbnDiff::extract_from(before, after) {
    Ok(diff) => Ok(diff.bound_diffs.len() + diff.polygon_diffs.len()),
    Err(_) => Ok(distance(&serde_json::to_value(before)?, &serde_json::to_value(after)?)),
  }
}

pub(super) fn generate_resource(
  resource: &Path,
  id: &str,
  generated_at: &str,
  output: &Path,
  manifest: &VanillaCacheManifest,
  histories: &BTreeMap<String, Vec<Variant>>,
  provider: &mut NativeVariants<'_>,
) -> Result<ResourceReport> {
  log::info!("Inferring vanilla version for {id}");
  let files = stream_files(resource)?;
  let scanned_files = files.len();
  let mut unmatched = Vec::new();
  let mut unsupported = Vec::new();
  let mut matched = Vec::new();
  'files: for source in files {
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
    let model = if name.ends_with(".ymap") {
      ResourceModel::Ymap(ModelState::new(
        (provider.reader)(&source)
          .map_err(|error| format!("Reading MLO {}: {error}", source.display()))?,
      )?)
    } else if name.ends_with(".ybn") {
      match history_model(&fs::read(&source)?) {
        Ok(model) => ResourceModel::Ybn(model),
        Err(error) => {
          log::warn!("Skipping undecodable YBN {id}/{relative}: {error}");
          unsupported.push(relative);
          continue;
        }
      }
    } else {
      log::warn!("Skipping unsupported MLO diff type: {id}/{relative}");
      unsupported.push(relative);
      continue;
    };
    let mut scores = Vec::new();
    for variant in variants {
      let difference_count = match &model {
        ResourceModel::Ymap(model) => {
          let vanilla = provider.model(&variant.file)?;
          distance(&vanilla.comparison, &model.comparison)
        }
        ResourceModel::Ybn(model) => {
          let vanilla = match provider.ybn_model(&variant.file) {
            Ok(model) => model,
            Err(error) => {
              log::warn!("Skipping YBN with undecodable vanilla variant {id}/{relative}: {error}");
              unsupported.push(relative.clone());
              continue 'files;
            }
          };
          ybn_distance(&vanilla, model)?
        }
      };
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
    log::warn!("No vanilla-matching YMAPs or YBNs in {id}");
  }
  let directory = cache_path(output, id)?;
  fs::create_dir_all(directory.join("ymap"))?;
  fs::create_dir_all(directory.join("ybn"))?;
  let mut reports = Vec::new();
  'reports: for file in matched {
    let baseline = &file.variants[file.best];
    log::info!(
      "Generating diff {} against content from {}",
      file.source.display(),
      baseline.version
    );
    let (family, difference_count) = match &file.model {
      ResourceModel::Ymap(model) => {
        let vanilla = provider.model(&baseline.file)?;
        let diff = YmapDiff::extract_from(&vanilla.model, &model.model);
        let difference_count = distance(&vanilla.comparison, &model.comparison);
        let artifact = format!("ymap/{}.diff.json", file.relative);
        let destination = cache_path(&directory, &artifact)?;
        fs::create_dir_all(destination.parent().ok_or("Diff artifact has no parent")?)?;
        write_json(&destination, &diff)?;
        ("ymap", difference_count)
      }
      ResourceModel::Ybn(model) => {
        let vanilla = provider.ybn_model(&baseline.file)?;
        let diff = match YbnDiff::extract_from(&vanilla, model) {
          Ok(diff) => diff,
          Err(error) => {
            log::warn!("Skipping unsupported YBN diff {id}/{}: {error}", file.relative);
            unsupported.push(file.relative.clone());
            continue 'reports;
          }
        };
        let difference_count = diff.bound_diffs.len() + diff.polygon_diffs.len();
        let artifact = format!("ybn/{}.diff.json", file.relative);
        let destination = cache_path(&directory, &artifact)?;
        fs::create_dir_all(destination.parent().ok_or("Diff artifact has no parent")?)?;
        write_json(&destination, &diff)?;
        ("ybn", difference_count)
      }
    };
    let artifact = format!("{family}/{}.diff.json", file.relative);
    reports.push(FileReport {
      input: file.relative,
      input_sha256: file.sha256,
      best_version: file.variants[file.best].version.clone(),
      best_difference_count: file.scores[file.best].difference_count,
      candidates: file.scores,
      baseline_content_version: baseline.version.clone(),
      baseline_sha256: baseline.file.sha256.clone(),
      baseline_source: baseline.file.source.clone(),
      difference_count,
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
    unsupported_files: unsupported,
    files: reports,
  };
  write_json(&directory.join("resource_info.json"), &report)?;
  Ok(report)
}
