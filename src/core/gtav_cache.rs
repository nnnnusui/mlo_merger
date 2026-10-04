//! Ordered vanilla YMAP additions and semantic diff reports extracted from GTA V RPF archives.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
  codewalker::CodeWalker,
  format::{
    gamefile::resource_convert::{NativeResourceFormat, resource_to_xml},
    ymap::{model::Ymap, xml::XmlYmap},
  },
  merge::YmapDiff,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct LogSink {
  file: BufWriter<fs::File>,
  error: Option<std::io::Error>,
}

thread_local! {
  static VERSION_LOG: RefCell<Option<LogSink>> = const { RefCell::new(None) };
}

struct VersionLogger;

impl log::Log for VersionLogger {
  fn enabled(
    &self,
    metadata: &log::Metadata<'_>,
  ) -> bool {
    metadata.level() <= log::Level::Debug
  }

  fn log(
    &self,
    record: &log::Record<'_>,
  ) {
    if !self.enabled(record.metadata()) {
      return;
    }
    VERSION_LOG.with_borrow_mut(|sink| {
      if let Some(sink) = sink
        && let Err(error) = writeln!(
          sink.file,
          "{} [{}] {}: {}",
          chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
          record.level(),
          record.target(),
          record.args()
        )
      {
        sink.error = Some(error);
      }
    });
  }

  fn flush(&self) {
    VERSION_LOG.with_borrow_mut(|sink| {
      if let Some(sink) = sink
        && let Err(error) = sink.file.flush()
      {
        sink.error = Some(error);
      }
    });
  }
}

impl simplelog::SharedLogger for VersionLogger {
  fn level(&self) -> log::LevelFilter {
    log::LevelFilter::Debug
  }
  fn config(&self) -> Option<&simplelog::Config> {
    None
  }
  fn as_log(self: Box<Self>) -> Box<dyn log::Log> {
    self
  }
}

/// Adds per-thread cache log routing to an application's existing CombinedLogger.
///
/// ```no_run
/// simplelog::CombinedLogger::init(vec![mlo_merger::core::gtav_cache::version_logger()])?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn version_logger() -> Box<dyn simplelog::SharedLogger> {
  Box::new(VersionLogger)
}

struct VersionLog;

impl VersionLog {
  fn start(directory: &Path) -> Result<Self> {
    let file = fs::File::create(directory.join("create_cache.log"))?;
    VERSION_LOG.with_borrow_mut(|sink| {
      *sink = Some(LogSink {
        file: BufWriter::new(file),
        error: None,
      });
    });
    Ok(Self)
  }

  fn finish(&self) -> Result<()> {
    VERSION_LOG.with_borrow_mut(|sink| -> Result<()> {
      if let Some(sink) = sink {
        sink.file.flush()?;
        if let Some(error) = sink.error.take() {
          return Err(error.into());
        }
      }
      Ok(())
    })
  }
}

impl Drop for VersionLog {
  fn drop(&mut self) {
    VERSION_LOG.with_borrow_mut(|sink| {
      *sink = None;
    });
  }
}

/// Builds a vanilla archive cache independently of the MLO merge pipeline.
#[derive(Debug, Clone)]
pub struct BuildGtavCache {
  /// Installed GTA V Legacy directory containing base RPFs and update/.
  pub game_dir: PathBuf,
  /// Cache directory containing cache_info.json and version directories.
  pub output_dir: PathBuf,
}

/// A cache artifact and its source within the game's archive hierarchy.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CachedFile {
  /// SHA-256 of the extracted, standalone native file.
  pub sha256: String,
  /// Cache-relative path to a new native file or a replacement's YmapDiff JSON report.
  pub object: String,
  /// Game-relative archive and entry path, including nested RPFs.
  pub source: String,
}

/// An addition or content replacement relative to the preceding stage.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileChange {
  /// Previous content hash, or None for an added file.
  pub previous_sha256: Option<String>,
  /// New cache artifact and provenance; replacements reference .ymap.diff.json.
  pub file: CachedFile,
}

