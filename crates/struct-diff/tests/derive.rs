use serde_json::json;
use struct_diff::{ApplyError, Diff};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
struct Position {
  x: i32,
  y: i32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
struct Entity {
  #[diff(nested)]
  position: Position,
  name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
struct World {
  #[diff(nested)]
  entity: Entity,
  #[diff(skip)]
  #[allow(dead_code)]
  cache: Vec<u8>,
}

fn entity(
  x: i32,
  y: i32,
  name: &str,
) -> Entity {
  Entity {
    position: Position {
      x,
      y,
    },
    name: name.into(),
  }
}

/// Asserts that applying the diff of `after` to `before` rebuilds `after`.
fn assert_round_trip<T: Diff + Clone + PartialEq + std::fmt::Debug>(
  before: &T,
  after: &T,
  ctx: &T::Context,
) {
  let mut applied = before.clone();
  applied.apply(&before.diff(after, ctx).unwrap()).unwrap();
  assert_eq!(&applied, after);
}

#[test]
fn equal_values_have_no_diff() {
  assert_eq!(entity(1, 2, "a").diff(&entity(1, 2, "a"), &()), None);
}

#[test]
fn only_changed_fields_are_set() {
  let diff = entity(1, 2, "a").diff(&entity(1, 2, "b"), &()).unwrap();
  assert_eq!(
    diff,
    EntityDiff {
      position: None,
      name: Some("b".into()),
    }
  );
  let diff = entity(1, 2, "a").diff(&entity(5, 2, "b"), &()).unwrap();
  assert_eq!(diff.name.as_deref(), Some("b"));
  assert_eq!(
    diff.position,
    Some(PositionDiff {
      x: Some(5),
      y: None,
    })
  );
  assert_round_trip(&entity(1, 2, "a"), &entity(5, 2, "b"), &());
}

#[test]
fn nested_diffs_omit_unchanged_fields_at_every_level() {
  let before = World {
    entity: entity(1, 2, "a"),
    cache: vec![1],
  };
  let after = World {
    entity: entity(1, 7, "a"),
    cache: vec![2],
  };
  let diff = before.diff(&after, &()).unwrap();
  assert_eq!(serde_json::to_value(&diff).unwrap(), json!({ "entity": { "position": { "y": 7 } } }));
  let same_but_skipped = World {
    cache: vec![3],
    ..before.clone()
  };
  assert_eq!(before.diff(&same_but_skipped, &()), None);
}

#[test]
fn an_empty_diff_serializes_as_an_empty_object() {
  assert_eq!(serde_json::to_value(EntityDiff::default()).unwrap(), json!({}));
  let diff: EntityDiff = serde_json::from_value(json!({ "name": "b" })).unwrap();
  assert_eq!(diff.name.as_deref(), Some("b"));
  assert_eq!(diff.position, None);
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
#[diff(context = f32)]
struct Point {
  #[diff(threshold = *ctx)]
  x: f32,
  #[diff(threshold = *ctx)]
  y: f32,
}

struct Tolerances {
  speed: f32,
  point: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
#[diff(context = Tolerances)]
struct Vehicle {
  #[diff(threshold = ctx.speed)]
  speed: f32,
  #[diff(nested, context = ctx.point)]
  at: Point,
  #[diff(nested, context = 0.0)]
  exact: Point,
  #[diff(threshold = 2)]
  gear: u8,
}

const TOLERANCES: Tolerances = Tolerances {
  speed: 0.5,
  point: 1.0,
};

fn vehicle(speed: f32) -> Vehicle {
  Vehicle {
    speed,
    at: Point {
      x: 0.0,
      y: 0.0,
    },
    exact: Point {
      x: 0.0,
      y: 0.0,
    },
    gear: 1,
  }
}

#[test]
fn thresholds_come_from_the_context() {
  assert_eq!(vehicle(1.0).diff(&vehicle(1.4), &TOLERANCES), None);
  assert_eq!(vehicle(1.0).diff(&vehicle(1.5), &TOLERANCES).unwrap().speed, Some(1.5));

  let mut moved = vehicle(1.0);
  moved.at.x = 0.9;
  moved.exact.y = 0.001;
  moved.gear = 2;
  let diff = vehicle(1.0).diff(&moved, &TOLERANCES).unwrap();
  assert_eq!(serde_json::to_value(&diff).unwrap(), json!({ "exact": { "y": 0.001f32 } }));
  moved.at.y = -1.0;
  moved.gear = 3;
  let diff = vehicle(1.0).diff(&moved, &TOLERANCES).unwrap();
  assert_eq!(diff.at.unwrap().y, Some(-1.0));
  assert_eq!(diff.gear, Some(3));
}

/// Records the name only when its letters change, ignoring case.
mod case_insensitive {
  use struct_diff::ApplyError;

  pub fn diff<C: ?Sized>(
    before: &str,
    after: &str,
    _ctx: &C,
  ) -> Option<String> {
    (!before.eq_ignore_ascii_case(after)).then(|| after.to_owned())
  }

  pub fn apply(
    target: &mut String,
    value: &String,
  ) -> Result<(), ApplyError> {
    target.clone_from(value);
    Ok(())
  }
}

/// Elements appended to a list that only grows.
mod appended {
  use struct_diff::ApplyError;

  pub fn diff(
    before: &[u32],
    after: &[u32],
    limit: &usize,
  ) -> Option<Vec<u32>> {
    let added = after.strip_prefix(before)?;
    (!added.is_empty()).then(|| added.iter().take(*limit).copied().collect())
  }

  pub fn apply(
    target: &mut Vec<u32>,
    added: &Vec<u32>,
  ) -> Result<(), ApplyError> {
    if target.len() + added.len() > 4 {
      return Err(ApplyError::new("too many items"));
    }
    target.extend(added);
    Ok(())
  }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
struct Inventory {
  #[diff(with = case_insensitive)]
  owner: String,
  #[diff(with = appended, output = Vec<u32>, context = 10)]
  items: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
struct Shop {
  #[diff(nested, context = ())]
  inventory: Inventory,
}

#[test]
fn custom_strategies_choose_the_change_and_its_type() {
  let before = Inventory {
    owner: "Ann".into(),
    items: vec![1],
  };
  let after = Inventory {
    owner: "ANN".into(),
    items: vec![1, 2, 3],
  };
  let diff = before.diff(&after, &()).unwrap();
  assert_eq!(serde_json::to_value(&diff).unwrap(), json!({ "items": [2, 3] }));
  let mut applied = before.clone();
  applied.apply(&diff).unwrap();
  assert_eq!(applied.items, after.items);
}

#[test]
fn apply_errors_name_the_field_path() {
  let mut shop = Shop {
    inventory: Inventory {
      owner: "Ann".into(),
      items: vec![1, 2, 3],
    },
  };
  let diff = ShopDiff {
    inventory: Some(InventoryDiff {
      owner: None,
      items: Some(vec![4, 5]),
    }),
  };
  let error: ApplyError = shop.apply(&diff).unwrap_err();
  assert_eq!(error.to_string(), "inventory.items: too many items");
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
#[serde(rename_all = "camelCase")]
#[diff(name = Renamed, attr(derive(Eq, Hash)), field_attr(doc = "A changed field."))]
pub struct Settings {
  pub max_count: u32,
  #[serde(rename = "label")]
  #[diff(attr(serde(alias = "title")))]
  pub display_name: String,
}

#[test]
fn naming_and_extra_attributes_carry_over() {
  let before = Settings {
    max_count: 1,
    display_name: "a".into(),
  };
  let after = Settings {
    max_count: 2,
    display_name: "b".into(),
  };
  let diff: Renamed = before.diff(&after, &()).unwrap();
  assert_eq!(serde_json::to_value(&diff).unwrap(), json!({ "maxCount": 2, "label": "b" }));
  let aliased: Renamed = serde_json::from_value(json!({ "title": "c" })).unwrap();
  assert_eq!(aliased.display_name.as_deref(), Some("c"));
  std::collections::HashSet::<Renamed>::new().insert(diff);
}

/// A field type without `Clone`, compared by its own `Diff`.
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
struct Counter {
  value: u32,
}

#[derive(Debug, PartialEq, Diff)]
struct Holder {
  #[diff(nested)]
  counter: Counter,
}

#[test]
fn nested_fields_need_not_be_clone() {
  let before = Holder {
    counter: Counter {
      value: 1,
    },
  };
  let after = Holder {
    counter: Counter {
      value: 2,
    },
  };
  let diff = before.diff(&after, &()).unwrap();
  assert_eq!(diff.counter.unwrap().value, Some(2));
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
struct Pair<T> {
  first: T,
  second: T,
}

#[test]
fn generic_structs_diff_their_parameters() {
  let before = Pair {
    first: 1u8,
    second: 2u8,
  };
  let after = Pair {
    first: 1u8,
    second: 3u8,
  };
  let diff: PairDiff<u8> = before.diff(&after, &()).unwrap();
  assert_eq!(serde_json::to_value(&diff).unwrap(), json!({ "second": 3 }));
  assert_round_trip(&before, &after, &());
}

#[test]
fn compile_errors() {
  trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
