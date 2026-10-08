use super::*;

pub(in crate::core::merge) fn output_path(
  root: &Path,
  relative: &str,
) -> Result<PathBuf> {
  let mut path = root.to_path_buf();
  let relative = Path::new(relative);
  if relative.components().any(|part| !matches!(part, Component::Normal(_))) {
    return Err("Unsafe merge-cache output path".into());
  }
  for part in relative.components() {
    path.push(part);
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Refusing symlink merge output: {}", path.display()).into());
    }
  }
  Ok(path)
}

pub(super) fn file_name(path: &Path) -> Result<String> {
  Ok(
    path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or("Merge file name is not UTF-8")?
      .to_ascii_lowercase(),
  )
}

pub(super) fn retain_file(
  from: &Path,
  to: &Path,
) -> Result<()> {
  fs::create_dir_all(to.parent().ok_or("Merge file has no parent")?)?;
  if fs::hard_link(from, to).is_err() {
    fs::copy(from, to)?;
    fs::File::options()
      .write(true)
      .open(to)?
      .set_times(fs::FileTimes::new().set_modified(fs::metadata(from)?.modified()?))?;
  }
  Ok(())
}