/// One cumulative vanilla stage, identified by dlclist order rather than build number.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheVersion {
  /// Stable position-prefixed ID, such as 0000-base or 0002-mpbeach.
  pub id: String,
  /// Preceding stage ID; absent for the base snapshot.
  pub parent: Option<String>,
  /// Archives contributing to this stage, in overlay order.
  pub archives: Vec<String>,
  /// Case-insensitive native filenames added or replaced at this stage.
  pub changes: BTreeMap<String, FileChange>,
  /// Number of filenames whose content matched the preceding cumulative stage.
  pub unchanged: usize,
}

/// Ordered delta manifest for a particular installed game's archives.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GtavCacheManifest {
  /// Cache schema version, currently 2.
  pub format_version: u32,
  /// Installed game directory from which this cache was generated.
  pub game_dir: PathBuf,
  /// Base, title update, then DLC stages in dlclist.xml order.
  pub versions: Vec<CacheVersion>,
}

impl GtavCacheManifest {
  /// Resolves the latest artifact per filename at a stage, retaining unchanged parent entries.
  /// Replacement artifacts are diff reports, not standalone native binaries.
  ///
  /// ```
  /// # use mlo_merger::core::gtav_cache::GtavCacheManifest;
  /// # let cache = GtavCacheManifest { format_version: 2, game_dir: ".".into(), versions: vec![] };
  /// assert!(cache.resolve_version("unknown").is_err());
  /// ```
  pub fn resolve_version(
    &self,
    id: &str,
  ) -> Result<BTreeMap<String, CachedFile>> {
    if self.format_version != 2 {
      return Err(format!("Unsupported GTA V cache schema {}", self.format_version).into());
    }
    let mut files = BTreeMap::new();
    for version in &self.versions {
      for (name, change) in &version.changes {
        files.insert(name.clone(), change.file.clone());
      }
      if version.id == id {
        return Ok(files);
      }
    }
    Err(format!("Unknown vanilla stage: {id}").into())
  }
}

#[derive(Debug, Clone, Deserialize)]
struct ExtractedFile {
  name: String,
  source: String,
  stored: String,
  sha256: String,
}

#[derive(Deserialize)]
struct DlcList {
  #[serde(rename = "Paths")]
  paths: DlcPaths,
}

#[derive(Deserialize)]
struct DlcPaths {
  #[serde(rename = "Item", default)]
  items: Vec<String>,
}

fn dlc_archive(item: &str) -> Result<PathBuf> {
  let normalized = item.trim().replace('\\', "/").to_ascii_lowercase();
  let (mount, path) = normalized.split_once(':').ok_or("DLC path has no mount prefix")?;
  let root = match mount {
    "dlcpacks" => "update/x64/dlcpacks",
    "platform" => "x64",
    _ => return Err(format!("Unsupported DLC mount: {item}").into()),
  };
  let path = path.trim_matches('/');
  if path.is_empty() || path.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
    return Err(format!("Invalid DLC path: {item}").into());
  }
  Ok(Path::new(root).join(path).join("dlc.rpf"))
}

fn game_path(
  root: &Path,
  relative: &Path,
) -> Result<PathBuf> {
  let mut path = root.to_path_buf();
  for component in relative.iter() {
    let name = component.to_str().ok_or("Game path is not UTF-8")?;
    let matches = fs::read_dir(&path)?
      .map(|entry| entry.map(|entry| entry.path()))
      .collect::<std::io::Result<Vec<_>>>()?
      .into_iter()
      .filter(|candidate| {
        candidate
          .file_name()
          .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case(name))
      })
      .collect::<Vec<_>>();
    if matches.len() != 1 {
      return Err(
        format!("Missing or ambiguous game archive path: {}", path.join(name).display()).into(),
      );
    }
    path = matches[0].clone();
  }
  Ok(path)
}

