//! Materializes one ordered game-archive stage.

use crate::platform::fs;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{
  Result,
  archives::ExtractedFile,
  dlc_content::DlcContent,
  manifest::{CacheVersion, CachedFile, FileChange, PatchVariant, VanillaCacheManifest},
};
use crate::common::json::write_json;

/// The directory of a DLC pack's stage holding the versions of the title update's patch of the
/// pack (`update/update.rpf/dlc_patch/{pack}/`), one subdirectory each.
pub(crate) const PATCH_DIRECTORY: &str = "update_rpf";

/// The name of the version of a patch only the installed game's title update ships.
pub(crate) const INSTALLED_PATCH: &str = "installed";

/// The extracted files of a stage and the archives they come from, each file with the directory
/// it was extracted into.
type StageFiles = (Vec<String>, Vec<(ExtractedFile, PathBuf)>);

/// The cumulative files of the stages so far, as the installed game loads them, and the names
/// whose content differs between the game builds' versions of the title update's patches.
#[derive(Debug, Default)]
pub(crate) struct Cumulative {
  pub(crate) files: BTreeMap<String, CachedFile>,
  pub(crate) varied: BTreeSet<String>,
}

/// One version of the title update's patch of a DLC pack, as some game builds ship it.
#[derive(Debug)]
pub(crate) struct PatchSource {
  /// The name of its directory: the oldest game build whose own title update ships it, or
  /// `installed`.
  pub(crate) name: String,
  /// The game builds whose title update ships it.
  pub(crate) builds: Vec<u32>,
  /// Whether the installed game's title update ships it.
  pub(crate) installed: bool,
  /// Its extracted files.
  pub(crate) files: Vec<(ExtractedFile, PathBuf)>,
  /// What the pack declares with it.
  pub(crate) dlc: Option<DlcContent>,
}

/// What a DLC pack's stage is made of: its archives and files, what it declares, and the
/// versions of the title update's patch of it.
pub(crate) struct PackFiles {
  pub(crate) archives: Vec<String>,
  pub(crate) files: Vec<(ExtractedFile, PathBuf)>,
  pub(crate) dlc: Option<DlcContent>,
  pub(crate) patches: Vec<PatchSource>,
}

/// Materializes the stage of the DLC pack `label` from `pack`, then each version of the title
/// update's patch of it into `{stage}/update_rpf/{name}/`; a file an earlier version already
/// stores is not stored again.
///
/// The patches are kept beside the pack's own files, so the pack's versions of the files they
/// replace stay, and a reader takes the version of its game build ([`VanillaCacheManifest::for_build`]).
/// `builds` are the game builds the patches were read for: a single version every one of them
/// and the installed game ship is the same on every build.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stage_pack(
  manifest: &mut VanillaCacheManifest,
  current: &mut Cumulative,
  output: &Path,
  label: &str,
  extensions: &[&str],
  builds: &BTreeSet<u32>,
  pack: impl FnOnce() -> Result<PackFiles>,
) -> Result<()> {
  let mut content = None;
  let mut patches = Vec::new();
  stage_with(manifest, current, output, label, extensions, || {
    let pack = pack()?;
    content = pack.dlc;
    patches = pack.patches;
    Ok((pack.archives, pack.files))
  })?;
  let version = manifest.versions.last_mut().ok_or("The pack's stage is missing")?;
  version.dlc = content;
  let uniform = matches!(patches.as_slice(), [patch]
    if patch.installed && builds.iter().all(|build| patch.builds.contains(build)));
  let mut written = BTreeSet::new();
  let mut stored = BTreeMap::new();
  for patch in patches {
    let id = format!("{}/{PATCH_DIRECTORY}/{}", version.id, patch.name);
    tracing::info!("Creating patch {id} for builds {:?}", patch.builds);
    let staged = write_changes(
      (&current.files, &current.varied),
      output,
      &id,
      extensions,
      patch.files,
      &mut stored,
    )?;
    written.extend(staged.changes.keys().cloned());
    if patch.installed {
      for (name, change) in &staged.changes {
        current.files.insert(name.clone(), change.file.clone());
      }
    }
    version.patches.push(PatchVariant {
      id,
      builds: patch.builds,
      installed: patch.installed,
      archives: vec![format!("update/update.rpf/dlc_patch/{label}")],
      changes: staged.changes,
      file_kinds: staged.file_kinds,
      unchanged: staged.unchanged,
      dlc: patch.dlc,
    });
  }
  if uniform {
    current.varied.retain(|name| !written.contains(name));
  } else {
    current.varied.extend(written);
  }
  if has_files(version) {
    write_json(&output.join(&version.id).join("version_info.json"), &*version)?;
  }
  Ok(())
}

/// Whether the stage `version` or a version of a patch in it stores files.
pub(crate) fn has_files(version: &CacheVersion) -> bool {
  !version.changes.is_empty() || version.patches.iter().any(|patch| !patch.changes.is_empty())
}

