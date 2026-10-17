use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

use struct_diff::Diff;
use walkdir::WalkDir;

use crate::core::{
  config::matching,
  format::ymap::{compare::YmapDiffContext, model::Ymap, reference_hash},
  vanilla::{read_ymap, write_json},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Writes each source YMAP's differences from its latest vanilla-cache baseline, as
/// `compare` takes them (the `diffs` of a source-cache `.diff.json`).
///
/// Entity parents are read from the parent YMAP among the inputs when there is one, from the
/// latest vanilla cache otherwise. Tolerances come from `asset/config.toml`.
///
/// The output is written to `<output_dir>/<source filename>.diff.json`.
pub fn generate_ymap_diff(
  source_input: &Path,
  vanilla_cache_dir: &Path,
  output_path: &Path,
) -> Result<Vec<PathBuf>> {
  matching::with_config(|| generate(source_input, vanilla_cache_dir, output_path))?
}

fn generate(
  source_input: &Path,
  vanilla_cache_dir: &Path,
  output_path: &Path,
) -> Result<Vec<PathBuf>> {
  let sources = collect_source_ymaps(source_input)?;
  let vanilla_dir = vanilla_cache_dir.join("latest/ymap");
  let source_paths = by_reference(sources.iter().cloned());
  let vanilla_paths = by_reference(
    std::fs::read_dir(&vanilla_dir)?.filter_map(|entry| entry.ok().map(|entry| entry.path())),
  );
  let mut outputs = Vec::with_capacity(sources.len());
  for source_path in &sources {
    let name = source_path.file_name().ok_or("Source YMAP has no filename")?;
    let vanilla_path = vanilla_dir.join(name);
    if !vanilla_path.is_file() {
      return Err(
        format!("Latest vanilla YMAP baseline is missing: {}", vanilla_path.display()).into(),
      );
    }
    let vanilla = read_ymap(&vanilla_path)?;
    let source = read_ymap(source_path)?;
    let parent = |ymap: &Ymap, own: Option<&HashMap<u32, PathBuf>>| -> Result<Option<Ymap>> {
      let hash = reference_hash(&ymap.parent);
      let path = own.and_then(|paths| paths.get(&hash)).or_else(|| vanilla_paths.get(&hash));
      Ok(if hash == 0 { None } else { path.map(|path| read_ymap(path)).transpose()? })
    };
    let (vanilla_parent, source_parent) =
      (parent(&vanilla, None)?, parent(&source, Some(&source_paths))?);
    let ctx = YmapDiffContext::new(
      matching::current(),
      (&vanilla, vanilla_parent.as_ref().map(|parent| parent.entities.as_slice())),
      (&source, source_parent.as_ref().map(|parent| parent.entities.as_slice())),
    );
    let diff = vanilla.diff(&source, &ctx).unwrap_or_default();
    let output = diff_output_path(source_input, source_path, sources.len(), output_path)?;
    if let Some(parent) = output.parent() {
      std::fs::create_dir_all(parent)?;
    }
    write_json(&output, &diff)?;
    outputs.push(output);
  }
  Ok(outputs)
}

/// YMAP `paths` keyed by the reference hash of their file stem.
fn by_reference(paths: impl Iterator<Item = PathBuf>) -> HashMap<u32, PathBuf> {
  let mut by_hash = HashMap::new();
  for path in paths {
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
      && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ymap"))
    {
      by_hash.entry(reference_hash(stem)).or_insert(path);
    }
  }
  by_hash
}

fn diff_output_path(
  source_input: &Path,
  source_path: &Path,
  source_count: usize,
  output_path: &Path,
) -> Result<PathBuf> {
  let name = source_path.file_name().ok_or("Source YMAP has no filename")?;
  if source_count == 1 {
    return Ok(if output_path == Path::new(".") {
      output_path.join(format!("{}.diff.json", name.to_string_lossy()))
    } else {
      output_path.to_path_buf()
    });
  }
  let relative = source_path.strip_prefix(source_input)?;
  let relative_output = relative.with_file_name(format!("{}.diff.json", name.to_string_lossy()));
  Ok(output_path.join(relative_output))
}

fn collect_source_ymaps(source_input: &Path) -> Result<Vec<PathBuf>> {
  let mut sources = if source_input.is_file() {
    if !source_input.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ymap")) {
      return Err("Single-input --get-diff currently supports YMAP files only".into());
    }
    vec![source_input.to_path_buf()]
  } else if source_input.is_dir() {
    WalkDir::new(source_input)
      .follow_links(false)
      .into_iter()
      .filter_map(|entry| match entry {
        Ok(entry)
          if entry.file_type().is_file()
            && entry
              .path()
              .extension()
              .is_some_and(|extension| extension.eq_ignore_ascii_case("ymap")) =>
        {
          Some(Ok(entry.into_path()))
        }
        Ok(_) => None,
        Err(error) => Some(Err(error)),
      })
      .collect::<std::result::Result<Vec<_>, _>>()?
  } else {
    if !source_input.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ymap")) {
      return Err("Single-input --get-diff currently supports YMAP files only".into());
    }
    return Err(format!("YMAP input does not exist: {}", source_input.display()).into());
  };
  sources.sort();
  if sources.is_empty() {
    return Err(format!("No YMAP files found in {}", source_input.display()).into());
  }
  Ok(sources)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn names_diff_output_after_the_source_file() {
    assert_eq!(
      diff_output_path(
        Path::new("resources/my_map.ymap"),
        Path::new("resources/my_map.ymap"),
        1,
        Path::new("."),
      )
      .unwrap(),
      PathBuf::from("./my_map.ymap.diff.json")
    );
  }

  #[test]
  fn output_path_is_a_file_for_one_input_and_directory_for_multiple_inputs() {
    assert_eq!(
      diff_output_path(
        Path::new("resources/my_map.ymap"),
        Path::new("resources/my_map.ymap"),
        1,
        Path::new("chosen.json"),
      )
      .unwrap(),
      PathBuf::from("chosen.json")
    );
    assert_eq!(
      diff_output_path(
        Path::new("resources"),
        Path::new("resources/stream/my_map.ymap"),
        2,
        Path::new("out"),
      )
      .unwrap(),
      PathBuf::from("out/stream/my_map.ymap.diff.json")
    );
  }

  #[test]
  fn rejects_non_ymap_source_paths() {
    let error =
      generate_ymap_diff(Path::new("source.ybn"), Path::new("vanilla-cache"), Path::new("out"))
        .unwrap_err();
    assert!(error.to_string().contains("YMAP files only"));
  }
}
