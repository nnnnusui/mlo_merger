//! Shared geometry types, filesystem, path, JSON and fingerprint primitives.

pub(crate) mod filesystem;
pub(crate) mod fingerprint;
pub(crate) mod function;
pub(crate) mod json;
mod r#macro;
pub(crate) mod path;
pub mod position;
pub mod rotation;
pub mod triangle;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[cfg(test)]
mod tests {
  use super::*;
  use std::{fs, path::PathBuf};

  struct TemporaryDirectory(PathBuf);

  impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.0);
    }
  }

  #[test]
  fn json_fingerprints_and_file_moves_preserve_content_and_timestamps() {
    let root = std::env::temp_dir().join(format!("common_io_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _cleanup = TemporaryDirectory(root.clone());
    let source = root.join("input.json");
    let read =
      |path: &PathBuf| serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap());
    let value = serde_json::json!({"resource": "maps", "files": ["map.ymap"]});
    json::write_json(&source, &value).unwrap();
    assert_eq!(read(&source).unwrap(), value);
    let before = fingerprint::Fingerprint::read(&source, None, false).unwrap();
    assert_eq!(fingerprint::Fingerprint::read(&source, Some(&before), false).unwrap(), before);
    assert_eq!(
      fingerprint::Fingerprint::read(&source, Some(&before), true).unwrap().sha256,
      before.sha256
    );
    let changed_time =
      fs::metadata(&source).unwrap().modified().unwrap() + std::time::Duration::from_secs(5);
    fs::OpenOptions::new()
      .write(true)
      .open(&source)
      .unwrap()
      .set_times(fs::FileTimes::new().set_modified(changed_time))
      .unwrap();
    let touched = fingerprint::Fingerprint::read(&source, Some(&before), false).unwrap();
    assert_eq!(touched.sha256, before.sha256);
    assert_ne!(touched.modified_seconds, before.modified_seconds);
    let destination = root.join("moved.json");
    filesystem::move_file(&source, &destination).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::metadata(&destination).unwrap().modified().unwrap(), changed_time);
    assert_eq!(read(&destination).unwrap(), value);
    fs::write(&destination, b"changed and malformed JSON").unwrap();
    assert_ne!(
      fingerprint::Fingerprint::read(&destination, Some(&touched), false).unwrap().sha256,
      touched.sha256
    );
    assert!(read(&destination).is_err());
  }

  #[test]
  fn path_validation_rejects_traversal_and_symlinks() {
    let root = std::env::temp_dir().join(format!("common_paths_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _cleanup = TemporaryDirectory(root.clone());
    let file = root.join("file");
    fs::write(&file, b"data").unwrap();
    path::ensure_safe_path(&root, &file).unwrap();
    assert!(path::ensure_safe_path(&root, &root.join("../escape")).is_err());
    assert_eq!(
      path::absolute_path(&root.join("missing/../new/file")).unwrap(),
      root.canonicalize().unwrap().join("new/file")
    );
    #[cfg(unix)]
    {
      let link = root.join("link");
      std::os::unix::fs::symlink(&file, &link).unwrap();
      assert!(path::reject_symlinks(&link).is_err());
      assert!(path::ensure_safe_path(&root, &link).is_err());
      assert!(filesystem::read_bytes(&link).is_err());
      assert!(fingerprint::Fingerprint::read(&link, None, false).is_err());
      assert_eq!(fs::read(&file).unwrap(), b"data");
    }
  }
}
