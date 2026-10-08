use crate::core::vanilla::write_json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::{BufReader, Read, Write},
  path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Copies generated merged files and cache files not replaced by a merged basename.
#[derive(Debug, Clone)]
pub struct Deploy {
  /// Existing merged output directory.
  pub merged_dir: PathBuf,
  /// Existing source cache containing `resources/`.
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "DeployFingerprint"))]
struct Fingerprint {
  sha256: String,
  size: u64,
  modified_seconds: u64,
  modified_nanos: u32,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct CopiedFile {
  source: PathBuf,
  input: Fingerprint,
  output: Fingerprint,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct DeployMetadata {
  format_version: u32,
  merged_dir: PathBuf,
  source_cache_dir: PathBuf,
  generated_at: String,
  files: BTreeMap<String, CopiedFile>,
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
    let merged = self.merged_dir.canonicalize()?;
    let cache = self.source_cache_dir.canonicalize()?;
    let output = prepare_output(&self.output_dir, &[&merged, &cache])?;
    let planned = plan(&merged, &cache)?;
    let moved_files = crate::core::source_cache::load_moved_source_paths(&cache)?
      .into_iter()
      .collect::<Vec<_>>()
      .join("\n");
    let moved_files_path = destination(&output, "files.txt")?;
    let metadata_path = destination(&output, "deploy_cache_info.json")?;
    let previous = if metadata_path.is_file() {
      let metadata: DeployMetadata =
        serde_json::from_reader(BufReader::new(fs::File::open(&metadata_path)?))?;
      if metadata.format_version != 1 {
        return Err("Unsupported deployment cache schema".into());
      }
      Some(metadata)
    } else {
      None
    };
    for relative in planned.keys().chain(previous.iter().flat_map(|metadata| metadata.files.keys()))
    {
      if !relative.starts_with("stream/") {
        return Err("Managed deployment files must be under stream/".into());
      }
      destination(&output, relative)?;
    }
    let mut summary = DeploySummary::default();
    let mut files = BTreeMap::new();
    for (relative, source) in &planned {
      let target = destination(&output, relative)?;
      let old = previous.as_ref().and_then(|metadata| metadata.files.get(relative));
      let input = fingerprint(
        source,
        old.filter(|file| file.source == *source).map(|file| &file.input),
        self.force,
      )?;
      let existing = if target.is_file() {
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
          source: source.clone(),
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
        if target.exists() {
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
      merged_dir: merged,
      source_cache_dir: cache,
      generated_at: chrono::Utc::now().to_rfc3339(),
      files,
    };
    if previous.as_ref().is_none_or(|previous| previous.files != metadata.files)
      || previous.as_ref().is_some_and(|previous| {
        previous.merged_dir != metadata.merged_dir
          || previous.source_cache_dir != metadata.source_cache_dir
      })
    {
      let temporary = destination(&output, &format!(".deploy-cache-{}.tmp", std::process::id()))?;
      if temporary.exists() {
        return Err("Deployment cache temporary file already exists".into());
      }
      write_json(&temporary, &metadata)?;
      fs::rename(temporary, metadata_path)?;
    }
    if fs::read_to_string(&moved_files_path).ok().as_deref() != Some(&moved_files) {
      let temporary = destination(&output, &format!(".files-{}.tmp", std::process::id()))?;
      let mut writer = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
      writer.write_all(moved_files.as_bytes())?;
      writer.flush()?;
      drop(writer);
      fs::rename(temporary, moved_files_path)?;
    }
    remove_empty_stream_directories(&output)?;
    Ok(summary)
  }
}

fn remove_empty_stream_directories(output: &Path) -> Result<()> {
  let stream = destination(output, "stream")?;
  if !stream.is_dir() {
    return Ok(());
  }
  for entry in WalkDir::new(&stream).follow_links(false).contents_first(true) {
    let entry = entry?;
    if !entry.file_type().is_dir() {
      continue;
    }
    match fs::remove_dir(entry.path()) {
      Ok(()) => {}
      Err(error)
        if matches!(
          error.kind(),
          std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
        ) => {}
      Err(error) => return Err(error.into()),
    }
  }
  Ok(())
}

fn plan(
  merged: &Path,
  cache: &Path,
) -> Result<BTreeMap<String, PathBuf>> {
  let mut files = BTreeMap::new();
  let mut merged_names = BTreeSet::new();
  for entry in WalkDir::new(merged).follow_links(false) {
    let entry = entry?;
    if entry.file_type().is_symlink() {
      return Err("Symlink merged input is not supported".into());
    }
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.path();
    let extension = extension(path);
    if !matches!(extension.as_str(), "ymap" | "ybn") {
      continue;
    }
    let name =
      path.file_name().and_then(|name| name.to_str()).ok_or("Merged filename is not UTF-8")?;
    if !merged_names.insert(name.to_ascii_lowercase()) {
      return Err(format!("Duplicate merged stream basename: {name}").into());
    }
    files.insert(format!("stream/{extension}/merged/{name}"), path.to_path_buf());
  }
  let resources = cache.join("resources");
  if !resources.is_dir() && !cache.join("source_cache_info.json").is_file() {
    return Err("Source cache has no resources directory or inventory".into());
  }
  if resources.is_dir() {
    for entry in WalkDir::new(&resources).follow_links(false) {
      let entry = entry?;
      if entry.file_type().is_symlink() {
        return Err("Symlink source-cache input is not supported".into());
      }
      if !entry.file_type().is_file() {
        continue;
      }
      let path = entry.path();
      let name =
        path.file_name().and_then(|name| name.to_str()).ok_or("Cached filename is not UTF-8")?;
      if merged_names.contains(&name.to_ascii_lowercase()) {
        continue;
      }
      let relative = path.strip_prefix(&resources)?.to_string_lossy().replace('\\', "/");
      files.insert(format!("stream/{}/clone/{relative}", extension(path)), path.to_path_buf());
    }
  }
  Ok(files)
}

fn extension(path: &Path) -> String {
  path
    .extension()
    .and_then(|value| value.to_str())
    .map(|value| value.to_ascii_lowercase())
    .unwrap_or_else(|| "no_extension".into())
}

fn fingerprint(
  path: &Path,
  cached: Option<&Fingerprint>,
  force: bool,
) -> Result<Fingerprint> {
  let metadata = fs::metadata(path)?;
  let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
  if !force
    && let Some(cached) = cached
    && cached.size == metadata.len()
    && cached.modified_seconds == modified.as_secs()
    && cached.modified_nanos == modified.subsec_nanos()
  {
    return Ok(cached.clone());
  }
  let mut reader = BufReader::new(fs::File::open(path)?);
  let mut digest = Sha256::new();
  let mut buffer = [0u8; 65536];
  loop {
    let count = reader.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    digest.update(&buffer[..count]);
  }
  Ok(Fingerprint {
    sha256: format!("{:x}", digest.finalize()),
    size: metadata.len(),
    modified_seconds: modified.as_secs(),
    modified_nanos: modified.subsec_nanos(),
  })
}

fn copy_file(
  source: &Path,
  target: &Path,
) -> Result<()> {
  let parent = target.parent().ok_or("Deployment file has no parent")?;
  fs::create_dir_all(parent)?;
  let temporary = parent.join(format!(
    ".{}.deploy-{}.tmp",
    target.file_name().ok_or("Deployment file has no name")?.to_string_lossy(),
    std::process::id()
  ));
  let mut writer = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
  let result = (|| -> Result<()> {
    let mut reader = fs::File::open(source)?;
    std::io::copy(&mut reader, &mut writer)?;
    writer.flush()?;
    writer.set_times(fs::FileTimes::new().set_modified(reader.metadata()?.modified()?))?;
    drop(writer);
    fs::rename(&temporary, target)?;
    Ok(())
  })();
  if result.is_err() {
    let _ = fs::remove_file(temporary);
  }
  result
}

fn prepare_output(
  path: &Path,
  inputs: &[&Path],
) -> Result<PathBuf> {
  let absolute =
    if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir()?.join(path) };
  let parent = absolute.parent().ok_or("Deployment output has no parent")?;
  fs::create_dir_all(parent)?;
  let output =
    parent.canonicalize()?.join(absolute.file_name().ok_or("Deployment output has no name")?);
  let current = std::env::current_dir()?.canonicalize()?;
  if current.starts_with(&output)
    || inputs.iter().any(|input| output.starts_with(input) || input.starts_with(&output))
  {
    return Err("Deployment output overlaps workspace or inputs".into());
  }
  if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err("Symlink deployment output is not supported".into());
  }
  fs::create_dir_all(&output)?;
  Ok(output)
}

fn destination(
  root: &Path,
  relative: &str,
) -> Result<PathBuf> {
  let mut target = root.to_path_buf();
  for component in Path::new(relative).components() {
    if !matches!(component, Component::Normal(_)) {
      return Err("Unsafe deployment cache path".into());
    }
    target.push(component);
    if fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Symlink deployment path is not supported: {}", target.display()).into());
    }
  }
  Ok(target)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn deploy_copies_only_changed_files_and_removes_only_managed_stale_files() {
    let root = std::env::temp_dir().join(format!("deploy_incremental_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let merged = root.join("merged");
    let cache = root.join("cache");
    let output = root.join("merged_mlo");
    fs::create_dir_all(merged.join("ymap")).unwrap();
    fs::create_dir_all(cache.join("resources/resource_a/nested")).unwrap();
    fs::create_dir_all(cache.join("_old/timestamp/resource_a")).unwrap();
    fs::create_dir_all(output.join("stream/obsolete/clone/resource/nested")).unwrap();
    fs::create_dir_all(output.join("stream/unmanaged")).unwrap();
    fs::write(output.join("stream/unmanaged/keep.txt"), b"keep").unwrap();
    fs::write(merged.join("ymap/map.ymap"), b"merged map").unwrap();
    fs::write(merged.join("_omit.txt"), b"source paths").unwrap();
    fs::write(cache.join("resources/resource_a/nested/MAP.YMAP"), b"must not clone").unwrap();
    fs::write(cache.join("resources/resource_a/nested/collision.ybn"), b"clone bytes").unwrap();
    fs::write(cache.join("_old/timestamp/resource_a/archived.ybn"), b"archive").unwrap();
    let deploy = Deploy {
      merged_dir: merged.clone(),
      source_cache_dir: cache.clone(),
      output_dir: output.clone(),
      force: false,
    };
    assert_eq!(
      deploy.run().unwrap(),
      DeploySummary {
        copied: 2,
        skipped: 0,
        removed: 0
      }
    );
    let cloned = output.join("stream/ybn/clone/resource_a/nested/collision.ybn");
    assert!(!output.join("stream/obsolete").exists());
    assert!(output.join("stream/unmanaged/keep.txt").is_file());
    assert_eq!(fs::read_to_string(output.join("files.txt")).unwrap(), "");
    assert_eq!(fs::read(output.join("stream/ymap/merged/map.ymap")).unwrap(), b"merged map");
    assert_eq!(fs::read(&cloned).unwrap(), b"clone bytes");
    assert!(!output.join("stream/ymap/clone").exists());
    assert!(!output.join("_omit.txt").exists());
    let stamp = fs::metadata(&cloned).unwrap().modified().unwrap();
    assert_eq!(
      stamp,
      fs::metadata(cache.join("resources/resource_a/nested/collision.ybn"))
        .unwrap()
        .modified()
        .unwrap()
    );
    assert_eq!(
      deploy.run().unwrap(),
      DeploySummary {
        copied: 0,
        skipped: 2,
        removed: 0
      }
    );
    assert_eq!(fs::metadata(&cloned).unwrap().modified().unwrap(), stamp);
    fs::File::options()
      .write(true)
      .open(cache.join("resources/resource_a/nested/collision.ybn"))
      .unwrap()
      .set_times(
        fs::FileTimes::new()
          .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(10)),
      )
      .unwrap();
    assert_eq!(deploy.run().unwrap().copied, 0);
    fs::write(&cloned, b"damaged").unwrap();
    assert_eq!(deploy.run().unwrap().copied, 1);
    assert_eq!(fs::read(&cloned).unwrap(), b"clone bytes");
    fs::write(cache.join("resources/resource_a/nested/collision.ybn"), b"changed clone content")
      .unwrap();
    assert_eq!(deploy.run().unwrap().copied, 1);
    assert_eq!(fs::read(&cloned).unwrap(), b"changed clone content");
    fs::remove_file(output.join("stream/ymap/merged/map.ymap")).unwrap();
    assert_eq!(deploy.run().unwrap().copied, 1);
    fs::write(merged.join("ymap/map.ymap"), b"changed merged map").unwrap();
    assert_eq!(deploy.run().unwrap().copied, 1);
    fs::write(output.join("unmanaged.txt"), b"keep").unwrap();
    fs::create_dir_all(merged.join("ybn")).unwrap();
    fs::write(merged.join("ybn/collision.ybn"), b"new merged collision").unwrap();
    assert_eq!(
      deploy.run().unwrap(),
      DeploySummary {
        copied: 1,
        skipped: 1,
        removed: 1
      }
    );
    assert!(!cloned.exists());
    assert!(!output.join("stream/ybn/clone").exists());
    assert!(output.join("stream/ybn/merged/collision.ybn").is_file());
    assert!(output.join("unmanaged.txt").exists());
    fs::remove_file(merged.join("ybn/collision.ybn")).unwrap();
    assert_eq!(
      deploy.run().unwrap(),
      DeploySummary {
        copied: 1,
        skipped: 1,
        removed: 1
      }
    );
    assert!(cloned.is_file());
    assert!(!output.join("stream/ybn/merged").exists());
    let mut forced = deploy.clone();
    forced.force = true;
    assert_eq!(forced.run().unwrap().copied, 2);
    let metadata: DeployMetadata =
      serde_json::from_reader(fs::File::open(output.join("deploy_cache_info.json")).unwrap())
        .unwrap();
    assert_eq!(metadata.files.len(), 2);
    assert!(metadata.files.values().all(|file| file.input.sha256 == file.output.sha256));
    fs::write(cache.join("resources/resource_a/nested/model.ytyp"), b"cached model").unwrap();
    assert_eq!(deploy.run().unwrap().copied, 1);
    assert_eq!(
      fs::read(output.join("stream/ytyp/clone/resource_a/nested/model.ytyp")).unwrap(),
      b"cached model"
    );
    fs::write(&cloned, b"user edit").unwrap();
    fs::remove_file(cache.join("resources/resource_a/nested/collision.ybn")).unwrap();
    assert!(deploy.run().unwrap_err().to_string().contains("Refusing to remove modified"));
    assert_eq!(fs::read(&cloned).unwrap(), b"user edit");
    let mut overlapping = deploy.clone();
    overlapping.output_dir = merged;
    assert!(overlapping.run().is_err());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn deploy_directory_cleanup_keeps_the_root_and_does_not_follow_links() {
    let root =
      std::env::temp_dir().join(format!("deploy_empty_directories_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let output = root.join("merged_mlo");
    fs::create_dir_all(output.join("stream/ymap/clone/resource/nested")).unwrap();
    fs::write(output.join("deploy_cache_info.json"), b"metadata").unwrap();
    fs::write(output.join("files.txt"), b"source paths").unwrap();
    remove_empty_stream_directories(&output).unwrap();
    assert!(!output.join("stream").exists());
    assert!(output.is_dir());
    assert!(output.join("deploy_cache_info.json").is_file());
    assert!(output.join("files.txt").is_file());
    remove_empty_stream_directories(&output).unwrap();
    #[cfg(unix)]
    {
      let external = root.join("external");
      fs::create_dir_all(external.join("empty/nested")).unwrap();
      fs::create_dir_all(output.join("stream/clone")).unwrap();
      std::os::unix::fs::symlink(&external, output.join("stream/clone/link")).unwrap();
      remove_empty_stream_directories(&output).unwrap();
      assert!(external.join("empty/nested").is_dir());
      assert!(
        fs::symlink_metadata(output.join("stream/clone/link")).unwrap().file_type().is_symlink()
      );
      fs::remove_dir_all(output.join("stream")).unwrap();
      std::os::unix::fs::symlink(&external, output.join("stream")).unwrap();
      assert!(remove_empty_stream_directories(&output).is_err());
      assert!(external.join("empty/nested").is_dir());
    }
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn deploy_rejects_duplicate_merged_names_and_unsafe_cache_entries() {
    let root = std::env::temp_dir().join(format!("deploy_paths_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let merged = root.join("merged");
    let cache = root.join("cache");
    let output = root.join("output");
    fs::create_dir_all(merged.join("ymap")).unwrap();
    fs::create_dir_all(cache.join("resources")).unwrap();
    fs::write(merged.join("ymap/map.ymap"), b"map").unwrap();
    fs::write(merged.join("MAP.YMAP"), b"duplicate").unwrap();
    let deploy = Deploy {
      merged_dir: merged.clone(),
      source_cache_dir: cache,
      output_dir: output.clone(),
      force: false,
    };
    assert!(deploy.run().unwrap_err().to_string().contains("Duplicate merged stream basename"));
    assert!(!output.join("stream").exists());
    fs::remove_file(merged.join("MAP.YMAP")).unwrap();
    assert_eq!(deploy.run().unwrap().copied, 1);
    let mut metadata: DeployMetadata =
      serde_json::from_reader(fs::File::open(output.join("deploy_cache_info.json")).unwrap())
        .unwrap();
    let record = metadata.files.remove("stream/ymap/merged/map.ymap").unwrap();
    metadata.files.insert("../outside.ymap".into(), record);
    write_json(&output.join("deploy_cache_info.json"), &metadata).unwrap();
    assert!(
      deploy
        .run()
        .unwrap_err()
        .to_string()
        .contains("Managed deployment files must be under stream/")
    );
    assert_eq!(fs::read(output.join("stream/ymap/merged/map.ymap")).unwrap(), b"map");
    #[cfg(unix)]
    {
      fs::remove_file(output.join("deploy_cache_info.json")).unwrap();
      std::os::unix::fs::symlink(&merged, output.join("linked")).unwrap();
      let mut linked = deploy.clone();
      linked.output_dir = output.join("linked");
      assert!(linked.run().is_err());
    }
    fs::remove_dir_all(root).unwrap();
  }
}
