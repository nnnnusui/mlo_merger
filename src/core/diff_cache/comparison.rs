//! Parsed-state comparison and closest-version selection.

use super::{
  Result,
  types::{CandidateScore, Variant},
};
use crate::core::format::{gamefile::meta_resource::jenk_hash, ymap::model::Ymap};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) struct ModelState {
  pub(super) model: Ymap,
  pub(super) comparison: Value,
}

impl ModelState {
  /// Export names and block bookkeeping must not influence baseline selection.
  pub(super) fn new(model: Ymap) -> Result<Self> {
    let mut comparison = serde_json::to_value(&model)?;
    let fields = comparison.as_object_mut().ok_or("YMAP model is not an object")?;
    fields.remove("name");
    fields.remove("block");
    Ok(Self {
      model,
      comparison,
    })
  }
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
pub(super) fn distance(
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

/// Ties select the newest content-change stage, not a later unchanged DLC stage.
pub(super) fn best_candidate(
  scores: &[CandidateScore],
  variants: &[Variant],
) -> Result<usize> {
  scores
    .iter()
    .enumerate()
    .min_by_key(|(index, score)| {
      (score.difference_count, std::cmp::Reverse(variants[*index].index))
    })
    .map(|(index, _)| index)
    .ok_or_else(|| "No vanilla candidates".into())
}
