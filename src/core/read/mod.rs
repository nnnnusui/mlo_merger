//! Writes stream file models as JSON for tools outside the merge, such as overwrite editors.
//!
//! A resource's file is written to `resources/{resource name}/{stream path}.json` and a vanilla
//! file to `vanilla/{version}/{file type}/{file name}.json` after the version folder it is stored
//! in, each recording the file it was read from; a JSON still describing its file is not written
//! again.

#[cfg(test)]
mod tests;

use crate::common::{Result, fingerprint::Fingerprint};
use crate::core::config::gamebuild::GameBuild;
use crate::core::conversion::load_hash_names_or_empty;
use crate::core::format::ymap::{binary::ymap_to_model, model::ymap::Ymap};
use crate::core::source_cache::{InventoriedFile, inventoried_files};
use crate::core::vanilla::{VanillaCacheManifest, load_manifest};
use crate::core::vanilla_cache::{built_version, target_version};
use crate::platform::fs;
use crate::platform::progress::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Which stream files to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadTarget {
  /// A resource's files, by resource name or source-relative resource ID.
  Resource(String),
  /// The files of the vanilla stage [`Read::gamebuild`] selects.
  Vanilla,
  /// The named files in every resource and in that vanilla stage.
  Everywhere,
  /// Every supported file of every resource and of that vanilla stage.
  All,
}

/// Writes the JSON of stream file models.
#[derive(Debug, Clone)]
pub struct Read {
  pub target: ReadTarget,
  /// File names, or resources' stream paths, to read; none reads every supported file of the
  /// target. Required for [`ReadTarget::Everywhere`] and rejected for [`ReadTarget::All`].
  pub files: Vec<String>,
  /// Source root holding the resources' files that were not moved into the source cache.
  pub source_dir: PathBuf,
  /// Source cache whose inventory locates the resources' files.
  pub source_cache_dir: PathBuf,
  /// Raw vanilla archive holding the versions and `hash_names.json`.
  pub vanilla_dir: PathBuf,
  /// Vanilla cache whose target stage vanilla files are read from without [`Self::gamebuild`].
  pub vanilla_cache_dir: PathBuf,
  /// Game build whose last stage vanilla files are read from. Without one they are read from
  /// the stage the vanilla cache was built for, or the last stage when there is no cache, so
  /// they match the merge and the vanilla cache's index.
  pub gamebuild: Option<GameBuild>,
  /// Where the JSON is written.
  pub output_dir: PathBuf,
  /// Writes every JSON again, even one still describing its file.
  pub force: bool,
  /// With [`ReadTarget::Resource`] and its files named, reads instead the earlier version with
  /// this SHA-256 of each, which the source cache keeps in its history (see
  /// [`crate::core::source_cache::history`]).
  pub version: Option<String>,
}

/// What [`Read::run`] wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ReadResult {
  pub files: Vec<ReadFileResult>,
}

/// One stream file [`Read::run`] read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ReadFileResult {
  pub origin: ReadOrigin,
  /// The stream file read.
  pub input: PathBuf,
  /// The JSON describing it.
  pub output: PathBuf,
  /// Whether the JSON was written; `false` when it already described the file.
  pub updated: bool,
}

/// Where a stream file read comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum ReadOrigin {
  Resource {
    /// Resource directory name.
    resource: String,
    /// Path below the resource's `stream` directory.
    stream_path: String,
  },
  Vanilla {
    /// The version whose folder holds the file: the last one, up to the stage read, that
    /// changed it.
    version: String,
    file_name: String,
  },
  /// An earlier version of a resource's file, kept in the source cache's history.
  History {
    /// Resource directory name.
    resource: String,
    /// Path below the resource's `stream` directory.
    stream_path: String,
    sha256: String,
  },
}

/// The JSON written for a YMAP: its model and the files it was read from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapReadDocument {
  /// Version of the document layout; documents of another version are written again.
  pub schema_version: u32,
  /// The YMAP read.
  pub source: ReadFingerprint,
  /// The `hash_names.json` that named its hashes, if any.
  pub hash_names: Option<ReadFingerprint>,
  pub model: Ymap,
}

impl YmapReadDocument {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;
}

