use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::core::merge) struct Fingerprint {
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl Fingerprint {
  pub(in crate::core::merge) fn read(
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

  pub(in crate::core::merge) fn same_content(
    &self,
    other: &Self,
  ) -> bool {
    self.sha256 == other.sha256 && self.size == other.size
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::core::merge) struct InputFile {
  pub path: PathBuf,
  pub resource: Option<String>,
  pub original_path: Option<PathBuf>,
  pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::core::merge) struct OutputFile {
  pub path: String,
  pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(in crate::core::merge) struct FileRecord {
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
pub(in crate::core::merge) struct MergeMetadata {
  pub format_version: u32,
  pub algorithm_version: u32,
  pub generated_at: String,
  pub files: BTreeMap<String, FileRecord>,
}

impl MergeMetadata {
  pub(in crate::core::merge) fn load(output: &Path) -> Result<Option<Self>> {
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

pub(in crate::core::merge) fn reusable(
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
pub(super) fn signature(files: &BTreeMap<String, FileRecord>) -> Result<String> {
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
}
