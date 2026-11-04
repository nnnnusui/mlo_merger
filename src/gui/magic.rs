//! Locating and downloading CodeWalker's `magic.dat`.

use std::{error::Error, fs, io::Read, path::Path};

/// CodeWalker's `magic.dat` on GitHub, shown to the user.
pub const PAGE_URL: &str =
  "https://github.com/dexyfex/CodeWalker/blob/master/CodeWalker.Core/Resources/magic.dat";

/// The raw file behind [`PAGE_URL`].
const DOWNLOAD_URL: &str =
  "https://raw.githubusercontent.com/dexyfex/CodeWalker/master/CodeWalker.Core/Resources/magic.dat";

/// Upper bound for the download; the file is well under 1 MiB.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// Downloads `magic.dat` from CodeWalker's repository to `destination`.
///
/// The file is written beside `destination` first and renamed into place, so a failed
/// download leaves any existing file untouched.
pub fn download(destination: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
  tracing::info!("Downloading magic.dat from {DOWNLOAD_URL}");
  let response = ureq::get(DOWNLOAD_URL)
    .header("User-Agent", concat!("mlo_merger/", env!("CARGO_PKG_VERSION")))
    .call()?;
  let mut bytes = Vec::new();
  response.into_body().into_reader().take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
  if bytes.is_empty() || bytes.len() as u64 > MAX_BYTES {
    return Err(format!("Unexpected magic.dat size: {} bytes", bytes.len()).into());
  }
  if let Some(parent) = destination.parent() {
    fs::create_dir_all(parent)?;
  }
  let partial = destination.with_extension("dat.download");
  fs::write(&partial, &bytes)?;
  fs::rename(&partial, destination).inspect_err(|_| {
    let _ = fs::remove_file(&partial);
  })?;
  tracing::info!("Saved magic.dat to {} ({} bytes)", destination.display(), bytes.len());
  Ok(())
}
