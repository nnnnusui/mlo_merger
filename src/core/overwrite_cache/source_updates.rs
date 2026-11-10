//! What happens to resource overwrites when their source changes (`source_updates/`).
//!
//! An overwrite is written against one version of its source, which it names by SHA-256. When
//! the resource ships another version, the overwrite no longer applies. Building the overwrite
//! cache then compares the source's diff document from the version the overwrite was written
//! against, which the source cache keeps in its history (see
//! [`crate::core::source_cache::history`]), with the current one:
//!
//! - Every difference the overwrite left out of the earlier diff is either left out of the
//!   current diff by the same selector, or gone from it, the resource no longer making it, and
//!   the selectors leave out nothing more: the overwrite is rewritten in the overwrites against
//!   the current version, dropping the selectors that match nothing any more.
//! - Otherwise, as when a difference it left out changed (an entity with the same GUID, or an
//!   item with the same vanilla index, differs), when the earlier version is not in the history,
//!   or when the baseline changed too, it is left as it is, to be written again.
//!
//! Either way, a [`SourceUpdate`] at `source_updates/{resource}/{stream path}/{sha256}.json`,
//! named after the version the overwrite was written against, records the overwrite as it was,
//! the differences it left out, both versions' files and what was done. These records are a
//! history: rebuilding the cache keeps them, and a version already recorded is not looked at
//! again. Only source changes are handled; an overwrite whose baseline changed is listed in
//! `warnings.json` as before.

use super::{Result, normalize, resource_name};
use crate::common::json::{read_json, write_json};
use crate::core::format::ymap::compare::{YmapComparisonDiff, YmapComparisonFile, YmapDiffFields};
use crate::core::overwrite::{OverwriteFingerprint, Overwrites, StreamFileOverwrite, skip_diffs};
use crate::core::source_cache::{MergeInputs, history};
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

/// The directory of the [`SourceUpdate`] records below the overwrite cache.
pub const SOURCE_UPDATES: &str = "source_updates";

/// A resource overwrite whose source changed, and what was done about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct SourceUpdate {
  pub schema_version: u32,
  /// The resource name, in lowercase, as the overwrite is keyed.
  pub resource: String,
  /// The stream path, in lowercase, as the overwrite is keyed.
  pub stream_path: String,
  /// The version the overwrite was written against.
  pub before: SourceVersion,
  /// The version the resource ships now.
  pub after: SourceVersion,
  /// The overwrite as it was.
  pub overwrite: StreamFileOverwrite,
  /// The differences of [`Self::before`]'s diff document the overwrite left out.
  pub reverted: YmapDiffFields,
  pub outcome: SourceUpdateOutcome,
  /// Why the overwrite was not rewritten.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub reasons: Vec<String>,
  /// The overwrite as it was rewritten in the overwrites.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub rewritten: Option<StreamFileOverwrite>,
}

impl SourceUpdate {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;
}

/// One version of a source stream file, by the files the source cache holds for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct SourceVersion {
  pub sha256: String,
  /// The stream file, relative to the source cache; absent when it holds no such version.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub source: Option<String>,
  /// Its diff document, relative to the source cache; absent when it holds none.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub diff: Option<String>,
}

/// What was done about an overwrite whose source changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum SourceUpdateOutcome {
  /// It was rewritten against the current version.
  Rewritten,
  /// It is left as it is, not applied, to be written again.
  Manual,
}

