//! Writes stream file models as JSON for tools outside the merge, such as overwrite editors.
//!
//! A resource's file is written to `resources/{resource name}/{stream path}.json` and a vanilla
//! version's to `vanilla/{version}/{file type}/{file name}.json`, each recording the file it was
//! read from; a JSON still describing its file is not written again.

#[cfg(test)]
mod tests;

use crate::common::{Result, fingerprint::Fingerprint};
use crate::core::conversion::load_hash_names_or_empty;
use crate::core::format::ymap::{binary::ymap_to_model, model::ymap::Ymap};
use crate::core::source_cache::{InventoriedFile, inventoried_files};
use crate::core::vanilla::load_manifest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Which stream files to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadTarget {
  /// A resource's files, by resource name or source-relative resource ID.
  Resource(String),
  /// A vanilla version's files; `None` is the latest.
  Vanilla(Option<String>),
  /// The named files in every resource and in the latest vanilla version.
  Everywhere,
}

/// Writes the JSON of stream file models.
#[derive(Debug, Clone)]
pub struct Read {
  pub target: ReadTarget,
  /// File names, or resources' stream paths, to read; none reads every supported file of the
  /// target. Required for [`ReadTarget::Everywhere`].
  pub files: Vec<String>,
  /// Source cache whose inventory locates the resources' files.
  pub source_cache_dir: PathBuf,
  /// Raw vanilla archive holding the versions and `hash_names.json`.
  pub vanilla_dir: PathBuf,
  /// Where the JSON is written.
  pub output_dir: PathBuf,
  /// Writes every JSON again, even one still describing its file.
  pub force: bool,
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
    version: String,
    file_name: String,
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
  pub path: PathBuf,
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl ReadFingerprint {
  /// The file at `path` as it is now; its contents are hashed again only when its size or
  /// modification time differs from `previous`.
  fn read(
    path: &Path,
    previous: Option<&Self>,
  ) -> Result<Self> {
    let previous = previous.filter(|previous| previous.path == path).map(|previous| Fingerprint {
      sha256: previous.sha256.clone(),
      size: previous.size,
      modified_seconds: previous.modified_seconds,
      modified_nanos: previous.modified_nanos,
    });
    let fingerprint = Fingerprint::read(path, previous.as_ref(), false)?;
    Ok(Self {
      path: path.to_path_buf(),
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
    // Absolute paths keep the recorded fingerprints valid from any working directory.
    fs::create_dir_all(&self.output_dir)?;
    let absolute = |path: &Path| {
      path.canonicalize().map_err(|error| format!("Cannot open {}: {error}", path.display()))
    };
    let read = Self {
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
      ReadTarget::Vanilla(version) => self.vanilla_files(version.as_deref(), &mut matched)?,
      ReadTarget::Everywhere => {
        if self.files.is_empty() {
          return Err("--read without --resource or --vanilla needs a file name".into());
        }
        let mut files = self.resource_files(None, &mut matched)?;
        files.extend(self.vanilla_files(None, &mut matched)?);
        files
      }
    };
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
    let mut outputs = std::collections::HashSet::new();
    selected.retain(|selected| outputs.insert(selected.output.clone()));
    let names_path = self.vanilla_dir.join("hash_names.json");
    let mut names: Option<HashMap<u32, String>> = None;
    let mut files = Vec::with_capacity(selected.len());
    for selected in selected {
      let previous = self.header(&selected.output);
      let source = ReadFingerprint::read(&selected.input, previous.as_ref().map(|it| &it.source))?;
      let hash_names = names_path
        .is_file()
        .then(|| {
          ReadFingerprint::read(
            &names_path,
            previous.as_ref().and_then(|it| it.hash_names.as_ref()),
          )
        })
        .transpose()?;
      let current = previous.as_ref().is_some_and(|previous| {
        previous.schema_version == YmapReadDocument::SCHEMA_VERSION
          && previous.source.same_content(&source)
          && match (&previous.hash_names, &hash_names) {
            (Some(previous), Some(current)) => previous.same_content(current),
            (None, None) => true,
            _ => false,
          }
      });
      if !self.force && current {
        files.push(ReadFileResult {
          origin: selected.origin,
          input: selected.input,
          output: selected.output,
          updated: false,
        });
        continue;
      }
      let names = names.get_or_insert_with(|| load_hash_names_or_empty(&names_path));
      let model = ymap_to_model(&fs::read(&selected.input)?, names)
        .map_err(|error| format!("Cannot read {}: {error}", selected.input.display()))?;
      write_minified(
        &selected.output,
        &YmapReadDocument {
          schema_version: YmapReadDocument::SCHEMA_VERSION,
          source,
          hash_names,
          model,
        },
      )?;
      files.push(ReadFileResult {
        origin: selected.origin,
        input: selected.input,
        output: selected.output,
        updated: true,
      });
    }
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
    let inventory = inventoried_files(&self.source_cache_dir)?;
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

  /// The files of a vanilla version, the latest when `version` is `None`, matching
  /// [`Self::files`], marking in `matched` the names that match.
  fn vanilla_files(
    &self,
    version: Option<&str>,
    matched: &mut [bool],
  ) -> Result<Vec<Selected>> {
    let manifest = load_manifest(&self.vanilla_dir)?;
    let version = match version {
      None => manifest.versions.last().ok_or("The vanilla archive has no version")?.id.clone(),
      Some(version) => {
        let matching = manifest
          .versions
          .iter()
          .filter(|candidate| {
            candidate.id.eq_ignore_ascii_case(version)
              || candidate
                .id
                .split_once('-')
                .is_some_and(|(_, name)| name.eq_ignore_ascii_case(version))
          })
          .collect::<Vec<_>>();
        match matching[..] {
          [version] => version.id.clone(),
          [] => return Err(format!("Unknown vanilla version {version}").into()),
          _ => return Err(format!("Vanilla version {version} is ambiguous").into()),
        }
      }
    };
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
      let extension = extension(&file_name).to_ascii_lowercase();
      selected.push(Selected {
        output: self
          .output_dir
          .join("vanilla")
          .join(&version)
          .join(&extension)
          .join(format!("{file_name}.json")),
        input: self.vanilla_dir.join(&file.object),
        origin: ReadOrigin::Vanilla {
          version: version.clone(),
          file_name,
        },
      });
    }
    Ok(selected)
  }

  /// The header of the document at `path`, unless it is missing or unreadable.
  fn header(
    &self,
    path: &Path,
  ) -> Option<Header> {
    let file = fs::File::open(path).ok()?;
    serde_json::from_reader(BufReader::new(file)).ok()
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