#[cfg(test)]
pub(crate) fn stage(
  manifest: &mut VanillaCacheManifest,
  current: &mut BTreeMap<String, CachedFile>,
  output: &Path,
  label: &str,
  extensions: &[&str],
  files: impl FnOnce() -> Result<StageFiles>,
) -> Result<()> {
  let mut cumulative = Cumulative {
    files: std::mem::take(current),
    varied: BTreeSet::new(),
  };
  let result = stage_with(manifest, &mut cumulative, output, label, extensions, files);
  *current = cumulative.files;
  result
}

/// [`stage`] over `current`, storing a file of a name in `current.varied` even when its content
/// matches the installed game's, as another build's differs.
pub(crate) fn stage_with(
  manifest: &mut VanillaCacheManifest,
  current: &mut Cumulative,
  output: &Path,
  label: &str,
  extensions: &[&str],
  files: impl FnOnce() -> Result<StageFiles>,
) -> Result<()> {
  let id = format!("{:04}-{label}", manifest.versions.len());
  let directory = output.join(&id);
  let span = tracing::info_span!("vanilla_cache_stage", stage = %id);
  let _entered = span.enter();
  let result = (|| -> Result<()> {
    tracing::info!("Creating cache {id}");
    let (archives, files) = files()?;
    tracing::info!("Processing archives: {}", archives.join(", "));
    let staged = write_changes(
      (&current.files, &current.varied),
      output,
      &id,
      extensions,
      files,
      &mut BTreeMap::new(),
    )?;
    for (name, change) in &staged.changes {
      current.files.insert(name.clone(), change.file.clone());
      current.varied.remove(name);
    }
    tracing::info!(
      "{id}: {} additions/replacements, {} unchanged, {} effective native files",
      staged.changes.len(),
      staged.unchanged,
      current.files.len()
    );
    let version = CacheVersion {
      id,
      parent: manifest.versions.last().map(|version| version.id.clone()),
      archives,
      changes: staged.changes,
      file_kinds: staged.file_kinds,
      unchanged: staged.unchanged,
      dlc: None,
      patches: Vec::new(),
    };
    if !version.changes.is_empty() {
      write_json(&directory.join("version_info.json"), &version)?;
    }
    manifest.versions.push(version);
    Ok(())
  })();
  if let Err(error) = &result {
    tracing::error!("Cache creation failed: {error}");
  }
  result
}

/// The files of one stage or patch, relative to the files before it.
struct StagedChanges {
  changes: BTreeMap<String, FileChange>,
  file_kinds: BTreeMap<String, String>,
  unchanged: usize,
}

/// Copies the files of the selected `extensions` among `files` whose content differs from
/// `current`'s, or whose name is in `varied`, into `{output}/{directory}/{extension}/`; of
/// files sharing a name, the last wins. A file `stored` lists by name and SHA-256 is recorded at
/// the object it lists instead of being copied, and the files copied are added to it.
fn write_changes(
  (current, varied): (&BTreeMap<String, CachedFile>, &BTreeSet<String>),
  output: &Path,
  directory: &str,
  extensions: &[&str],
  files: Vec<(ExtractedFile, PathBuf)>,
  stored: &mut BTreeMap<(String, String), String>,
) -> Result<StagedChanges> {
  let mut winners = BTreeMap::new();
  for (file, directory) in files {
    if let Some((stem, extension)) = file.name.rsplit_once('.')
      && extensions.contains(&extension)
    {
      if file.name.contains(['/', '\\']) || stem.is_empty() {
        return Err(format!("Invalid native basename: {}", file.name).into());
      }
      if let Some((previous, _)) = winners.get(&file.name) {
        let previous: &ExtractedFile = previous;
        if previous.sha256 != file.sha256 {
          tracing::warn!(
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
  let mut staged = StagedChanges {
    changes: BTreeMap::new(),
    file_kinds: BTreeMap::new(),
    unchanged: 0,
  };
  for (name, (file, source_directory)) in winners {
    let previous = current.get(&name);
    if previous.is_some_and(|previous| previous.sha256 == file.sha256) && !varied.contains(&name) {
      staged.unchanged += 1;
      tracing::debug!("Unchanged {name}: {}", file.source);
      continue;
    }
    let input = source_directory.join(&file.stored);
    let family = Path::new(&name)
      .extension()
      .and_then(|extension| extension.to_str())
      .ok_or_else(|| format!("Invalid native basename: {name}"))?;
    if let Some(previous) = previous {
      tracing::info!("Modified {name}: {} -> {}", previous.source, file.source);
    } else {
      tracing::info!("Added {name}: {}", file.source);
    }
    let key = (name.clone(), file.sha256.clone());
    let object = match stored.get(&key) {
      Some(object) => object.clone(),
      None => {
        let object = format!("{directory}/{family}/{name}");
        fs::create_dir_all(output.join(directory).join(family))?;
        fs::copy(&input, output.join(&object))?;
        stored.insert(key, object.clone());
        object
      }
    };
    if let Some(kind) = file.kind {
      staged.file_kinds.insert(name.clone(), kind.to_string());
    }
    staged.changes.insert(
      name,
      FileChange {
        previous_sha256: previous.map(|previous| previous.sha256.clone()),
        file: CachedFile {
          sha256: file.sha256,
          object,
          source: file.source,
        },
      },
    );
  }
  Ok(staged)
}