struct Staging(PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

fn extract(
  codewalker: &CodeWalker,
  game_dir: &Path,
  relative: &str,
  output: &Path,
  subtree: Option<&str>,
) -> Result<Vec<ExtractedFile>> {
  log::info!("Scanning {relative}");
  let archive = game_path(game_dir, Path::new(relative))?;
  match subtree {
    Some(subtree) => codewalker.extract_rpf_subtree(&archive, subtree, output)?,
    None => codewalker.extract_rpf(&archive, output)?,
  }
  let mut files: Vec<ExtractedFile> =
    serde_json::from_reader(BufReader::new(fs::File::open(output.join("files.json"))?))?;
  for file in &mut files {
    let (_, entry) = file.source.split_once('/').ok_or("Invalid extracted RPF entry path")?;
    file.source = format!("{relative}/{entry}");
  }
  Ok(files)
}

fn read_ymap(path: &Path) -> Result<Ymap> {
  let xml = resource_to_xml(NativeResourceFormat::Ymap, &fs::read(path)?, &HashMap::new())?;
  let xml: XmlYmap = quick_xml::de::from_str(&xml)?;
  Ok(xml.into())
}

fn write_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let mut writer = BufWriter::new(fs::File::create(path)?);
  serde_json::to_writer_pretty(&mut writer, value)?;
  writer.flush()?;
  Ok(())
}

fn stage(
  manifest: &mut GtavCacheManifest,
  current: &mut BTreeMap<String, CachedFile>,
  output: &Path,
  label: &str,
  files: impl FnOnce() -> Result<(Vec<String>, Vec<(ExtractedFile, PathBuf)>)>,
  reader: &impl Fn(&Path) -> Result<Ymap>,
) -> Result<()> {
  let id = format!("{:04}-{label}", manifest.versions.len());
  let directory = output.join(&id);
  fs::create_dir(&directory)?;
  fs::create_dir(directory.join("ymap"))?;
  fs::create_dir_all(output.join(".working"))?;
  let version_log = VersionLog::start(&directory)?;
  let result = (|| -> Result<()> {
    log::info!("Creating cache {id}");
    let (archives, files) = files()?;
    log::info!("Processing archives: {}", archives.join(", "));
    let mut winners = BTreeMap::new();
    for (file, directory) in files {
      if file.name.ends_with(".ymap") {
        if file.name.contains(['/', '\\']) || file.name == ".ymap" {
          return Err(format!("Invalid YMAP basename: {}", file.name).into());
        }
        if let Some((previous, _)) = winners.get(&file.name) {
          let previous: &ExtractedFile = previous;
          if previous.sha256 != file.sha256 {
            log::warn!(
              "Within-stage override {}: {} -> {}",
              file.name,
              previous.source,
              file.source
            );
          }
        }
        winners.insert(file.name.clone(), (file, directory));
      }
    }
    let mut changes = BTreeMap::new();
    let mut unchanged = 0;
    for (name, (file, directory)) in winners {
      let previous = current.get(&name);
      if previous.is_some_and(|previous| previous.sha256 == file.sha256) {
        unchanged += 1;
        log::debug!("Unchanged {name}: {}", file.source);
        continue;
      }
      let input = directory.join(&file.stored);
      let working = output.join(".working").join(&name);
      let object = if previous.is_some() {
        format!("{id}/ymap/{name}.diff.json")
      } else {
        format!("{id}/ymap/{name}")
      };
      let destination = output.join(&object);
      if let Some(previous) = previous {
        log::info!("Diff {name}: {} -> {}", previous.source, file.source);
        let before =
          reader(&working).map_err(|error| format!("Reading previous {name}: {error}"))?;
        let after =
          reader(&input).map_err(|error| format!("Reading replacement {name}: {error}"))?;
        let diff = YmapDiff::extract_from(&before, &after);
        write_json(&destination, &diff)?;
      } else {
        log::info!("Added {name}: {}", file.source);
        fs::copy(&input, &destination)?;
      }
      fs::copy(&input, &working)?;
      let cached = CachedFile {
        sha256: file.sha256,
        object,
        source: file.source,
      };
      changes.insert(
        name.clone(),
        FileChange {
          previous_sha256: previous.map(|previous| previous.sha256.clone()),
          file: cached.clone(),
        },
      );
      current.insert(name, cached);
    }
    log::info!(
      "{id}: {} additions/replacements, {unchanged} unchanged, {} effective YMAPs",
      changes.len(),
      current.len()
    );
    let version = CacheVersion {
      id,
      parent: manifest.versions.last().map(|version| version.id.clone()),
      archives,
      changes,
      unchanged,
    };
    write_json(&directory.join("version_info.json"), &version)?;
    manifest.versions.push(version);
    Ok(())
  })();
  if let Err(error) = &result {
    log::error!("Cache creation failed: {error}");
  }
  version_log.finish()?;
  result
}

