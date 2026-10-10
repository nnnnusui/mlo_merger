use super::Result;

pub(super) const FILE_NAME: &str = "source_exclusions.json";

pub(crate) fn parse_exclusions(value: &str) -> Result<Vec<String>> {
  if value.trim().is_empty() {
    return Ok(Vec::new());
  }
  let mut patterns = Vec::new();
  for item in value.split(',') {
    let pattern = item.trim().replace('\\', "/").to_ascii_lowercase();
    if pattern.is_empty() || pattern.contains(['\r', '\n']) {
      return Err(
        "Exclusions must be nonempty comma-separated resource names or partial paths".into(),
      );
    }
    if !patterns.contains(&pattern) {
      patterns.push(pattern);
    }
  }
  Ok(patterns)
}

pub(super) fn is_excluded(
  id: &str,
  patterns: &[String],
) -> bool {
  let id = id.replace('\\', "/").to_ascii_lowercase();
  let name = id.rsplit('/').next().unwrap_or(&id);
  patterns.iter().any(|pattern| name == pattern || id.contains(pattern))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn matches_names_and_partial_paths_without_glob_interpretation() {
    let patterns = parse_exclusions(" Resource_A, [GABZ]\\shops, Resource_A ").unwrap();
    assert_eq!(patterns, ["resource_a", "[gabz]/shops"]);
    assert!(is_excluded("[group]/RESOURCE_A", &patterns));
    assert!(is_excluded("[gabz]/shops/resource_b", &patterns));
    assert!(!is_excluded("[gabz]/other/resource_b", &patterns));
    assert!(parse_exclusions(" ").unwrap().is_empty());
    assert!(parse_exclusions("resource_a,,resource_b").is_err());
  }
}