/// A file a document was read from, as it was then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ReadFingerprint {
  /// The file relative to its store: the source cache for a resource's file moved there, the
  /// source root for one that was not, the vanilla archive for vanilla files and
  /// `hash_names.json`.
  pub path: PathBuf,
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl ReadFingerprint {
  /// The file at `path`, recorded as `stored` (relative to its store), as it is now; its contents
  /// are hashed again only when its size or modification time differs from `previous`.
  fn read(
    path: &Path,
    stored: &str,
    previous: Option<&Self>,
  ) -> Result<Self> {
    let stored = PathBuf::from(stored);
    let previous =
      previous.filter(|previous| previous.path == stored).map(|previous| Fingerprint {
        sha256: previous.sha256.clone(),
        size: previous.size,
        modified_seconds: previous.modified_seconds,
        modified_nanos: previous.modified_nanos,
      });
    let fingerprint = Fingerprint::read(path, previous.as_ref(), false)?;
    Ok(Self {
      path: stored,
      sha256: fingerprint.sha256,
      size: fingerprint.size,
      modified_seconds: fingerprint.modified_seconds,
      modified_nanos: fingerprint.modified_nanos,
    })
  }

  fn same_content(
    &self,
    other: &Self,
  ) -> bool {
    self.path == other.path && self.sha256 == other.sha256 && self.size == other.size
  }
}

/// A document's header, read without its model.
#[derive(Deserialize)]
struct Header {
  schema_version: u32,
  source: ReadFingerprint,
  hash_names: Option<ReadFingerprint>,
}

/// A stream file selected to be read.
struct Selected {
  origin: ReadOrigin,
  input: PathBuf,
  /// [`Self::input`] relative to its store, as its document records it.
  stored: String,
  output: PathBuf,
}

/// The file types `--read` writes.
const SUPPORTED_EXTENSIONS: [&str; 1] = ["ymap"];