fn patch_dlc(source: &str) -> Option<&str> {
  source.split_once("/dlc_patch/")?.1.split('/').next()
}

fn platform_virtual_path(source: &str) -> Option<String> {
  let (archive, path) = source.split_once('/')?;
  if archive.starts_with("x64") && archive.ends_with(".rpf") && path.starts_with("dlcpacks/") {
    Some(format!("x64/{path}"))
  } else {
    None
  }
}

fn platform_archives(
  codewalker: &CodeWalker,
  game_dir: &Path,
  staging: &Path,
) -> Result<BTreeMap<String, (String, String)>> {
  let mut roots = fs::read_dir(game_dir)?
    .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase()))
    .collect::<std::io::Result<Vec<_>>>()?;
  roots.sort();
  let mut archives = BTreeMap::new();
  for relative in roots.into_iter().filter(|name| name.starts_with("x64") && name.ends_with(".rpf"))
  {
    log::info!("Indexing platform archives in {relative}");
    let output = staging.join("archives.json");
    codewalker.list_rpf_paths(&game_path(game_dir, Path::new(&relative))?, &output)?;
    let paths: Vec<String> = serde_json::from_reader(BufReader::new(fs::File::open(output)?))?;
    for subtree in paths {
      if let Some(virtual_path) = platform_virtual_path(&subtree)
        && archives.insert(virtual_path.clone(), (relative.clone(), subtree)).is_some()
      {
        return Err(format!("Ambiguous platform DLC archive: {virtual_path}").into());
      }
    }
  }
  Ok(archives)
}

fn base_archives(game_dir: &Path) -> Result<Vec<String>> {
  let mut archives = Vec::new();
  for entry in fs::read_dir(game_dir)? {
    let entry = entry?;
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    if entry.file_type()?.is_file() && name.ends_with(".rpf") {
      archives.push(name);
    }
  }
  archives.sort();
  if archives.is_empty() {
    return Err("No base RPF archives found in the game directory".into());
  }
  Ok(archives)
}

fn publish_cache(
  build: &Path,
  output: &Path,
  manifest: &GtavCacheManifest,
) -> Result<()> {
  for version in &manifest.versions {
    let destination = output.join(&version.id);
    if destination.exists() {
      let existing: CacheVersion = serde_json::from_reader(BufReader::new(fs::File::open(
        destination.join("version_info.json"),
      )?))?;
      if existing.id != version.id {
        return Err(
          format!("Refusing to replace unrecognized cache directory {}", destination.display())
            .into(),
        );
      }
    }
  }
  let backup = output.join(format!(".backup-{}", chrono::Local::now().format("%Y%m%d-%H%M%S-%f")));
  fs::create_dir(&backup)?;
  let mut published = Vec::new();
  let result = (|| -> Result<()> {
    for version in &manifest.versions {
      let destination = output.join(&version.id);
      let existed = destination.exists();
      if existed {
        fs::rename(&destination, backup.join(&version.id))?;
      }
      published.push((version.id.clone(), existed));
      fs::rename(build.join(&version.id), destination)?;
    }
    fs::rename(build.join("cache_info.json"), output.join("cache_info.json"))?;
    Ok(())
  })();
  if result.is_err() {
    for (id, existed) in published.into_iter().rev() {
      let destination = output.join(&id);
      if destination.exists() {
        fs::rename(&destination, build.join(&id))?;
      }
      if existed {
        fs::rename(backup.join(&id), destination)?;
      }
    }
  }
  if let Err(error) = fs::remove_dir_all(&backup) {
    log::warn!("Could not clean cache backup {}: {error}", backup.display());
  }
  result
}

