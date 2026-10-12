use super::Result;
use std::{
  fs,
  path::{Component, Path, PathBuf},
};

pub(crate) fn safe_relative(value: &str) -> Result<()> {
  if value.is_empty()
    || value.contains(['\\', '\r', '\n', ':'])
    || Path::new(value).components().any(|part| !matches!(part, Component::Normal(_)))
    || value.split('/').any(|part| part.is_empty() || part == "." || part == "..")
  {
    return Err(format!("Unsafe relative path: {value}").into());
  }
  Ok(())
}

pub(crate) fn reject_symlinks(path: &Path) -> Result<()> {
  for ancestor in path.ancestors() {
    if fs::symlink_metadata(ancestor).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Path contains a symlink: {}", path.display()).into());
    }
  }
  Ok(())
}

pub(crate) fn ensure_safe_path(
  root: &Path,
  path: &Path,
) -> Result<()> {
  let relative = path.strip_prefix(root)?;
  let mut current = root.to_path_buf();
  for component in relative.components() {
    if !matches!(component, Component::Normal(_)) {
      return Err("Unsafe path".into());
    }
    current.push(component);
    if fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Refusing symlink path: {}", current.display()).into());
    }
  }
  Ok(())
}
pub(crate) fn absolute_path(path: &Path) -> std::io::Result<PathBuf> {
  let absolute =
    if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir()?.join(path) };
  let mut normalized = PathBuf::new();
  for component in absolute.components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        if normalized.exists() {
          normalized = normalized.canonicalize()?;
        }
        normalized.pop();
      }
      component => normalized.push(component.as_os_str()),
    }
  }
  let mut ancestor = normalized;
  let mut missing = Vec::new();
  while !ancestor.exists() {
    missing.push(
      ancestor
        .file_name()
        .ok_or_else(|| {
          std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path has no existing ancestor")
        })?
        .to_os_string(),
    );
    ancestor.pop();
  }
  let mut path = ancestor.canonicalize()?;
  for component in missing.into_iter().rev() {
    path.push(component);
  }
  Ok(path)
}
