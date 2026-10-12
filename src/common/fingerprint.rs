use super::{Result, path::reject_symlinks};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  fs,
  io::{BufReader, Read},
  path::Path,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "LightExportFingerprint"))]
pub(crate) struct Fingerprint {
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl Fingerprint {
  pub(crate) fn read(
    path: &Path,
    previous: Option<&Self>,
    force: bool,
  ) -> Result<Self> {
    reject_symlinks(path)?;
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
