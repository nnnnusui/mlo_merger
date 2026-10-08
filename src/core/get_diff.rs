use std::path::{Path, PathBuf};

use crate::core::{
  format::ymap::diff::YmapDiff,
  vanilla::{read_ymap, write_json},
};
use walkdir::WalkDir;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Writes a source YMAP's merge-oriented diff against its latest vanilla-cache baseline.
///
/// The output is written to `<output_dir>/<source filename>.diff.json`.
pub fn generate_ymap_diff(
  source_input: &Path,
  vanilla_cache_dir: &Path,
  output_path: &Path,
) -> Result<Vec<PathBuf>> {
  let sources = collect_source_ymaps(source_input)?;
  let mut outputs = Vec::with_capacity(sources.len());
  for source_path in &sources {
    let name = source_path.file_name().ok_or("Source YMAP has no filename")?;
    let vanilla_path = vanilla_cache_dir.join("latest/ymap").join(name);
    if !vanilla_path.is_file() {
      return Err(
        format!("Latest vanilla YMAP baseline is missing: {}", vanilla_path.display()).into(),
      );
    }
    let vanilla = read_ymap(&vanilla_path)?;
    let source = read_ymap(source_path)?;
    let diff = YmapDiff::extract_for_merge(&vanilla, &source);
    let mut diff_json = serde_json::to_value(&diff)?;
    omit_occlude_model_baseline_triangles(&mut diff_json);
    let output = diff_output_path(source_input, source_path, sources.len(), output_path)?;
    if let Some(parent) = output.parent() {
      std::fs::create_dir_all(parent)?;
    }
    write_json(&output, &diff_json)?;
    outputs.push(output);
  }
  Ok(outputs)
}

fn omit_occlude_model_baseline_triangles(diff: &mut serde_json::Value) {
  let Some(diffs) = diff.get_mut("occlude_model_diffs").and_then(serde_json::Value::as_array_mut)
  else {
    return;
  };
  for item in diffs {
    let Some(variants) = item.as_object_mut() else {
      continue;
    };
    if let Some(model) = variants.get_mut("Removed").and_then(serde_json::Value::as_object_mut) {
      model.remove("triangles");
    }
    if let Some(model) = variants
      .get_mut("Modified")
      .and_then(|modified| modified.get_mut("vanilla"))
      .and_then(serde_json::Value::as_object_mut)
    {
      model.remove("triangles");
    }
  }
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
  fn omits_baseline_triangles_but_keeps_triangle_diffs_and_added_geometry() {
    let mut diff = serde_json::json!({
      "occlude_model_diffs": [
        {"Removed": {"triangles": ["baseline removed"]}},
        {"Modified": {
          "vanilla": {"triangles": ["baseline modified"]},
          "triangle_diffs": [{"Added": "changed triangle"}]
        }},
        {"Added": {"triangles": ["new model geometry"]}}
      ]
    });

    omit_occlude_model_baseline_triangles(&mut diff);

    assert!(diff["occlude_model_diffs"][0]["Removed"].get("triangles").is_none());
    assert!(diff["occlude_model_diffs"][1]["Modified"]["vanilla"].get("triangles").is_none());
    assert_eq!(
      diff["occlude_model_diffs"][1]["Modified"]["triangle_diffs"][0]["Added"],
      "changed triangle"
    );
    assert_eq!(diff["occlude_model_diffs"][2]["Added"]["triangles"][0], "new model geometry");
  }

  #[test]
  fn rejects_non_ymap_source_paths() {
    let error =
      generate_ymap_diff(Path::new("source.ybn"), Path::new("vanilla-cache"), Path::new("out"))
        .unwrap_err();
    assert!(error.to_string().contains("YMAP files only"));
  }
}