fn preserve_failed_logs(
  build: &Path,
  output: &Path,
) -> Result<PathBuf> {
  let failure = output.join(format!("failed-{}", chrono::Local::now().format("%Y%m%d-%H%M%S-%f")));
  fs::create_dir(&failure)?;
  for entry in fs::read_dir(build)? {
    let entry = entry?;
    let log = entry.path().join("create_cache.log");
    if log.is_file() {
      let directory = failure.join(entry.file_name());
      fs::create_dir(&directory)?;
      fs::copy(log, directory.join("create_cache.log"))?;
    }
  }
  Ok(failure)
}

impl BuildGtavCache {
  /// Reads all installed root RPFs and ordered DLCs, saving additions, diffs, and version logs.
  /// Register `version_logger()` with the application logger to capture diff log records.
  pub fn run(
    &self,
    codewalker: &CodeWalker,
  ) -> Result<()> {
    let game_dir = self.game_dir.canonicalize()?;
    let roots = base_archives(&game_dir)?;
    game_path(&game_dir, Path::new("update/update.rpf"))?;
    codewalker.load_game_keys(&game_dir)?;
    fs::create_dir_all(&self.output_dir)?;
    let staging_path = self.output_dir.join(format!(".staging-{}", std::process::id()));
    fs::create_dir(&staging_path)?;
    let staging = Staging(staging_path);
    let build = staging.0.join("cache");
    fs::create_dir(&build)?;
    let result = self.build(codewalker, &game_dir, roots, &staging.0, &build);
    if result.is_err() {
      match preserve_failed_logs(&build, &self.output_dir) {
        Ok(path) => log::error!("Preserved failed cache logs at {}", path.display()),
        Err(error) => log::error!("Could not preserve failed cache logs: {error}"),
      }
    }
    result
  }

