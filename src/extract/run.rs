use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct ExtractYmap {
  pub input_dir: PathBuf,
  pub output_dir: PathBuf,
  pub flatten: bool,
  pub vanilla_dir: Option<PathBuf>,
}

impl ExtractYmap {
  pub const FLATTEN_DELIMITER: &str = "___";
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

    fs::create_dir_all(&self.output_dir)?;

    // Collect vanilla ymap filenames if vanilla_dir is specified
    let vanilla_ymap_names: Option<HashSet<String>> = self.vanilla_dir.as_deref().map(|path| {
      collect_files_with_suffix(path, ".ymap.xml")
        .into_iter()
        .filter_map(|p| {
          p.file_name().and_then(|n| n.to_str()).map(|s| {
            // Remove .xml suffix from .ymap.xml files
            s.strip_suffix(".xml").unwrap_or(s).to_string()
          })
        })
        .collect()
    });

    let resource_dirs = get_resource_directories(&self.input_dir)?;
    println!("Found {} resource directories:", resource_dirs.len());
    println!(
      "Found {} vanilla YMAP files.",
      vanilla_ymap_names.as_ref().map_or(0, |s| s.len())
    );

    for resource_dir in &resource_dirs {
      extract_ymap_files(
        &self.output_dir,
        resource_dir,
        self.flatten,
        vanilla_ymap_names.as_ref(),
      )?;
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
          // Only include directories with fxmanifest.lua or __resource.lua
          let has_fxmanifest = path.join("fxmanifest.lua").exists();
          let has_resource = path.join("__resource.lua").exists();
          if has_fxmanifest || has_resource {
            acc.push(path);
          }
        }
        acc
      }),
  }
}

/// Extracts .ymap files from a resource directory
fn extract_ymap_files(
  output_dir: &Path,
  resource_dir: &Path,
  flatten: bool,
  vanilla_ymap_names: Option<&HashSet<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
  let dir_name = resource_dir
    .file_name()
    .and_then(|n| n.to_str())
    .ok_or("Invalid directory name")?;

  let ymap_files = collect_files_with_suffix(resource_dir, ".ymap");

  if ymap_files.is_empty() {
    return Ok(());
  }

  // Filter files based on vanilla_ymap_names if specified
  let filtered_files: Vec<_> = if let Some(vanilla_names) = vanilla_ymap_names {
    ymap_files
      .into_iter()
      .filter(|src_path| {
        src_path
          .file_name()
          .and_then(|n| n.to_str())
          .map(|filename| vanilla_names.contains(filename))
          .unwrap_or(false)
      })
      .collect()
  } else {
    ymap_files
  };

  if filtered_files.is_empty() {
    return Ok(());
  }

  if !flatten {
    fs::create_dir_all(output_dir.join(dir_name))?;
  }

  println!("Extracting from {}:", dir_name);
  for src_path in filtered_files {
    if let Some(filename) = src_path.file_name().and_then(|n| n.to_str()) {
      let dest_path = if flatten {
        output_dir.join(format!(
          "{}{}{}",
          dir_name,
          ExtractYmap::FLATTEN_DELIMITER,
          filename
        ))
      } else {
        output_dir.join(dir_name).join(filename)
      };
      fs::copy(&src_path, &dest_path)?;
      println!("  - {}", filename);
    }
  }

  Ok(())
}

/// Recursively collects all files with the specified suffix from a directory
fn collect_files_with_suffix(dir: &Path, suffix: &str) -> Vec<PathBuf> {
  WalkDir::new(dir)
    .into_iter()
    .filter_map(|e| e.ok())
    .map(|e| e.path().to_path_buf())
    .filter(|p| {
      p.is_file()
        && p
          .file_name()
          .and_then(|name| name.to_str())
          .map(|name| name.to_lowercase().ends_with(&suffix.to_lowercase()))
          .unwrap_or(false)
    })
    .collect()
}
