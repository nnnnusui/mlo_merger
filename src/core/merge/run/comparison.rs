use crate::core::format::{gamefile::meta::jenk_hash, ymap::model::Ymap};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn ymap_distance(
  before: &Ymap,
  after: &Ymap,
) -> Result<usize, Box<dyn std::error::Error>> {
  let before = comparison(before)?;
  let after = comparison(after)?;
  Ok(distance(&before, &after))
}

/// Export names and block bookkeeping must not influence baseline selection.
fn comparison(model: &Ymap) -> Result<Value, Box<dyn std::error::Error>> {
  let mut comparison = serde_json::to_value(model)?;
  let fields = comparison.as_object_mut().ok_or("YMAP model is not an object")?;
  fields.remove("name");
  fields.remove("block");
  Ok(comparison)
}

fn reference(value: &str) -> u32 {
  if value.is_empty() {
    return 0;
  }
  value
    .strip_prefix("hash_")
    .and_then(|hash| u32::from_str_radix(hash, 16).ok())
    .unwrap_or_else(|| jenk_hash(&value.to_ascii_lowercase()))
}

/// Counts unequal leaves and unmatched entries; arrays remain ordered and hash spellings compare equally.
fn distance(
  before: &Value,
  after: &Value,
) -> usize {
  match (before, after) {
    (Value::Object(before), Value::Object(after)) => {
      let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
      keys
        .into_iter()
        .map(|key| match (before.get(key), after.get(key)) {
          (Some(before), Some(after)) => distance(before, after),
          _ => 1,
        })
        .sum()
    }
    (Value::Array(before), Value::Array(after)) => (0..before.len().max(after.len()))
      .map(|index| match (before.get(index), after.get(index)) {
        (Some(before), Some(after)) => distance(before, after),
        _ => 1,
      })
      .sum(),
    (Value::String(before), Value::String(after))
      if before.starts_with("hash_") || after.starts_with("hash_") =>
    {
      usize::from(reference(before) != reference(after))
    }
    _ => usize::from(before != after),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn distance_is_symmetric_and_counts_model_changes() {
    let before = serde_json::json!({"entity_map": {"1": {"x": 1, "y": 2}}, "parent": "map"});
    let after =
      serde_json::json!({"entity_map": {"1": {"x": 3, "y": 4}, "2": {"x": 7}}, "parent": "map"});
    assert_eq!(distance(&before, &after), 3);
    assert_eq!(distance(&after, &before), 3);
    assert_eq!(distance(&before, &before), 0);
    assert_eq!(
      distance(
        &Value::String("parent".into()),
        &Value::String(format!("hash_{:08X}", jenk_hash("parent")))
      ),
      0
    );
  }
}
