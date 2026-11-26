//! Leaves selected differences out of a diff document: the skip selectors overwrites and the
//! source cache's LOD light pair documents write.

use crate::common::Result;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

/// Leaves the differences `skip` selects out of `diff`, dropping objects and arrays left empty.
/// In an object, `true` leaves out the whole difference and a nested selector only the parts it
/// names; in an array, each selector leaves out the items it matches (see [`selects`]). Returns
/// the paths of the selectors that matched nothing.
pub fn skip_diffs<T: Serialize + DeserializeOwned>(
  diff: &mut T,
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
    if !payload_matches(payload, fields) {
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
  // A group of occlude model triangles or YBN polygons, `{ "flags": 0, "removed": [...],
  // "added": [...] }`: its other fields name it, and its arrays of objects are selected within.
  if let (Value::Object(selector), Value::Object(fields)) = (selector, &mut *item)
    && selector.keys().any(|name| fields.get(name).is_some_and(is_object_list))
  {
    let within = |name: &String| fields.get(name).is_some_and(is_object_list);
    let names_it = selector
      .iter()
      .filter(|(name, _)| !within(name))
      .all(|(name, expected)| fields.get(name).is_some_and(|actual| matches(actual, expected)));
    if !names_it {
      return Ok(None);
    }
    let arrays = selector.keys().filter(|name| within(name)).cloned().collect::<Vec<_>>();
    for name in arrays {
      let path = join(path, &name);
      let target = fields.get_mut(&name).expect("an array field");
      if skip_value(target, &selector[&name], &path, unmatched)? {
        fields.remove(&name);
      }
    }
    return Ok(Some(!fields.values().any(is_object_list)));
  }
  Ok(matches(item, selector).then_some(true))
}

/// Whether `value` is an array of objects, such as the triangles of a group, rather than of
/// numbers, such as its flags.
fn is_object_list(value: &Value) -> bool {
  value.as_array().is_some_and(|items| items.first().is_none_or(Value::is_object))
}

/// Whether array selector `selector` selects `item`, an item of a diff array, leaving aside the
/// `changes` it names within the item.
pub fn selects(
  item: &Value,
  selector: &Value,
) -> bool {
  if let (Value::Object(selector), Value::Object(variants)) = (selector, item)
    && selector.len() == 1
    && let Some((variant, Value::Object(fields))) = selector.iter().next()
    && let Some(Value::Object(payload)) = variants.get(variant)
  {
    return payload_matches(payload, fields);
  }
  matches(item, selector)
}

/// Whether an item diff's `payload` has every field a variant selector writes, but `changes`.
fn payload_matches(
  payload: &Map<String, Value>,
  fields: &Map<String, Value>,
) -> bool {
  fields
    .iter()
    .filter(|(name, _)| *name != "changes")
    .all(|(name, expected)| payload.get(name).is_some_and(|actual| matches(actual, expected)))
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

/// The TypeScript type of a skip selector, written out by [`crate::typescript::export_types`] as
/// [`YMAP_DIFF_SKIP_TS`].
#[cfg(feature = "typescript")]
pub(crate) struct YmapDiffSkip;

#[cfg(feature = "typescript")]
impl specta::Type for YmapDiffSkip {
  fn inline(
    _: specta::DefOpts,
    _: &[specta::DataType],
  ) -> std::result::Result<specta::DataType, specta::ExportError> {
    Ok(specta::DataType::Reference(specta::DataTypeReference {
      name: "YmapDiffSkip",
      sid: specta::sid!(@with_specta_path; "YmapDiffSkip"; specta),
      generics: Vec::new(),
    }))
  }
}

/// The TypeScript definitions of [`YmapDiffSkip`], which Specta cannot derive.
#[cfg(feature = "typescript")]
pub(crate) const YMAP_DIFF_SKIP_TS: &str = r#"
/** The differences of a `YmapDiffFields` or `YbnDiffFields` document to leave out of a merge. */
export type YmapDiffSkip = DiffSkip<YmapDiffFields> | DiffSkip<YbnDiffFields>

/**
 * In an object, `true` leaves out the whole difference and a nested selector only the parts it
 * names; in an array, each selector leaves out the items it matches.
 */
export type DiffSkip<T> =
  | true
  | (T extends (infer I)[] ? ItemSkip<I>[] : T extends object ? { [K in keyof T]?: DiffSkip<NonNullable<T[K]>> } : never)

/**
 * Matches an array item: every field written must equal the item's, except `changes`, which
 * leaves out only the changes it names instead of the whole item.
 */
export type ItemSkip<I> = I extends object
  ? {
      [V in keyof I]?: I[V] extends { changes: infer C }
        ? DiffMatch<Omit<I[V], "changes">> & { changes?: DiffSkip<NonNullable<C>> }
        : DiffMatch<I[V]>
    }
  : I

/** A value an item's field must equal; objects match on the fields written. */
export type DiffMatch<T> = T extends (infer I)[] ? I[] : T extends object ? { [K in keyof T]?: DiffMatch<T[K]> } : T
"#;
