//! Leaves selected differences out of a diff document.

use crate::common::Result;
use crate::core::format::ymap::compare::YmapDiffFields;
use serde_json::{Map, Value};

/// Leaves the differences `skip` selects out of `diff` (see
/// [`super::StreamFileOverwrite::skip`]), dropping objects and arrays left empty. Returns the
/// paths of the selectors that matched nothing.
pub fn skip_diffs(
  diff: &mut YmapDiffFields,
  skip: &Value,
) -> Result<Vec<String>> {
  let mut value = serde_json::to_value(&*diff)?;
  let mut unmatched = Vec::new();
  if skip_value(&mut value, skip, "", &mut unmatched)? {
    value = Value::Object(Map::new());
  }
  *diff = serde_json::from_value(value)?;
  Ok(unmatched)
}

/// Leaves what `skip` selects out of `target`; returns whether nothing of it is left.
fn skip_value(
  target: &mut Value,
  skip: &Value,
  path: &str,
  unmatched: &mut Vec<String>,
) -> Result<bool> {
  match (skip, target) {
    (Value::Bool(true), _) => Ok(true),
    (Value::Object(fields), Value::Object(target)) => {
      for (name, skip) in fields {
        let path = join(path, name);
        let Some(field) = target.get_mut(name) else {
          unmatched.push(path);
          continue;
        };
        if skip_value(field, skip, &path, unmatched)? {
          target.remove(name);
        }
      }
      Ok(target.is_empty())
    }
    (Value::Array(selectors), Value::Array(items)) => {
      for (index, selector) in selectors.iter().enumerate() {
        let path = format!("{path}[{index}]");
        let mut matched = false;
        let mut kept = Vec::with_capacity(items.len());
        for mut item in std::mem::take(items) {
          match skip_item(&mut item, selector, &path, unmatched)? {
            None => kept.push(item),
            Some(emptied) => {
              matched = true;
              if !emptied {
                kept.push(item);
              }
            }
          }
        }
        *items = kept;
        if !matched {
          unmatched.push(path);
        }
      }
      Ok(items.is_empty())
    }
    (Value::Object(_) | Value::Array(_), _) => {
      unmatched.push(path.to_string());
      Ok(false)
    }
    (skip, _) => Err(format!("{path}: expected true, an object or an array, found {skip}").into()),
  }
}

/// Applies an array selector to `item`: `None` when it does not match, otherwise whether
/// nothing of the item is left.
fn skip_item(
  item: &mut Value,
  selector: &Value,
  path: &str,
  unmatched: &mut Vec<String>,
) -> Result<Option<bool>> {
  // An item diff is one variant, `{ "Modified": { ... } }`; its `changes` select within it.
  if let (Value::Object(selector), Value::Object(variants)) = (selector, &mut *item)
    && selector.len() == 1
    && let Some((variant, Value::Object(fields))) = selector.iter().next()
    && let Some(Value::Object(payload)) = variants.get_mut(variant)
  {
    let matches = fields
      .iter()
      .filter(|(name, _)| *name != "changes")
      .all(|(name, expected)| payload.get(name).is_some_and(|actual| matches(actual, expected)));
    if !matches {
      return Ok(None);
    }
    let Some(changes) = fields.get("changes") else {
      return Ok(Some(true));
    };
    let path = join(&join(path, variant), "changes");
    let Some(target) = payload.get_mut("changes") else {
      unmatched.push(path);
      return Ok(Some(false));
    };
    return Ok(Some(skip_value(target, changes, &path, unmatched)?));
  }
  Ok(matches(item, selector).then_some(true))
}

/// Whether `actual` has every field `expected` writes, with equal values.
fn matches(
  actual: &Value,
  expected: &Value,
) -> bool {
  match (actual, expected) {
    (Value::Object(actual), Value::Object(expected)) => expected
      .iter()
      .all(|(name, expected)| actual.get(name).is_some_and(|actual| matches(actual, expected))),
    (Value::Array(actual), Value::Array(expected)) => {
      actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| matches(actual, expected))
    }
    (Value::Number(actual), Value::Number(expected)) => {
      actual == expected
        || actual.as_f64().zip(expected.as_f64()).is_some_and(|(actual, expected)| {
          // Documents spell single-precision values; compare them at that precision.
          actual as f32 == expected as f32
        })
    }
    (actual, expected) => actual == expected,
  }
}

fn join(
  path: &str,
  name: &str,
) -> String {
  if path.is_empty() { name.to_string() } else { format!("{path}.{name}") }
}
