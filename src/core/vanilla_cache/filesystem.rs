use super::*;

pub(super) struct Staging(pub(super) PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

pub(super) fn relative_link_target(
  link_parent: &Path,
  source: &Path,
) -> PathBuf {
  let from: Vec<_> = link_parent.components().collect();
  let to: Vec<_> = source.components().collect();
  let common = from.iter().zip(&to).take_while(|(left, right)| left == right).count();
  if common == 0 {
    return source.to_path_buf();
  }
  let mut relative = PathBuf::new();
  for component in from.iter().skip(common) {
    if matches!(component, Component::Normal(_)) {
      relative.push("..");
    }
  }
  for component in to.iter().skip(common) {
    relative.push(component.as_os_str());
  }
  relative
}

pub(super) fn create_file_symlink(
  target: &Path,
  link: &Path,
) -> Result<()> {
  fs::symlink_file(target, link)?;
  Ok(())
}

pub(super) fn prepare_output_path(
  output: &Path,
  vanilla_dir: &Path,
) -> Result<PathBuf> {
  let absolute =
    if output.is_absolute() { output.to_path_buf() } else { fs::current_dir()?.join(output) };
  let current_dir = fs::canonicalize(&fs::current_dir()?)?;
  if absolute == current_dir
    || current_dir.starts_with(&absolute)
    || absolute.starts_with(vanilla_dir)
    || vanilla_dir.starts_with(&absolute)
  {
    return Err("Vanilla-cache output must not overlap the workspace or vanilla input".into());
  }
  if fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err(format!("Refusing to replace symlink output {}", absolute.display()).into());
  }
  if fs::exists(&absolute).unwrap_or(false) && !fs::is_dir(&absolute) {
    return Err(format!("Vanilla-cache output is not a directory: {}", absolute.display()).into());
  }
  let parent = absolute.parent().ok_or("Vanilla-cache output has no parent")?;
  fs::create_dir_all(parent)?;
  let parent = fs::canonicalize(parent)?;
  Ok(parent.join(absolute.file_name().ok_or("Vanilla-cache output has no name")?))
}

pub(super) fn create_staging_directory(output_dir: &Path) -> Result<PathBuf> {
  let parent = output_dir.parent().ok_or("Vanilla-cache output has no parent")?;
  let name = output_dir.file_name().ok_or("Vanilla-cache output has no name")?.to_string_lossy();
  let staging = parent.join(format!(".{name}.staging-{}", fs::process_id()));
  fs::create_dir(&staging)?;
  Ok(staging)
}

pub(super) fn publish_directory(
  staging: &Path,
  output: &Path,
) -> Result<()> {
  let backup = output.with_file_name(format!(
    ".{}.backup-{}",
    output.file_name().unwrap_or_default().to_string_lossy(),
    fs::process_id()
  ));
  let existed = fs::exists(output).unwrap_or(false);
  if existed {
    fs::rename(output, &backup)?;
  }
  if let Err(error) = fs::rename(staging, output) {
    if existed {
      fs::rename(&backup, output)?;
    }
    return Err(error.into());
  }
  if existed {
    fs::remove_dir_all(backup)?;
  }
  Ok(())
}

pub(super) fn write_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let mut writer = BufWriter::new(fs::File::create(path)?);
  serde_json::to_writer_pretty(&mut writer, value)?;
  writer.flush()?;
  Ok(())
}
