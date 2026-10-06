use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::common::function::collect_files_with_suffix;

/// Discovers one resource or a resources root using the shared manifest-aware explorer.
#[allow(
  clippy::ptr_arg,
  reason = "Preserve the existing extraction API while the shared explorer accepts &Path."
)]
pub fn get_resource_directories(
  base_dir: &PathBuf
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
  crate::core::common::function::get_resource_directories(base_dir).map_err(Into::into)
}

#[derive(Debug, Clone)]
pub struct ExtractYmap {
  pub input_dir: PathBuf,
  pub output_dir: PathBuf,
  pub flatten: bool,
  pub vanilla_dir: Option<PathBuf>,
}

impl ExtractYmap {
  pub const FLATTEN_DELIMITER: &str = "___";
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    let vanilla_ymap_names = self.vanilla_dir.as_deref().map(|path| {
      collect_files_with_suffix(path, ".ymap.xml")
        .into_iter()
        .filter_map(|path| {
          path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.strip_suffix(".xml").unwrap_or(name).to_string())
        })
        .collect::<HashSet<_>>()
    });
    self.run_with_filter(vanilla_ymap_names.as_ref())
  }

  /// Extracts only YMAP files whose basenames are present in the supplied vanilla set.
  pub fn run_with_vanilla_names(
    &self,
    vanilla_ymap_names: &HashSet<String>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_with_filter(Some(vanilla_ymap_names))
  }

  fn run_with_filter(
    &self,
    vanilla_ymap_names: Option<&HashSet<String>>,
  ) -> Result<(), Box<dyn std::error::Error>> {
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

    let resource_dirs = get_resource_directories(&self.input_dir)?;
    log::info!("Found {} resource directories:", resource_dirs.len());
    log::info!("Found {} vanilla YMAP files.", vanilla_ymap_names.as_ref().map_or(0, |s| s.len()));

    let extraction_results: Vec<_> = resource_dirs
      .into_iter()
      .map(|resource_dir| {
        let result = extract_ymap_files(&self.output_dir, &resource_dir, vanilla_ymap_names);
        (resource_dir, result)
      })
      .collect();
    let target_notfound_dirs = extraction_results
      .iter()
      .filter(|(_, result)| result.as_ref().map_or(true, |v| v.is_empty()))
      .map(|(dir, _)| dir);

    let extracted_ymap_source_paths: Vec<_> = extraction_results
      .iter()
      .filter_map(|(_, result)| result.as_ref().ok())
      .flatten()
      .cloned()
      .collect();

    let omit_list_path = self.output_dir.join("_extracted_ymaps.txt");
    let mut omit_list_file = fs::File::create(&omit_list_path)?;
    for src_path in extracted_ymap_source_paths {
      let relative_path = src_path.strip_prefix(&self.input_dir).unwrap();
      use std::io::Write;
      writeln!(omit_list_file, "{}", relative_path.display())?;
    }

    let notfound_list_path = self.output_dir.join("_notextracted_resources.txt");
    let mut notfound_list_file = fs::File::create(&notfound_list_path)?;
    for dir in target_notfound_dirs {
      let relative_path = dir.strip_prefix(&self.input_dir).unwrap();
      use std::io::Write;
      writeln!(notfound_list_file, "{}", relative_path.display())?;
    }

    log::info!("Extraction completed.");
    Ok(())
  }
}

/// Extracts .ymap files from a resource directory
fn extract_ymap_files(
  output_dir: &Path,
  resource_dir: &Path,
  vanilla_ymap_names: Option<&HashSet<String>>,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
  let dir_name =
    resource_dir.file_name().and_then(|n| n.to_str()).ok_or("Invalid directory name")?;

  let ymap_files = collect_files_with_suffix(resource_dir, ".ymap");

  // Filter files based on vanilla_ymap_names if specified
  let filtered_files: Vec<_> = if let Some(vanilla_names) = vanilla_ymap_names {
    ymap_files
      .into_iter()
      .filter(|src_path| {
        src_path
          .file_name()
          .and_then(|n| n.to_str())
          .map(|filename| vanilla_names.iter().any(|name| name.eq_ignore_ascii_case(filename)))
          .unwrap_or(false)
      })
      .collect()
  } else {
    ymap_files
  };

  log::info!("Extracting from {}:", dir_name);
  for src_path in &filtered_files {
    if let Some(filename) = src_path.file_name().and_then(|n| n.to_str()) {
      let dest_path =
        output_dir.join(format!("{}{}{}", dir_name, ExtractYmap::FLATTEN_DELIMITER, filename));
      fs::copy(src_path, &dest_path)?;
      log::info!("  - {}", filename);
    }
  }

  Ok(filtered_files)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn extraction_accepts_latest_cache_filename_filter() {
    let root =
      std::env::temp_dir().join(format!("extract_ymap_cache_filter_{}", std::process::id()));
    let source = root.join("source");
    let resource = source.join("resource_a");
    let stream = resource.join("stream");
    let output = root.join("extracted");
    fs::create_dir_all(&stream).unwrap();
    fs::write(resource.join("fxmanifest.lua"), []).unwrap();
    fs::write(stream.join("cached.ymap"), b"cached").unwrap();
    fs::write(stream.join("uncached.ymap"), b"uncached").unwrap();
    let command = ExtractYmap {
      input_dir: source,
      output_dir: output.clone(),
      flatten: true,
      vanilla_dir: None,
    };
    command.run_with_vanilla_names(&HashSet::from(["cached.ymap".to_string()])).unwrap();
    assert_eq!(fs::read(output.join("resource_a___cached.ymap")).unwrap(), b"cached");
    assert!(!output.join("resource_a___uncached.ymap").exists());
    fs::remove_dir_all(root).unwrap();
  }
}