/// Records and, where it can, rewrites each resource overwrite in `overwrites` written against
/// another version of its source than `inputs` hold; see the module docs. Returns whether it
/// rewrote any.
pub(super) fn review(
  output: &Path,
  source_cache_dir: &Path,
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> Result<bool> {
  let sources = inputs
    .ymap
    .iter()
    .map(|source| {
      (
        (resource_name(&source.resource).to_ascii_lowercase(), normalize(&source.stream_path)),
        source,
      )
    })
    .collect::<std::collections::BTreeMap<_, _>>();
  let mut rewrote = false;
  for (resource, stream_path) in overwrites.keys() {
    let Some(overwrite) = overwrites.stream_file(resource, stream_path) else {
      continue;
    };
    let (Some(written), false) = (&overwrite.source, overwrite.ignore) else {
      continue;
    };
    let Some(source) = sources.get(&(resource.to_string(), stream_path.to_string())) else {
      continue;
    };
    let current = OverwriteFingerprint::read(&source.path)?;
    if *written == current {
      continue;
    }
    let record = output
      .join(SOURCE_UPDATES)
      .join(resource)
      .join(format!("{stream_path}/{}.json", written.sha256));
    if fs::is_file(&record) {
      continue;
    }
    let name = resource_name(&source.resource);
    let stored =
      |relative: String| fs::is_file(source_cache_dir.join(&relative)).then_some(relative);
    let before = SourceVersion {
      sha256: written.sha256.clone(),
      source: stored(history::version_path(&name, &source.stream_path, &written.sha256)),
      diff: stored(history::diff_path(&name, &source.stream_path, &written.sha256)),
    };
    let after = SourceVersion {
      sha256: current.sha256.clone(),
      source: Some(source.stored_path.clone()),
      diff: source
        .diff_path
        .as_ref()
        .and_then(|path| path.strip_prefix(source_cache_dir).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/")),
    };
    let mut update = SourceUpdate {
      schema_version: SourceUpdate::SCHEMA_VERSION,
      resource: resource.to_string(),
      stream_path: stream_path.to_string(),
      before,
      after,
      overwrite: overwrite.clone(),
      reverted: YmapDiffFields::default(),
      outcome: SourceUpdateOutcome::Manual,
      reasons: Vec::new(),
      rewritten: None,
    };
    let documents = (
      update
        .before
        .diff
        .as_ref()
        .map(|path| read_json::<YmapComparisonDiff>(source_cache_dir.join(path))),
      source.diff_path.as_ref().map(read_json::<YmapComparisonDiff>),
    );
    match documents {
      (Some(before), Some(after)) => {
        let (before, after) = (before?, after?);
        let skip = overwrite.skip.as_ref().ok_or("Overwrite without skip")?;
        let rebased = rebase(&before, &after, overwrite, skip)?;
        update.reverted = serde_json::from_value(rebased.reverted)?;
        update.reasons = rebased.reasons;
        if update.reasons.is_empty() {
          let rewritten = StreamFileOverwrite {
            source: Some(current.clone()),
            skip: Some(rebased.skip),
            ..overwrite.clone()
          };
          let path = overwrites
            .stream_file_path(resource, stream_path)
            .ok_or("Overwrite was not read from a file")?;
          write_json(path, &rewritten)?;
          tracing::info!(
            "Rewrote the overwrite of {stream_path} ({resource}) against its source's current version"
          );
          update.outcome = SourceUpdateOutcome::Rewritten;
          update.rewritten = Some(rewritten);
          rewrote = true;
        }
      }
      (None, _) => update
        .reasons
        .push("the version it was written against is not in the source cache's history".into()),
      (_, None) => update.reasons.push("the current version has no diff document".into()),
    }
    if update.outcome == SourceUpdateOutcome::Manual {
      tracing::warn!(
        "Overwrite of {stream_path} ({resource}) was written against another version of its source and needs writing again: {}",
        update.reasons.join("; ")
      );
    }
    fs::create_dir_all(record.parent().ok_or("Record path has no parent")?)?;
    write_json(&record, &update)?;
  }
  Ok(rewrote)
}

/// The overwrite's skip carried from `before` to `after`, the diff documents of two versions of
/// its source.
struct Rebased {
  /// The differences of `before` it left out.
  reverted: Value,
  /// Its selectors without those that match nothing in `after`.
  skip: Value,
  /// Why it cannot be carried over; empty when it can.
  reasons: Vec<String>,
}

fn rebase(
  before: &YmapComparisonDiff,
  after: &YmapComparisonDiff,
  overwrite: &StreamFileOverwrite,
  skip: &Value,
) -> Result<Rebased> {
  let fingerprint = |file: &YmapComparisonFile| OverwriteFingerprint {
    sha256: file.sha256.clone(),
    size: file.size,
  };
  let left_out = |document: &YmapComparisonDiff| -> Result<(Value, Value, Vec<String>)> {
    let mut kept = document.diffs.clone();
    let unmatched = skip_diffs(&mut kept, skip)?;
    let full = serde_json::to_value(&document.diffs)?;
    let reverted =
      subtract(&full, &serde_json::to_value(&kept)?).unwrap_or(Value::Object(Map::new()));
    Ok((full, reverted, unmatched))
  };
  let (_, reverted, _) = left_out(before)?;
  let mut reasons = Vec::new();
  if overwrite.vanilla.as_ref() != Some(&fingerprint(&before.vanilla))
    || fingerprint(&before.vanilla) != fingerprint(&after.vanilla)
  {
    reasons.push("its baseline changed too".into());
    return Ok(Rebased {
      reverted,
      skip: skip.clone(),
      reasons,
    });
  }
  let (full, now_reverted, unmatched) = left_out(after)?;
  let (earlier, now) = (entries(&reverted), entries(&now_reverted));
  for (path, change) in &now {
    if !earlier.contains(&(path.clone(), change.clone())) {
      reasons.push(format!("it now leaves out {path}: {}, which it did not", describe(change)));
    }
  }
  let current = entries(&full);
  for (path, change) in &earlier {
    if now.contains(&(path.clone(), change.clone())) {
      continue;
    }
    if current.iter().any(|(other, candidate)| other == path && same_item(change, candidate)) {
      reasons.push(format!("{path}: {} it left out differs now", describe(change)));
    }
  }
  let mut skip = skip.clone();
  if reasons.is_empty() {
    prune(&mut skip, &unmatched);
  }
  Ok(Rebased {
    reverted,
    skip,
    reasons,
  })
}

/// What `full` has that `kept`, `full` with some of it left out, does not: the items of arrays
/// that `kept` lacks, and of an item whose changes it partly keeps, the changes it lacks.
fn subtract(
  full: &Value,
  kept: &Value,
) -> Option<Value> {
  match (full, kept) {
    (Value::Object(full), kept @ (Value::Object(_) | Value::Null)) => {
      let fields = full
        .iter()
        .filter_map(|(name, value)| {
          Some((name.clone(), subtract(value, kept.get(name).unwrap_or(&Value::Null))?))
        })
        .collect::<Map<_, _>>();
      (!fields.is_empty()).then_some(Value::Object(fields))
    }
    (Value::Array(full), kept @ (Value::Array(_) | Value::Null)) => {
      let kept = kept.as_array().map(Vec::as_slice).unwrap_or_default();
      let mut next = 0;
      let mut left_out = Vec::new();
      for item in full {
        match kept.get(next) {
          Some(candidate) if candidate == item => next += 1,
          Some(candidate) if same_variant(item, candidate) => {
            next += 1;
            left_out.extend(subtract_changes(item, candidate));
          }
          _ => left_out.push(item.clone()),
        }
      }
      (!left_out.is_empty()).then_some(Value::Array(left_out))
    }
    (full, kept) if full == kept => None,
    (full, _) => Some(full.clone()),
  }
}

/// The variant and payload of an item diff, `{ "Modified": { ... } }`.
fn variant(item: &Value) -> Option<(&String, &Map<String, Value>)> {
  match item.as_object()?.iter().collect::<Vec<_>>().as_slice() {
    [(name, Value::Object(payload))] => Some((name, payload)),
    _ => None,
  }
}

/// Whether `item` and `other` are the same item diff but for the changes they make.
fn same_variant(
  item: &Value,
  other: &Value,
) -> bool {
  let (Some((name, payload)), Some((other_name, other_payload))) = (variant(item), variant(other))
  else {
    return false;
  };
  let without = |payload: &Map<String, Value>| {
    payload
      .iter()
      .filter(|(key, _)| *key != "changes")
      .map(|(key, value)| (key.clone(), value.clone()))
      .collect::<Map<_, _>>()
  };
  name == other_name && without(payload) == without(other_payload)
}

/// The changes of `full` that `kept`, the same item diff, does not make, as that item diff.
fn subtract_changes(
  full: &Value,
  kept: &Value,
) -> Option<Value> {
  let ((name, payload), (_, kept_payload)) = (variant(full)?, variant(kept)?);
  let changes = subtract(
    payload.get("changes").unwrap_or(&Value::Null),
    kept_payload.get("changes").unwrap_or(&Value::Null),
  )?;
  let mut payload = payload.clone();
  payload.insert("changes".into(), changes);
  Some(Value::Object(Map::from_iter([(name.clone(), Value::Object(payload))])))
}

/// Each array item and other value of `diffs`, by the path of the field holding it.
fn entries(diffs: &Value) -> Vec<(String, Value)> {
  fn walk(
    value: &Value,
    path: &str,
    entries: &mut Vec<(String, Value)>,
  ) {
    match value {
      Value::Object(fields) => {
        for (name, value) in fields {
          let path = if path.is_empty() { name.clone() } else { format!("{path}.{name}") };
          walk(value, &path, entries);
        }
      }
      Value::Array(items) => {
        entries.extend(items.iter().map(|item| (path.to_string(), item.clone())))
      }
      value => entries.push((path.to_string(), value.clone())),
    }
  }
  let mut entries = Vec::new();
  walk(diffs, "", &mut entries);
  entries
}

/// A short description of `change`, naming the item it changes.
fn describe(change: &Value) -> String {
  let Some((name, payload)) = variant(change) else {
    return format!("the value {change}");
  };
  let guid = payload.get("entity").and_then(|entity| entity.get("guid"));
  let index = payload.get("before_index").or_else(|| payload.get("index"));
  match (name.as_str(), guid, index) {
    ("Added", Some(guid), _) => format!("the addition of entity {guid}"),
    ("Added", ..) => "an addition".into(),
    (name, _, index) => {
      let change = match name {
        "Removed" => "the removal",
        "Replaced" => "the replacement",
        _ => "the modification",
      };
      match index {
        Some(index) => format!("{change} of vanilla item {index}"),
        None => change.into(),
      }
    }
  }
}

/// Whether `change` and `other`, at the same path, change the same item: entities added with
/// the same non-zero GUID, or items changing the same vanilla index, or the same field's value.
fn same_item(
  change: &Value,
  other: &Value,
) -> bool {
  let (Some((name, payload)), Some((other_name, other_payload))) =
    (variant(change), variant(other))
  else {
    // A field's value, rather than an array item.
    return !change.is_object() && !change.is_array();
  };
  let guid = |payload: &Map<String, Value>| {
    payload
      .get("entity")
      .and_then(|entity| entity.get("guid"))
      .and_then(Value::as_u64)
      .filter(|guid| *guid != 0)
  };
  let index = |payload: &Map<String, Value>| {
    payload.get("before_index").or_else(|| payload.get("index")).and_then(Value::as_u64)
  };
  match (name.as_str(), other_name.as_str()) {
    ("Added", "Added") => guid(payload).is_some() && guid(payload) == guid(other_payload),
    ("Added", _) | (_, "Added") => false,
    _ => index(payload).is_some() && index(payload) == index(other_payload),
  }
}

/// Removes the selectors at `paths`, as [`skip_diffs`] names those that matched nothing, from
/// `skip`, and then the array selectors whose `changes` that leaves empty, which would otherwise
/// select the whole item.
fn prune(
  skip: &mut Value,
  paths: &[String],
) {
  #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
  enum Step {
    Field(String),
    Item(usize),
  }
  let parse = |path: &str| {
    let mut steps = Vec::new();
    for part in path.split('.') {
      let (name, rest) = part.split_once('[').unwrap_or((part, ""));
      steps.push(Step::Field(name.to_string()));
      for index in rest.split('[').filter(|index| !index.is_empty()) {
        if let Ok(index) = index.trim_end_matches(']').parse() {
          steps.push(Step::Item(index));
        }
      }
    }
    steps
  };
  let mut paths = paths.iter().map(|path| parse(path)).collect::<Vec<_>>();
  // Later items first, so earlier indices stay valid.
  paths.sort();
  paths.reverse();
  fn remove(
    value: &mut Value,
    steps: &[Step],
  ) {
    match (steps, value) {
      ([Step::Field(name)], Value::Object(fields)) => {
        fields.remove(name);
      }
      ([Step::Item(index)], Value::Array(items)) if *index < items.len() => {
        items.remove(*index);
      }
      ([Step::Field(name), rest @ ..], Value::Object(fields)) if !rest.is_empty() => {
        if let Some(value) = fields.get_mut(name) {
          remove(value, rest);
        }
      }
      ([Step::Item(index), rest @ ..], Value::Array(items)) if !rest.is_empty() => {
        if let Some(value) = items.get_mut(*index) {
          remove(value, rest);
        }
      }
      _ => {}
    }
  }
  for steps in paths {
    remove(skip, &steps);
  }
  // Whether nothing is left of `value`. A selector's `changes` is kept even when empty, for
  // its array to drop the whole selector.
  fn clean(value: &mut Value) -> bool {
    match value {
      Value::Object(fields) => {
        fields.retain(|name, value| !clean(value) || name == "changes");
        fields.is_empty()
      }
      Value::Array(items) => {
        for item in items.iter_mut() {
          clean(item);
        }
        items.retain(|item| {
          let emptied = variant(item).is_some_and(|(_, payload)| {
            payload
              .get("changes")
              .is_some_and(|changes| changes.as_object().is_some_and(Map::is_empty))
          });
          !emptied && item.as_object().is_none_or(|fields| !fields.is_empty())
        });
        items.is_empty()
      }
      _ => false,
    }
  }
  if clean(skip) {
    *skip = Value::Object(Map::new());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn what_a_skip_leaves_out_includes_the_changes_of_partly_kept_items() {
    let modified = |changes: Value| json!({"Modified": {"before_index": 0, "changes": changes}});
    let full = json!({
      "content_flags": 4,
      "entities": [modified(json!({"position": {"x": 1.0}, "flags": 2})), {"Added": {"entity": {"guid": 5}}}]
    });
    let kept = json!({"entities": [modified(json!({"flags": 2}))]});
    assert_eq!(
      subtract(&full, &kept),
      Some(json!({
        "content_flags": 4,
        "entities": [modified(json!({"position": {"x": 1.0}})), {"Added": {"entity": {"guid": 5}}}]
      }))
    );
    assert_eq!(subtract(&full, &full), None);
  }

  #[test]
  fn pruning_drops_selectors_left_without_changes_rather_than_widening_them() {
    let mut skip = json!({
      "content_flags": true,
      "entities": [
        {"Modified": {"before_index": 0, "changes": {"position": true}}},
        {"Added": {"entity": {"guid": 10}}},
        {"Added": {"entity": {"guid": 11}}},
        {"Modified": {"before_index": 1, "changes": {"position": true, "flags": true}}}
      ]
    });
    prune(
      &mut skip,
      &[
        "content_flags".into(),
        "entities[0].Modified.changes.position".into(),
        "entities[2]".into(),
        "entities[3].Modified.changes.flags".into(),
      ],
    );
    assert_eq!(
      skip,
      json!({"entities": [
        {"Added": {"entity": {"guid": 10}}},
        {"Modified": {"before_index": 1, "changes": {"position": true}}}
      ]})
    );
    let mut everything = json!({"content_flags": true});
    prune(&mut everything, &["content_flags".into()]);
    assert_eq!(everything, json!({}));
  }

  #[test]
  fn changes_to_the_same_item_are_told_apart_from_others() {
    let added = |guid: u32| json!({"Added": {"entity": {"guid": guid}}});
    assert!(same_item(&added(5), &added(5)));
    assert!(!same_item(&added(0), &added(0)));
    assert!(!same_item(&added(5), &added(6)));
    let removed = json!({"Removed": {"index": 3}});
    assert!(same_item(&removed, &json!({"Modified": {"before_index": 3, "changes": {}}})));
    assert!(!same_item(&removed, &json!({"Removed": {"index": 4}})));
    assert!(same_item(&json!(4), &json!(8)));
  }
}