impl Read {
  /// Writes the JSON of each selected file whose JSON is missing or describes another file.
  ///
  /// Fails before writing anything when the target selects no file, when a requested name
  /// matches no file, or when a resource holds more than one file of a requested name.
  pub fn run(&self) -> Result<ReadResult> {
    // Documents record paths relative to each store, so they stay valid wherever the stores are;
    // the result names files by absolute path.
    fs::create_dir_all(&self.output_dir)?;
    let absolute = |path: &Path| {
      fs::canonicalize(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))
    };
    let read = Self {
      source_dir: absolute(&self.source_dir)?,
      source_cache_dir: absolute(&self.source_cache_dir)?,
      vanilla_dir: absolute(&self.vanilla_dir)?,
      output_dir: absolute(&self.output_dir)?,
      ..self.clone()
    };
    read.run_absolute()
  }

  fn run_absolute(&self) -> Result<ReadResult> {
    for file in &self.files {
      supported(file)?;
    }
    let mut matched = vec![false; self.files.len()];
    let mut selected = match &self.target {
      ReadTarget::Resource(resource) => self.resource_files(Some(resource), &mut matched)?,
      ReadTarget::Vanilla => self.vanilla_files(&mut matched)?,
      ReadTarget::Everywhere | ReadTarget::All => {
        match (&self.target, self.files.is_empty()) {
          (ReadTarget::Everywhere, true) => {
            return Err("--read without --resource, --vanilla or --all needs a file name".into());
          }
          (ReadTarget::All, false) => {
            return Err("--read --all reads every file; leave out the file names".into());
          }
          _ => {}
        }
        let mut files = self.resource_files(None, &mut matched)?;
        files.extend(self.vanilla_files(&mut matched)?);
        files
      }
    };
    if let Some(version) = &self.version {
      selected = self.history_versions(selected, version)?;
    }
    let unmatched = self
      .files
      .iter()
      .zip(&matched)
      .filter(|(_, matched)| !**matched)
      .map(|(file, _)| file.as_str())
      .collect::<Vec<_>>();
    if !unmatched.is_empty() {
      return Err(format!("No stream file matches {}", unmatched.join(", ")).into());
    }
    if selected.is_empty() {
      return Err("The target has no supported stream file".into());
    }
    // A file named twice, by its name and by its stream path, is read once.
    let mut outputs = HashSet::new();
    selected.retain(|selected| outputs.insert(selected.output.clone()));
    tracing::info!("Reading {} stream files", selected.len());

    // Each file's document header and fingerprint are read independently.
    let progress = FileProgress::new(selected.len(), "Checked")?;
    let checked = selected
      .into_par_iter()
      .map(|selected| {
        let _tick = progress.tick();
        let previous = self.header(&selected.output);
        let source = ReadFingerprint::read(
          &selected.input,
          &selected.stored,
          previous.as_ref().map(|it| &it.source),
        )
        .map_err(|error| error.to_string())?;
        Ok((selected, previous, source))
      })
      .collect::<std::result::Result<Vec<_>, String>>();
    progress.finish();
    let checked = checked?;

    // Every document records the same `hash_names.json`; any previous document's record spares
    // hashing it again when it is unchanged.
    let names_path = self.vanilla_dir.join("hash_names.json");
    let hash_names = fs::is_file(&names_path)
      .then(|| {
        let previous =
          checked.iter().find_map(|(_, previous, _)| previous.as_ref()?.hash_names.as_ref());
        ReadFingerprint::read(&names_path, "hash_names.json", previous)
      })
      .transpose()?;
    let stale = |previous: &Option<Header>, source: &ReadFingerprint| {
      self.force
        || !previous.as_ref().is_some_and(|previous| {
          previous.schema_version == YmapReadDocument::SCHEMA_VERSION
            && previous.source.same_content(source)
            && match (&previous.hash_names, &hash_names) {
              (Some(previous), Some(current)) => previous.same_content(current),
              (None, None) => true,
              _ => false,
            }
        })
    };
    let names = if checked.iter().any(|(_, previous, source)| stale(previous, source)) {
      load_hash_names_or_empty(&names_path)
    } else {
      HashMap::new()
    };

    // Stale documents are converted and written independently; results keep the selection's
    // order.
    let progress = FileProgress::new(checked.len(), "Wrote")?;
    let files = checked
      .into_par_iter()
      .map(|(selected, previous, source)| {
        let _tick = progress.tick();
        let updated = stale(&previous, &source);
        if updated {
          let bytes = fs::read(&selected.input)
            .map_err(|error| format!("Cannot open {}: {error}", selected.input.display()))?;
          let model = ymap_to_model(&bytes, &names)
            .map_err(|error| format!("Cannot read {}: {error}", selected.input.display()))?;
          write_minified(
            &selected.output,
            &YmapReadDocument {
              schema_version: YmapReadDocument::SCHEMA_VERSION,
              source,
              hash_names: hash_names.clone(),
              model,
            },
          )
          .map_err(|error| error.to_string())?;
        }
        Ok(ReadFileResult {
          origin: selected.origin,
          input: selected.input,
          output: selected.output,
          updated,
        })
      })
      .collect::<std::result::Result<Vec<_>, String>>();
    progress.finish();
    let files = files?;
    let written = files.iter().filter(|file| file.updated).count();
    tracing::info!("Read {} stream files; wrote {written} JSON", files.len());
    Ok(ReadResult {
      files,
    })
  }

  /// The resources' files matching [`Self::files`], marking in `matched` the names that match;
  /// every resource's when `resource` is `None`.
  fn resource_files(
    &self,
    resource: Option<&str>,
    matched: &mut [bool],
  ) -> Result<Vec<Selected>> {
    let inventory = inventoried_files(&self.source_cache_dir, &self.source_dir)?;
    let inventory = match resource {
      Some(resource) => {
        let resource = resolve_resource(&inventory, resource)?;
        inventory.into_iter().filter(|file| file.resource == resource).collect()
      }
      None => inventory,
    };
    // Files keyed by resource and the index of the name they match, in that order.
    let mut by_name = BTreeMap::<(&str, Option<usize>), Vec<&InventoriedFile>>::new();
    for file in &inventory {
      if self.files.is_empty() {
        if supported(&file.file_name).is_ok() {
          by_name.entry((&file.resource, None)).or_default().push(file);
        }
        continue;
      }
      for (index, name) in self.files.iter().enumerate() {
        if matches_file(file, name) {
          matched[index] = true;
          by_name.entry((&file.resource, Some(index))).or_default().push(file);
        }
      }
    }
    let mut selected = Vec::new();
    for ((resource, index), files) in by_name {
      if let Some(index) = index
        && files.len() > 1
      {
        return Err(
          format!(
            "{} matches {} files of resource {resource}; name one by its stream path: {}",
            self.files[index],
            files.len(),
            files.iter().map(|file| file.stream_path.as_str()).collect::<Vec<_>>().join(", ")
          )
          .into(),
        );
      }
      for file in files {
        selected.push(Selected {
          origin: ReadOrigin::Resource {
            resource: file.resource_name.clone(),
            stream_path: file.stream_path.clone(),
          },
          input: file.path.clone(),
          stored: file.stored_path.clone(),
          output: self
            .output_dir
            .join("resources")
            .join(&file.resource_name)
            .join(format!("{}.json", file.stream_path)),
        });
      }
    }
    Ok(selected)
  }

  /// The versions with `sha256` of the resource files `selected`, from the source cache's
  /// history.
  fn history_versions(
    &self,
    selected: Vec<Selected>,
    sha256: &str,
  ) -> Result<Vec<Selected>> {
    if !matches!(self.target, ReadTarget::Resource(_)) || self.files.is_empty() {
      return Err("--read --version reads named files of one --resource".into());
    }
    selected
      .into_iter()
      .map(|selected| {
        let ReadOrigin::Resource {
          resource,
          stream_path,
        } = selected.origin
        else {
          return Err("--read --version reads resource files".into());
        };
        let stored =
          crate::core::source_cache::history::version_path(&resource, &stream_path, sha256);
        let input = self.source_cache_dir.join(&stored);
        if !fs::is_file(&input) {
          return Err(
            format!("The source cache keeps no version {sha256} of {stream_path} ({resource})")
              .into(),
          );
        }
        Ok(Selected {
          output: self.output_dir.join(format!("{stored}.json")),
          origin: ReadOrigin::History {
            resource,
            stream_path,
            sha256: sha256.to_string(),
          },
          input,
          stored,
        })
      })
      .collect()
  }

  /// The files of the vanilla stage [`Self::vanilla_version`] selects, matching
  /// [`Self::files`], marking in `matched` the names that match.
  fn vanilla_files(
    &self,
    matched: &mut [bool],
  ) -> Result<Vec<Selected>> {
    let manifest = load_manifest(&self.vanilla_dir)?;
    let version = self.vanilla_version(&manifest)?;
    let files = manifest.resolve_version(&version)?;
    let mut selected = Vec::new();
    for (file_name, file) in files {
      let wanted = if self.files.is_empty() {
        supported(&file_name).is_ok()
      } else {
        let mut wanted = false;
        for (index, name) in self.files.iter().enumerate() {
          if file_name.eq_ignore_ascii_case(name) {
            matched[index] = true;
            wanted = true;
          }
        }
        wanted
      };
      if !wanted {
        continue;
      }
      // The file lives in the folder of the version that last changed it, such as
      // `0000-base/ymap/{file name}`; its document mirrors that path, so every version holding
      // the same file shares one document.
      let stored = file.object.replace('\\', "/");
      let holder = stored.split_once('/').map_or(version.as_str(), |(holder, _)| holder);
      selected.push(Selected {
        output: self.output_dir.join("vanilla").join(format!("{stored}.json")),
        input: self.vanilla_dir.join(&file.object),
        origin: ReadOrigin::Vanilla {
          version: holder.to_string(),
          file_name,
        },
        stored,
      });
    }
    Ok(selected)
  }

  /// The stage vanilla files are read from: the last stage of [`Self::gamebuild`], otherwise
  /// the stage the vanilla cache was built for, otherwise the last stage.
  fn vanilla_version(
    &self,
    manifest: &VanillaCacheManifest,
  ) -> Result<String> {
    if self.gamebuild.is_some() {
      return target_version(manifest, self.gamebuild.as_ref());
    }
    if let Some(version) = built_version(&self.vanilla_cache_dir) {
      if manifest.versions.iter().any(|candidate| candidate.id == version) {
        return Ok(version);
      }
      tracing::warn!(
        "The vanilla cache targets {version}, which the vanilla archive does not have; reading the last stage"
      );
    }
    target_version(manifest, None)
  }

  /// The header of the document at `path`, unless it is missing or unreadable.
  fn header(
    &self,
    path: &Path,
  ) -> Option<Header> {
    crate::common::json::read_json(path).ok()
  }
}

