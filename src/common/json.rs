use super::Result;
use serde::{Serialize, de::DeserializeOwned};
use std::{
  any::{Any, TypeId},
  collections::HashMap,
  fs,
  io::{BufWriter, Write},
  path::{Path, PathBuf},
  sync::{Arc, Mutex, OnceLock},
};

pub(crate) fn write_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let mut writer = BufWriter::new(fs::File::create(path)?);
  serde_json::to_writer_pretty(&mut writer, value)?;
  writer.flush()?;
  Ok(())
}

/// Reads the JSON file at `path`.
///
/// The file is read whole before parsing: serde_json parses a slice several times faster than a
/// reader, which matters for metadata of several megabytes read on every run.
pub(crate) fn read_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
  Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// A decoded file and the bytes it was decoded from.
struct Shared {
  bytes: Vec<u8>,
  value: Arc<dyn Any + Send + Sync>,
}

/// Files decoded by [`read_shared`], by path and decoded type.
static SHARED: OnceLock<Mutex<HashMap<(PathBuf, TypeId), Shared>>> = OnceLock::new();

/// The file at `path` decoded by `decode`, shared with earlier calls in this process that
/// decoded the same bytes to the same type.
///
/// The pipeline's stages each read the metadata they own; several read the same files of
/// several megabytes, such as the vanilla archive's manifest. Comparing the bytes costs far less
/// than decoding them again, and unlike modification times, which some file systems record
/// coarsely, it cannot miss a rewrite.
pub(crate) fn read_shared<T: Send + Sync + 'static>(
  path: &Path,
  decode: impl FnOnce(&[u8]) -> Result<T>,
) -> Result<Arc<T>> {
  let path = path.canonicalize()?;
  let key = (path.clone(), TypeId::of::<T>());
  let bytes = fs::read(&path)?;
  let shared = SHARED.get_or_init(Default::default);
  let lock = || shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
  if let Some(entry) = lock().get(&key)
    && entry.bytes == bytes
    && let Ok(value) = entry.value.clone().downcast::<T>()
  {
    return Ok(value);
  }
  let value = Arc::new(decode(&bytes)?);
  lock().insert(
    key,
    Shared {
      bytes,
      value: value.clone(),
    },
  );
  Ok(value)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};

  #[test]
  fn shared_files_are_decoded_again_only_when_their_bytes_change() {
    let root = std::env::temp_dir().join(format!("read_shared_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("value.json");
    let decodes = AtomicUsize::new(0);
    let read = || {
      read_shared(&path, |bytes| {
        decodes.fetch_add(1, Ordering::Relaxed);
        Ok(serde_json::from_slice::<u32>(bytes)?)
      })
      .unwrap()
    };
    fs::write(&path, b"1").unwrap();
    assert_eq!(*read(), 1);
    assert_eq!(*read(), 1);
    assert_eq!(decodes.load(Ordering::Relaxed), 1);
    // Rewritten with the same size, possibly within the same modification time.
    fs::write(&path, b"2").unwrap();
    assert_eq!(*read(), 2);
    assert_eq!(decodes.load(Ordering::Relaxed), 2);
    // Another type decodes the same file separately.
    assert_eq!(*read_shared(&path, |bytes| Ok(bytes.to_vec())).unwrap(), b"2");
    assert_eq!(*read(), 2);
    assert_eq!(decodes.load(Ordering::Relaxed), 2);
    fs::remove_dir_all(root).unwrap();
  }
}
