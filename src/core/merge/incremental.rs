use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::{BufReader, Read},
  path::{Component, Path, PathBuf},
};

use super::duplicates::{DuplicateReport, EntityDuplicate};
use super::merge::Result;
use super::{run::MergeYmap, ybn_conflicts::MergeYbnConflicts};
use crate::core::{
  format::ymap::diff::reference_hash,
  source_cache::{MergeInputs, MergeSourceFile},
  vanilla::write_json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bump when merge policy, encoding or cached result/report semantics change.
const ALGORITHM_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Fingerprint {
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl Fingerprint {
  pub(super) fn read(
    path: &Path,
    previous: Option<&Self>,
    force: bool,
  ) -> Result<Self> {
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
    if !force
      && let Some(previous) = previous
      && previous.size == metadata.len()
      && previous.modified_seconds == modified.as_secs()
      && previous.modified_nanos == modified.subsec_nanos()
    {
      return Ok(previous.clone());
    }
    let mut reader = BufReader::new(fs::File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
      let count = reader.read(&mut buffer)?;
      if count == 0 {
        break;
      }
      digest.update(&buffer[..count]);
    }
    Ok(Self {
      sha256: format!("{:x}", digest.finalize()),
      size: metadata.len(),
      modified_seconds: modified.as_secs(),
      modified_nanos: modified.subsec_nanos(),
    })
  }

  fn same_content(
    &self,
    other: &Self,
  ) -> bool {
    self.sha256 == other.sha256 && self.size == other.size
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct InputFile {
  pub path: PathBuf,
  pub resource: Option<String>,
  pub original_path: Option<PathBuf>,
  pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct OutputFile {
  pub path: String,
  pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct FileRecord {
  pub merge_sources: Vec<InputFile>,
  pub vanilla: Option<InputFile>,
  pub dependencies: BTreeSet<String>,
  pub dependency_fingerprint: String,
  pub output: Option<OutputFile>,
  pub merged_at: String,
  #[serde(default)]
  pub duplicates: Vec<EntityDuplicate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MergeMetadata {
  pub format_version: u32,
  pub algorithm_version: u32,
  pub generated_at: String,
  pub files: BTreeMap<String, FileRecord>,
}

impl MergeMetadata {
  pub(super) fn load(output: &Path) -> Result<Option<Self>> {
    let path = output.join("merge_cache_info.json");
    if !path.is_file() {
      return Ok(None);
    }
    let metadata: Self = serde_json::from_reader(BufReader::new(fs::File::open(path)?))?;
    Ok(
      (metadata.format_version == 1 && metadata.algorithm_version == ALGORITHM_VERSION)
        .then_some(metadata),
    )
  }
}

pub(super) fn output_path(
  root: &Path,
  relative: &str,
) -> Result<PathBuf> {
  let mut path = root.to_path_buf();
  let relative = Path::new(relative);
  if relative.components().any(|part| !matches!(part, Component::Normal(_))) {
    return Err("Unsafe merge-cache output path".into());
  }
  for part in relative.components() {
    path.push(part);
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Refusing symlink merge output: {}", path.display()).into());
    }
  }
  Ok(path)
}

pub(super) fn reusable(
  record: &FileRecord,
  signature: &str,
  output: &Path,
  force: bool,
) -> Result<bool> {
  if force || record.dependency_fingerprint != signature {
    return Ok(false);
  }
  let Some(file) = &record.output else {
    return Ok(true);
  };
  let path = output_path(output, &file.path)?;
  if !path.is_file() {
    return Ok(false);
  }
  Ok(Fingerprint::read(&path, Some(&file.fingerprint), false)?.same_content(&file.fingerprint))
}

pub(super) struct IncrementalMerge<'a> {
  pub source_dir: &'a Path,
  pub vanilla_cache: &'a Path,
  pub source_cache: &'a Path,
  pub output: &'a Path,
  pub staging: &'a Path,
  pub ybn: &'a [PathBuf],
  pub ymap: &'a [PathBuf],
  pub inputs: &'a MergeInputs,
  pub force: bool,
}

struct Group {
  extension: &'static str,
  names: BTreeSet<String>,
}

#[derive(Deserialize)]
struct Relationships {
  children_by_parent_hash: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct SourceParents {
  resources: BTreeMap<String, ParentResource>,
}

#[derive(Deserialize)]
struct ParentResource {
  files_by_format: BTreeMap<String, Vec<ParentFile>>,
}

#[derive(Deserialize)]
struct ParentFile {
  file_name: String,
  ymap_parent_hash: Option<String>,
}

impl IncrementalMerge<'_> {
  /// Rebuilds dependency groups while retaining unchanged outputs and negative results.
  pub(super) fn run(&self) -> Result<bool> {
    let previous = MergeMetadata::load(self.output)?;
    let mut previous_inputs = BTreeMap::new();
    if let Some(metadata) = &previous {
      for record in metadata.files.values() {
        for input in record.merge_sources.iter().chain(record.vanilla.iter()) {
          previous_inputs.insert(input.path.clone(), &input.fingerprint);
        }
      }
    }
    let baselines = self
      .ybn
      .iter()
      .chain(self.ymap)
      .map(|path| Ok((file_name(path)?, path)))
      .collect::<Result<BTreeMap<_, _>>>()?;
    let mut sources = BTreeMap::<String, Vec<&MergeSourceFile>>::new();
    for source in self.inputs.ybn.iter().chain(&self.inputs.ymap) {
      sources.entry(source.file_name.to_ascii_lowercase()).or_default().push(source);
    }
    let graph = self.ymap_graph()?;
    let groups = groups(&sources, &graph);
    let mut files = BTreeMap::new();
    let mut omitted = BTreeSet::new();
    let now = chrono::Utc::now().to_rfc3339();
    let mut rebuilt = 0;
    let mut reused = 0;
    let mut retained_files = Vec::new();
    for group in groups {
      let mut planned = BTreeMap::new();
      for name in &group.names {
        let merge_sources = sources
          .get(name)
          .into_iter()
          .flatten()
          .map(|source| {
            Ok(InputFile {
              path: source.path.clone(),
              resource: Some(source.resource.clone()),
              original_path: Some(source.original_path.clone()),
              fingerprint: Fingerprint::read(
                &source.path,
                previous_inputs.get(&source.path).copied(),
                self.force,
              )?,
            })
          })
          .collect::<Result<Vec<_>>>()?;
        let vanilla = baselines
          .get(name)
          .map(|path| {
            Ok::<_, Box<dyn std::error::Error>>(InputFile {
              path: (*path).clone(),
              resource: None,
              original_path: None,
              fingerprint: Fingerprint::read(
                path,
                previous_inputs.get(*path).copied(),
                self.force,
              )?,
            })
          })
          .transpose()?;
        planned.insert(
          name.clone(),
          FileRecord {
            merge_sources,
            vanilla,
            dependencies: graph.get(name).cloned().unwrap_or_default(),
            dependency_fingerprint: String::new(),
            output: None,
            merged_at: now.clone(),
            duplicates: Vec::new(),
          },
        );
      }
      let signature = signature(&planned)?;
      let mut current = !self.force;
      for name in &group.names {
        let Some(old) = previous.as_ref().and_then(|metadata| metadata.files.get(name)) else {
          current = false;
          break;
        };
        if let Some(output) = &old.output {
          let path = Path::new(&output.path);
          if path.parent() != Some(Path::new(group.extension)) || file_name(path)? != *name {
            current = false;
            break;
          }
        } else if self.output.join(group.extension).join(name).exists() {
          current = false;
          break;
        }
        if !reusable(old, &signature, self.output, false)? {
          current = false;
          break;
        }
      }
      if current {
        reused += 1;
        for (name, record) in &mut planned {
          let old = &previous.as_ref().ok_or("Merge cache missing")?.files[name];
          record.merged_at = old.merged_at.clone();
          record.duplicates = old.duplicates.clone();
          if let Some(output) = &old.output {
            let from = output_path(self.output, &output.path)?;
            let to = output_path(self.staging, &output.path)?;
            retained_files.push((from.clone(), to));
            record.output = Some(OutputFile {
              path: output.path.clone(),
              fingerprint: Fingerprint::read(&from, Some(&output.fingerprint), false)?,
            });
          }
        }
        log::info!("Reusing {} merge group ({} files)", group.extension, group.names.len());
      } else {
        rebuilt += 1;
        log::info!("Updating {} merge group ({} files)", group.extension, group.names.len());
        let duplicates = self.merge_group(&group, &sources, &baselines)?;
        for (name, entries) in duplicates.files {
          if let Some(record) = planned.get_mut(&name) {
            record.duplicates = entries;
          }
        }
        let directory = self.staging.join(group.extension);
        if directory.is_dir() {
          for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if !path.is_file() {
              continue;
            }
            if let Some(record) = planned.get_mut(&file_name(&path)?) {
              record.output = Some(OutputFile {
                path: path.strip_prefix(self.staging)?.to_string_lossy().replace('\\', "/"),
                fingerprint: Fingerprint::read(&path, None, true)?,
              });
            }
          }
        }
      }
      for record in planned.values_mut() {
        record.dependency_fingerprint = signature.clone();
        if record.output.is_some() {
          for input in &record.merge_sources {
            if let Some(original) = &input.original_path {
              omitted.insert(
                original.strip_prefix(self.source_dir)?.to_string_lossy().replace('\\', "/"),
              );
            }
          }
        }
      }
      files.extend(planned);
    }
    let omit_text = omitted.into_iter().collect::<Vec<_>>().join("\n");
    let duplicates = DuplicateReport {
      format_version: 1,
      files: files
        .iter()
        .filter(|(_, record)| !record.duplicates.is_empty())
        .map(|(name, record)| (name.clone(), record.duplicates.clone()))
        .collect(),
    };
    let duplicate_path = output_path(self.output, "duplicates.json")?;
    let duplicate_current = fs::File::open(&duplicate_path)
      .ok()
      .and_then(|file| serde_json::from_reader::<_, DuplicateReport>(BufReader::new(file)).ok())
      .is_some_and(|previous| previous == duplicates);
    if rebuilt == 0
      && !self.force
      && previous.as_ref().is_some_and(|previous| previous.files == files)
      && duplicate_current
      && fs::read_to_string(self.output.join("_omit.txt")).ok().as_deref() == Some(&omit_text)
    {
      log::info!("Merge output is current; nothing to do");
      return Ok(false);
    }
    for (from, to) in retained_files {
      retain_file(&from, &to)?;
    }
    fs::write(self.staging.join("_omit.txt"), omit_text)?;
    write_json(&self.staging.join("duplicates.json"), &duplicates)?;
    write_json(
      &self.staging.join("merge_cache_info.json"),
      &MergeMetadata {
        format_version: 1,
        algorithm_version: ALGORITHM_VERSION,
        generated_at: now,
        files,
      },
    )?;
    log::info!("Incremental merge: {rebuilt} groups rebuilt, {reused} groups reused");
    Ok(true)
  }

  fn merge_group(
    &self,
    group: &Group,
    sources: &BTreeMap<String, Vec<&MergeSourceFile>>,
    baselines: &BTreeMap<String, &PathBuf>,
  ) -> Result<DuplicateReport> {
    let vanilla = group
      .names
      .iter()
      .filter_map(|name| baselines.get(name).map(|path| (*path).clone()))
      .collect::<Vec<_>>();
    let sources = group
      .names
      .iter()
      .flat_map(|name| sources.get(name).into_iter().flatten().copied())
      .collect::<Vec<_>>();
    if group.extension == "ybn" {
      let omit = self.staging.join(".ybn_omit.txt");
      MergeYbnConflicts {
        source_dir: self.source_cache.to_path_buf(),
        vanilla_dir: self.vanilla_cache.join("latest/ybn"),
        output_dir: self.staging.to_path_buf(),
        omitted_files_path: omit.clone(),
      }
      .run_with_latest_vanilla_files(
        None,
        &vanilla,
        Some(&sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>()),
      )?;
      let _ = fs::remove_file(omit);
      for entry in fs::read_dir(self.staging)? {
        let path = entry?.path();
        if path.is_file()
          && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ybn"))
        {
          let directory = self.staging.join("ybn");
          fs::create_dir_all(&directory)?;
          fs::rename(&path, directory.join(path.file_name().ok_or("Merged file name missing")?))?;
        }
      }
    } else if !sources.is_empty() {
      let directory = self.staging.join("ymap");
      let sources = sources
        .iter()
        .map(|source| (source.resource.clone(), source.path.clone()))
        .collect::<Vec<_>>();
      MergeYmap {
        vanilla_dir: self.vanilla_cache.join("latest/ymap"),
        mod_dir: self.source_dir.to_path_buf(),
        mod_ymap_dir: self.source_dir.to_path_buf(),
        output_dir: directory.clone(),
        rebuild_all: true,
        blacklist_config: None,
      }
      .run_with_latest_vanilla_files(&vanilla, Some(&sources))?;
      let duplicate_path = directory.join(".duplicates.json");
      let duplicates: DuplicateReport =
        serde_json::from_reader(BufReader::new(fs::File::open(&duplicate_path)?))?;
      fs::remove_file(duplicate_path)?;
      let _ = fs::remove_file(directory.join("_copy_targets.txt"));
      let _ = fs::remove_file(directory.join("_managed_ymaps.txt"));
      let _ = fs::remove_dir_all(directory.join("clone"));
      return Ok(duplicates);
    }
    Ok(DuplicateReport {
      format_version: 1,
      files: BTreeMap::new(),
    })
  }

  fn ymap_graph(&self) -> Result<BTreeMap<String, BTreeSet<String>>> {
    if self.inputs.ymap.is_empty() {
      return Ok(BTreeMap::new());
    }
    let mut hashes = BTreeMap::new();
    let mut graph = BTreeMap::new();
    for path in self.ymap {
      let name = file_name(path)?;
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if hashes.insert(hash, name.clone()).is_some() {
        return Err("Ambiguous vanilla YMAP hash in merge dependencies".into());
      }
      graph.insert(name, BTreeSet::new());
    }
    let relationships: Relationships = serde_json::from_reader(BufReader::new(fs::File::open(
      self.vanilla_cache.join("ymap_relationships.json"),
    )?))?;
    for (parent_hash, children) in relationships.children_by_parent_hash {
      if let Ok(hash) = u32::from_str_radix(&parent_hash, 16)
        && let Some(parent) = hashes.get(&hash)
      {
        for child in children {
          connect(&mut graph, parent, &child.to_ascii_lowercase());
        }
      }
    }
    let metadata: SourceParents = serde_json::from_reader(BufReader::new(fs::File::open(
      self.source_cache.join("source_cache_info.json"),
    )?))?;
    for resource in metadata.resources.values() {
      for source in resource.files_by_format.get(".ymap").into_iter().flatten() {
        if let Some(hash) = &source.ymap_parent_hash
          && let Ok(hash) = u32::from_str_radix(hash, 16)
          && let Some(parent) = hashes.get(&hash)
        {
          connect(&mut graph, parent, &source.file_name.to_ascii_lowercase());
        }
      }
    }
    Ok(graph)
  }
}

fn connect(
  graph: &mut BTreeMap<String, BTreeSet<String>>,
  parent: &str,
  child: &str,
) {
  if parent == child || !graph.contains_key(parent) || !graph.contains_key(child) {
    return;
  }
  graph.get_mut(parent).unwrap().insert(child.into());
  graph.get_mut(child).unwrap().insert(parent.into());
}

fn groups(
  sources: &BTreeMap<String, Vec<&MergeSourceFile>>,
  graph: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Group> {
  let mut visited = BTreeSet::new();
  let mut groups = Vec::new();
  for name in sources.keys() {
    if !visited.insert(name.clone()) {
      continue;
    }
    let extension = if name.ends_with(".ybn") { "ybn" } else { "ymap" };
    let mut names = BTreeSet::from([name.clone()]);
    let mut pending = vec![name.clone()];
    while let Some(current) = pending.pop() {
      for neighbor in graph.get(&current).into_iter().flatten() {
        if visited.insert(neighbor.clone()) {
          names.insert(neighbor.clone());
          pending.push(neighbor.clone());
        }
      }
    }
    groups.push(Group {
      extension,
      names,
    });
  }
  groups
}

fn signature(files: &BTreeMap<String, FileRecord>) -> Result<String> {
  let inputs = files
    .iter()
    .map(|(name, record)| {
      let inputs = record
        .merge_sources
        .iter()
        .chain(record.vanilla.iter())
        .map(|input| {
          serde_json::json!({
            "path": input.path, "resource": input.resource, "original_path": input.original_path,
            "sha256": input.fingerprint.sha256, "size": input.fingerprint.size,
          })
        })
        .collect::<Vec<_>>();
      (name, serde_json::json!({"dependencies": record.dependencies, "inputs": inputs}))
    })
    .collect::<BTreeMap<_, _>>();
  Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&(ALGORITHM_VERSION, inputs))?)))
}

fn file_name(path: &Path) -> Result<String> {
  Ok(
    path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or("Merge file name is not UTF-8")?
      .to_ascii_lowercase(),
  )
}

fn retain_file(
  from: &Path,
  to: &Path,
) -> Result<()> {
  fs::create_dir_all(to.parent().ok_or("Merge file has no parent")?)?;
  if fs::hard_link(from, to).is_err() {
    fs::copy(from, to)?;
    fs::File::options()
      .write(true)
      .open(to)?
      .set_times(fs::FileTimes::new().set_modified(fs::metadata(from)?.modified()?))?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn merge_cache_checks_content_outputs_and_noop_results() {
    let root = std::env::temp_dir().join(format!("merge_cache_check_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("ybn")).unwrap();
    let path = root.join("ybn/map.ybn");
    fs::write(&path, b"merged").unwrap();
    let fingerprint = Fingerprint::read(&path, None, false).unwrap();
    let mut record = FileRecord {
      merge_sources: vec![],
      vanilla: None,
      dependencies: BTreeSet::new(),
      dependency_fingerprint: "signature".into(),
      output: Some(OutputFile {
        path: "ybn/map.ybn".into(),
        fingerprint: fingerprint.clone(),
      }),
      merged_at: "original".into(),
      duplicates: Vec::new(),
    };
    assert!(reusable(&record, "signature", &root, false).unwrap());
    assert!(!reusable(&record, "changed", &root, false).unwrap());
    assert!(!reusable(&record, "signature", &root, true).unwrap());
    fs::File::options()
      .write(true)
      .open(&path)
      .unwrap()
      .set_times(
        fs::FileTimes::new()
          .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(10)),
      )
      .unwrap();
    assert!(
      Fingerprint::read(&path, Some(&fingerprint), false).unwrap().same_content(&fingerprint)
    );
    assert!(reusable(&record, "signature", &root, false).unwrap());
    fs::write(&path, b"broken content").unwrap();
    assert!(!reusable(&record, "signature", &root, false).unwrap());
    fs::remove_file(&path).unwrap();
    assert!(!reusable(&record, "signature", &root, false).unwrap());
    record.output = None;
    assert!(reusable(&record, "signature", &root, false).unwrap());
    assert!(output_path(&root, "../outside").is_err());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn dependency_groups_invalidate_connected_maps_but_not_independent_maps() {
    let graph = BTreeMap::from([
      ("parent.ymap".into(), BTreeSet::from(["child.ymap".into()])),
      ("child.ymap".into(), BTreeSet::from(["parent.ymap".into()])),
      ("independent.ymap".into(), BTreeSet::new()),
    ]);
    let source = |name: &str| MergeSourceFile {
      resource: "resource".into(),
      path: PathBuf::from(name),
      original_path: PathBuf::from(name),
      file_name: name.into(),
    };
    let parent = source("parent.ymap");
    let independent = source("independent.ymap");
    let sources = BTreeMap::from([
      ("parent.ymap".into(), vec![&parent]),
      ("independent.ymap".into(), vec![&independent]),
    ]);
    let groups = groups(&sources, &graph);
    assert_eq!(groups.len(), 2);
    assert_eq!(
      groups.iter().find(|group| group.names.contains("parent.ymap")).unwrap().names,
      BTreeSet::from(["parent.ymap".into(), "child.ymap".into()])
    );
    let record = |name: &str| FileRecord {
      merge_sources: vec![],
      vanilla: Some(InputFile {
        path: PathBuf::from(name),
        resource: None,
        original_path: None,
        fingerprint: Fingerprint {
          sha256: name.into(),
          size: 10,
          modified_seconds: 20,
          modified_nanos: 0,
        },
      }),
      dependencies: graph[name].clone(),
      dependency_fingerprint: String::new(),
      output: None,
      merged_at: "cached".into(),
      duplicates: Vec::new(),
    };
    let mut connected = BTreeMap::from([
      ("parent.ymap".into(), record("parent.ymap")),
      ("child.ymap".into(), record("child.ymap")),
    ]);
    let independent = BTreeMap::from([("independent.ymap".into(), record("independent.ymap"))]);
    let original = signature(&connected).unwrap();
    let other = signature(&independent).unwrap();
    connected
      .get_mut("child.ymap")
      .unwrap()
      .vanilla
      .as_mut()
      .unwrap()
      .fingerprint
      .modified_seconds += 1;
    assert_eq!(signature(&connected).unwrap(), original);
    connected.get_mut("child.ymap").unwrap().vanilla.as_mut().unwrap().fingerprint.sha256 =
      "changed".into();
    assert_ne!(signature(&connected).unwrap(), original);
    assert_eq!(signature(&independent).unwrap(), other);
    connected.get_mut("child.ymap").unwrap().vanilla.as_mut().unwrap().fingerprint.sha256 =
      "child.ymap".into();
    connected.get_mut("child.ymap").unwrap().dependencies.clear();
    assert_ne!(signature(&connected).unwrap(), original);
  }
}