/// The source-relative ID of the resource named `name`: its ID, or its directory name when no
/// other resource shares it.
fn resolve_resource(
  inventory: &[InventoriedFile],
  name: &str,
) -> Result<String> {
  let mut resources = inventory
    .iter()
    .filter(|file| {
      file.resource.eq_ignore_ascii_case(name) || file.resource_name.eq_ignore_ascii_case(name)
    })
    .map(|file| file.resource.as_str())
    .collect::<Vec<_>>();
  resources.sort_unstable();
  resources.dedup();
  match resources[..] {
    [resource] => Ok(resource.to_string()),
    [] => Err(format!("The source cache has no resource {name}").into()),
    _ => Err(format!("Resource name {name} is ambiguous: {}", resources.join(", ")).into()),
  }
}

/// Whether `file` is the one `name` asks for: by stream path when `name` has a directory,
/// otherwise by file name.
fn matches_file(
  file: &InventoriedFile,
  name: &str,
) -> bool {
  let name = name.replace('\\', "/");
  if name.contains('/') {
    file.stream_path.eq_ignore_ascii_case(name.trim_start_matches('/'))
  } else {
    file.file_name.eq_ignore_ascii_case(&name)
  }
}

fn extension(file_name: &str) -> &str {
  file_name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

fn supported(file_name: &str) -> Result<()> {
  let extension = extension(file_name);
  if SUPPORTED_EXTENSIONS.iter().any(|supported| supported.eq_ignore_ascii_case(extension)) {
    Ok(())
  } else {
    Err(
      format!("--read supports {} files, not {file_name}", SUPPORTED_EXTENSIONS.join(", ")).into(),
    )
  }
}

/// A progress bar over the files of one stage of [`Read::run`], drawn on a terminal and logged
/// every thousand files elsewhere.
struct FileProgress {
  bar: ProgressBar,
  total: usize,
  /// What the log says of the files counted, such as `Checked`.
  verb: &'static str,
  log: bool,
}

impl FileProgress {
  fn new(
    total: usize,
    verb: &'static str,
  ) -> Result<Self> {
    #[cfg(test)]
    let (bar, log) = (ProgressBar::hidden(), false);
    #[cfg(not(test))]
    let (bar, log) = (
      ProgressBar::with_draw_target(
        Some(total as u64),
        crate::platform::progress::ProgressDrawTarget::stderr(),
      ),
      !std::io::IsTerminal::is_terminal(&std::io::stderr()),
    );
    bar.set_style(
      ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
      )?
      .progress_chars("##-"),
    );
    bar.set_message(format!("{verb} stream files"));
    bar.enable_steady_tick(Duration::from_millis(100));
    Ok(Self {
      bar,
      total,
      verb,
      log,
    })
  }

  /// Counts one file when the returned guard drops, whether its work succeeded or not.
  fn tick(&self) -> impl Drop + '_ {
    struct Tick<'a>(&'a FileProgress);
    impl Drop for Tick<'_> {
      fn drop(&mut self) {
        let progress = self.0;
        progress.bar.inc(1);
        let position = progress.bar.position();
        if progress.log && position.is_multiple_of(1000) {
          tracing::info!("{} {position}/{} stream files", progress.verb, progress.total);
        }
      }
    }
    Tick(self)
  }

  fn finish(&self) {
    self.bar.finish_and_clear();
  }
}

/// Writes `value` as minified JSON, replacing `path` only once it is complete.
fn write_minified<T: Serialize>(
  path: &Path,
  value: &T,
) -> Result<()> {
  fs::create_dir_all(path.parent().ok_or("Output path has no directory")?)?;
  let partial = path.with_extension("json.partial");
  let mut writer = BufWriter::new(fs::File::create(&partial)?);
  serde_json::to_writer(&mut writer, value)?;
  writer.flush()?;
  drop(writer);
  fs::rename(&partial, path)?;
  Ok(())
}
