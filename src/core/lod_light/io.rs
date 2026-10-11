use super::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  fs,
  io::{BufReader, Read},
  path::{Component, Path},
};

pub(super) fn atomic_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let temporary = path.with_file_name(format!(
    ".{}.{}-{}.tmp",
    path.file_name().ok_or("Missing JSON filename")?.to_string_lossy(),
    std::process::id(),
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()
  ));
  let result = (|| {
    crate::core::vanilla::write_json(&temporary, value)?;
    fs::rename(&temporary, path)?;
    Ok(())
  })();
  let _ = fs::remove_file(temporary);
  result
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "LightExportFingerprint"))]
pub(super) struct Fingerprint {
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl Fingerprint {
  pub fn read(
    path: &Path,
    previous: Option<&Self>,
    force: bool,
  ) -> Result<Self> {
    let metadata = fs::metadata(path)?;
    let time = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
    if !force
      && let Some(previous) = previous
      && previous.size == metadata.len()
      && previous.modified_seconds == time.as_secs()
      && previous.modified_nanos == time.subsec_nanos()
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
      modified_seconds: time.as_secs(),
      modified_nanos: time.subsec_nanos(),
    })
  }
}

pub(super) fn safe_relative(value: &str) -> Result<()> {
  if value.is_empty()
    || value.contains(['\\', '\r', '\n', ':'])
    || Path::new(value).components().any(|part| !matches!(part, Component::Normal(_)))
    || value.split('/').any(|part| part.is_empty() || part == "." || part == "..")
  {
    return Err(format!("Unsafe light editor path: {value}").into());
  }
  Ok(())
}

pub(super) fn reject_symlinks(path: &Path) -> Result<()> {
  for ancestor in path.ancestors() {
    if fs::symlink_metadata(ancestor).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Light editor path contains a symlink: {}", path.display()).into());
    }
  }
  Ok(())
}
