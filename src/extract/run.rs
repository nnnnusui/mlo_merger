use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ExtractYmap {
  pub input_dir: PathBuf,
  pub output_dir: PathBuf,
}

impl ExtractYmap {
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    let output_content_already_exists =
      self.output_dir.exists() && fs::read_dir(&self.output_dir)?.next().is_some();
    if output_content_already_exists {
      let abs_path = self.output_dir.canonicalize().unwrap_or_else(|_| {
        std::env::current_dir()
          .ok()
          .and_then(|cwd| cwd.join(&self.output_dir).canonicalize().ok())
          .unwrap_or_else(|| self.output_dir.clone())
      });
      return Err(format!("Output directory is not empty: {}", abs_path.display()).into());
    }

    let resource_dirs = get_resource_directories(&self.input_dir)?;
    println!("Found {} resource directories:", resource_dirs.len());

    for resource_dir in &resource_dirs {
      extract_ymap_files(&self.output_dir, resource_dir)?;
    }

    println!("Extraction completed.");
    Ok(())
  }
}

/// Explores FiveM resource directories
///
/// - Targets directories directly under the specified path
/// - Recursively explores directories enclosed in `[...]`
/// - Supports nested `[...]/[...]` structures
fn get_resource_directories(
  base_dir: &PathBuf,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
  let mut resources = explore_directory(base_dir);
  resources.sort();
  Ok(resources)
}

fn explore_directory(dir: &PathBuf) -> Vec<PathBuf> {
  match fs::read_dir(dir) {
    Err(_) => Vec::new(),
    Ok(read_dir) => read_dir
      .filter_map(|e| e.ok())
      .map(|e| e.path())
      .filter(|p| p.is_dir())
      .fold(Vec::new(), |mut acc, path| {
        let is_bracket = path
          .file_name()
          .and_then(|n| n.to_str())
          .map(|name| name.starts_with('[') && name.ends_with(']'))
          .unwrap_or(false);

        if is_bracket {
          acc.extend(explore_directory(&path));
        } else {
          acc.push(path);
        }
        acc
      }),
  }
}

/// Extracts .ymap files from a resource directory
fn extract_ymap_files(
  output_dir: &Path,
  resource_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
  let dir_name = resource_dir
    .file_name()
    .and_then(|n| n.to_str())
    .ok_or("Invalid directory name")?;

  let ymap_files = collect_ymap_files(resource_dir);

  if ymap_files.is_empty() {
    return Ok(());
  }

  let output_subdir = output_dir.join(dir_name);
  fs::create_dir_all(&output_subdir)?;

  println!("Extracting from {}:", dir_name);
  for src_path in ymap_files {
    if let Some(filename) = src_path.file_name() {
      let dest_path = output_subdir.join(filename);
      fs::copy(&src_path, &dest_path)?;
      println!("  - {}", filename.to_string_lossy());
    }
  }

  Ok(())
}

/// Recursively collects all .ymap files from a directory
fn collect_ymap_files(dir: &Path) -> Vec<PathBuf> {
  let Ok(entries) = fs::read_dir(dir) else {
    return Vec::new();
  };

  entries
    .filter_map(|e| e.ok())
    .flat_map(|entry| {
      let path = entry.path();
      if path.is_dir() {
        collect_ymap_files(&path)
      } else if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("ymap"))
        .unwrap_or(false)
      {
        vec![path]
      } else {
        Vec::new()
      }
    })
    .collect()
}