  fn build(
    &self,
    codewalker: &CodeWalker,
    game_dir: &Path,
    roots: Vec<String>,
    scratch: &Path,
    build: &Path,
  ) -> Result<()> {
    let mut manifest = GtavCacheManifest {
      format_version: 2,
      game_dir: game_dir.into(),
      versions: vec![],
    };
    let mut current = BTreeMap::new();
    let base_dir = scratch.join("base");
    stage(
      &mut manifest,
      &mut current,
      build,
      "base",
      || {
        let mut files = Vec::new();
        for relative in &roots {
          let directory = base_dir.join(relative);
          let extracted = extract(codewalker, game_dir, relative, &directory, None)?;
          files.extend(
            extracted
              .into_iter()
              .filter(|file| !file.source.contains("/dlcpacks/"))
              .map(|file| (file, directory.clone())),
          );
        }
        Ok((roots.clone(), files))
      },
      &read_ymap,
    )?;
    fs::remove_dir_all(&base_dir)?;

    let update_dir = scratch.join("update");
    let mut update = Vec::new();
    let mut dlc_items = Vec::new();
    stage(
      &mut manifest,
      &mut current,
      build,
      "update",
      || {
        update = extract(codewalker, game_dir, "update/update.rpf", &update_dir, None)?;
        let list = update
          .iter()
          .find(|file| file.source == "update/update.rpf/common/data/dlclist.xml")
          .ok_or("update/update.rpf/common/data/dlclist.xml is missing")?;
        let dlcs: DlcList = quick_xml::de::from_reader(std::io::BufReader::new(fs::File::open(
          update_dir.join(&list.stored),
        )?))?;
        dlc_items = dlcs.paths.items;
        Ok((
          vec!["update/update.rpf".into()],
          update
            .iter()
            .filter(|file| patch_dlc(&file.source).is_none())
            .cloned()
            .map(|file| (file, update_dir.clone()))
            .collect(),
        ))
      },
      &read_ymap,
    )?;

    let mut platform_index = None;
    for item in dlc_items {
      let relative = dlc_archive(&item)?.to_string_lossy().replace('\\', "/");
      let label = Path::new(&relative)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or("DLC name is missing")?;
      let directory = scratch.join("dlc");
      stage(
        &mut manifest,
        &mut current,
        build,
        label,
        || {
          let (physical, subtree) =
            if relative.starts_with("x64/") && game_path(game_dir, Path::new(&relative)).is_err() {
              if platform_index.is_none() {
                platform_index = Some(platform_archives(codewalker, game_dir, scratch)?);
              }
              let (physical, subtree) = platform_index
                .as_ref()
                .unwrap()
                .get(&relative)
                .ok_or_else(|| format!("Listed platform DLC archive not found: {item}"))?;
              (physical.clone(), Some(subtree.clone()))
            } else {
              (relative.clone(), None)
            };
          let extracted = extract(codewalker, game_dir, &physical, &directory, subtree.as_deref())?;
          let mut files: Vec<_> =
            extracted.into_iter().map(|file| (file, directory.clone())).collect();
          let patches: Vec<_> =
            update.iter().filter(|file| patch_dlc(&file.source) == Some(label)).cloned().collect();
          let mut archives = vec![subtree.unwrap_or(physical)];
          if !patches.is_empty() {
            archives.push(format!("update/update.rpf/dlc_patch/{label}"));
            files.extend(patches.into_iter().map(|file| (file, update_dir.clone())));
          }
          Ok((archives, files))
        },
        &read_ymap,
      )?;
      fs::remove_dir_all(directory)?;
    }
    write_json(&build.join("cache_info.json"), &manifest)?;
    publish_cache(build, &self.output_dir, &manifest)?;
    log::info!("Saved {} vanilla stages to {}", manifest.versions.len(), self.output_dir.display());
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dlclist_keeps_order_and_normalizes_mounts() {
    let list: DlcList = quick_xml::de::from_str(
      "<SMandatoryPacksData><Paths><Item>dlcpacks:\\MPBeach\\</Item><Unused/><Item>platform:/dlcpacks/mpbusiness/</Item></Paths></SMandatoryPacksData>",
    ).unwrap();
    assert_eq!(
      dlc_archive(&list.paths.items[0]).unwrap(),
      PathBuf::from("update/x64/dlcpacks/mpbeach/dlc.rpf")
    );
    assert_eq!(
      dlc_archive(&list.paths.items[1]).unwrap(),
      PathBuf::from("x64/dlcpacks/mpbusiness/dlc.rpf")
    );
    assert!(dlc_archive("dlcpacks:/../escape/").is_err());
    assert!(dlc_archive("unknown:/mpbeach/").is_err());
  }

  #[test]
  fn stages_store_only_changes_and_resolve_prior_versions() {
    static LOG_INIT: std::sync::Once = std::sync::Once::new();
    LOG_INIT.call_once(|| {
      log::set_boxed_logger(version_logger().as_log()).unwrap();
      log::set_max_level(log::LevelFilter::Debug);
    });
    let root = std::env::temp_dir().join(format!("gtav_cache_test_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _cleanup = Staging(root.clone());
    let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
    )))
    .unwrap();
    let before: Ymap = xml.into();
    let mut after = before.clone();
    after.parent = "changed_parent".into();
    fs::write(root.join("first"), serde_json::to_vec(&before).unwrap()).unwrap();
    fs::write(root.join("second"), serde_json::to_vec(&after).unwrap()).unwrap();
    let mut third = after.clone();
    third.parent = "next_parent".into();
    fs::write(root.join("third"), serde_json::to_vec(&third).unwrap()).unwrap();
    let reader =
      |path: &Path| -> Result<Ymap> { Ok(serde_json::from_reader(fs::File::open(path)?)?) };
    let mut manifest = GtavCacheManifest {
      format_version: 2,
      game_dir: "game".into(),
      versions: vec![],
    };
    let mut current = BTreeMap::new();
    let file = |name: &str, sha256: &str, stored: &str| {
      (
        ExtractedFile {
          name: name.into(),
          source: format!("nested.rpf/{name}"),
          stored: stored.into(),
          sha256: sha256.into(),
        },
        root.clone(),
      )
    };
    stage(
      &mut manifest,
      &mut current,
      &root,
      "base",
      || Ok((vec![], vec![file("a.ymap", "111", "first"), file("b.ymap", "111", "first")])),
      &reader,
    )
    .unwrap();
    stage(
      &mut manifest,
      &mut current,
      &root,
      "dlc",
      || {
        Ok((
          vec![],
          vec![
            file("a.ymap", "222", "second"),
            file("b.ymap", "111", "first"),
            file("c.ymap", "222", "second"),
          ],
        ))
      },
      &reader,
    )
    .unwrap();
    stage(&mut manifest, &mut current, &root, "empty", || Ok((vec![], vec![])), &reader).unwrap();
    assert_eq!(manifest.versions[1].changes.len(), 2);
    assert_eq!(manifest.versions[1].changes["a.ymap"].previous_sha256.as_deref(), Some("111"));
    assert_eq!(manifest.versions[1].changes["c.ymap"].previous_sha256, None);
    assert!(manifest.versions[2].changes.is_empty());
    assert_eq!(manifest.resolve_version("0000-base").unwrap()["a.ymap"].sha256, "111");
    assert_eq!(manifest.resolve_version("0002-empty").unwrap()["a.ymap"].sha256, "222");
    assert_eq!(manifest.resolve_version("0002-empty").unwrap().len(), 3);
    assert!(manifest.resolve_version("unknown").is_err());
    assert_eq!(manifest.versions[1].unchanged, 1);
    assert!(root.join("0000-base/ymap/a.ymap").is_file());
    assert!(root.join("0001-dlc/ymap/a.ymap.diff.json").is_file());
    assert!(!root.join("0001-dlc/ymap/a.ymap").exists());
    assert!(!root.join("0001-dlc/ymap/b.ymap").exists());
    assert!(root.join("0001-dlc/ymap/c.ymap").is_file());
    let diff: YmapDiff =
      serde_json::from_reader(fs::File::open(root.join("0001-dlc/ymap/a.ymap.diff.json")).unwrap())
        .unwrap();
    assert_eq!(diff.apply_to(&before, None).parent, after.parent);
    assert_eq!(
      fs::read(root.join("0001-dlc/ymap/c.ymap")).unwrap(),
      fs::read(root.join("second")).unwrap()
    );
    for version in &manifest.versions {
      assert!(root.join(&version.id).join("version_info.json").is_file());
      assert!(root.join(&version.id).join("create_cache.log").is_file());
      assert!(root.join(&version.id).join("ymap").is_dir());
    }
    let log = fs::read_to_string(root.join("0001-dlc/create_cache.log")).unwrap();
    assert!(log.contains("Diff a.ymap"));
    assert!(log.contains("Added c.ymap"));
    assert!(log.contains("Unchanged b.ymap"));
    assert!(
      !fs::read_to_string(root.join("0002-empty/create_cache.log"))
        .unwrap()
        .contains("Diff a.ymap")
    );
    stage(
      &mut manifest,
      &mut current,
      &root,
      "next",
      || Ok((vec![], vec![file("a.ymap", "333", "third")])),
      &reader,
    )
    .unwrap();
    assert_eq!(manifest.versions[3].changes["a.ymap"].previous_sha256.as_deref(), Some("222"));
    let diff: YmapDiff = serde_json::from_reader(
      fs::File::open(root.join("0003-next/ymap/a.ymap.diff.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(diff.apply_to(&after, None).parent, third.parent);
    let failed = stage(
      &mut manifest,
      &mut current,
      &root,
      "failed",
      || Err("test extraction failure".into()),
      &reader,
    );
    assert!(failed.is_err());
    assert!(
      fs::read_to_string(root.join("0004-failed/create_cache.log"))
        .unwrap()
        .contains("test extraction failure")
    );
    let failures = preserve_failed_logs(&root, &root).unwrap();
    assert!(failures.join("0004-failed/create_cache.log").is_file());
  }

  #[test]
  fn discovers_all_installed_base_archives_without_assuming_a_final_letter() {
    let root = std::env::temp_dir().join(format!("gtav_base_discovery_{}", std::process::id()));
    fs::create_dir_all(root.join("directory.rpf")).unwrap();
    let _cleanup = Staging(root.clone());
    for name in ["x64z.rpf", "x64A.RPF", "common.rpf", "ignored.txt"] {
      fs::write(root.join(name), []).unwrap();
    }
    assert_eq!(base_archives(&root).unwrap(), ["common.rpf", "x64a.rpf", "x64z.rpf"]);
  }

  #[test]
  fn publication_restores_existing_versions_if_root_metadata_cannot_be_replaced() {
    let root = std::env::temp_dir().join(format!("gtav_publish_test_{}", std::process::id()));
    let build = root.join("build");
    let output = root.join("output");
    fs::create_dir_all(build.join("0000-base/ymap")).unwrap();
    fs::create_dir_all(output.join("0000-base/ymap")).unwrap();
    let _cleanup = Staging(root);
    let version = CacheVersion {
      id: "0000-base".into(),
      parent: None,
      archives: vec![],
      changes: BTreeMap::new(),
      unchanged: 0,
    };
    let manifest = GtavCacheManifest {
      format_version: 2,
      game_dir: "game".into(),
      versions: vec![version],
    };
    fs::write(output.join("0000-base/ymap/map.ymap"), b"old").unwrap();
    fs::write(build.join("0000-base/ymap/map.ymap"), b"new").unwrap();
    serde_json::to_writer(
      fs::File::create(output.join("0000-base/version_info.json")).unwrap(),
      &manifest.versions[0],
    )
    .unwrap();
    serde_json::to_writer(fs::File::create(build.join("cache_info.json")).unwrap(), &manifest)
      .unwrap();
    fs::create_dir(output.join("cache_info.json")).unwrap();
    assert!(publish_cache(&build, &output, &manifest).is_err());
    assert_eq!(fs::read(output.join("0000-base/ymap/map.ymap")).unwrap(), b"old");
    assert_eq!(fs::read(build.join("0000-base/ymap/map.ymap")).unwrap(), b"new");
    fs::remove_dir(output.join("cache_info.json")).unwrap();
    publish_cache(&build, &output, &manifest).unwrap();
    assert_eq!(fs::read(output.join("0000-base/ymap/map.ymap")).unwrap(), b"new");
    assert!(output.join("cache_info.json").is_file());
  }

  #[test]
  fn title_update_patches_belong_to_their_dlc_stage() {
    assert_eq!(
      patch_dlc("update/update.rpf/dlc_patch/mpbeach/x64/levels/map.rpf/a.ymap"),
      Some("mpbeach")
    );
    assert_eq!(patch_dlc("update/update.rpf/x64/levels/map.rpf/a.ymap"), None);
    assert_eq!(
      platform_virtual_path("x64w.rpf/dlcpacks/mpbeach/dlc.rpf").as_deref(),
      Some("x64/dlcpacks/mpbeach/dlc.rpf")
    );
    assert_eq!(platform_virtual_path("x64n.rpf/levels/gta5/map.rpf"), None);
  }
}
