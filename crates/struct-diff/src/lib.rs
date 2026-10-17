//! Typed, recursive differences between values of one struct.
//!
//! `#[derive(Diff)]` on a struct with named fields generates a `<Name>Diff` struct holding an
//! `Option` per field, `None` when the field is unchanged, and implements [`Diff`] to take and
//! apply such differences:
//!
//! ```
//! use struct_diff::Diff;
//!
//! #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
//! struct Position {
//!   x: i32,
//!   y: i32,
//! }
//!
//! #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Diff)]
//! struct Entity {
//!   #[diff(nested)]
//!   position: Position,
//!   name: String,
//! }
//!
//! let before = Entity { position: Position { x: 0, y: 0 }, name: "a".into() };
//! let mut after = before.clone();
//! after.position.x = 10;
//!
//! let diff = before.diff(&after, &()).unwrap();
//! assert_eq!(serde_json::to_value(&diff).unwrap(), serde_json::json!({ "position": { "x": 10 } }));
//! assert_eq!(before.diff(&before, &()), None);
//!
//! let mut applied = before.clone();
//! applied.apply(&diff).unwrap();
//! assert_eq!(applied, after);
//! ```
//!
//! # Struct attributes
//!
//! - `#[diff(name = Ident)]` names the generated struct instead of `<Name>Diff`.
//! - `#[diff(context = Type)]` sets [`Diff::Context`], `()` by default. Field attribute
//!   expressions see it as `ctx: &Type`.
//! - `#[diff(attr(...))]` adds `#[...]` to the generated struct, and
//!   `#[diff(field_attr(...))]` adds `#[...]` to each of its fields.
//!
//! The generated struct always derives `Debug`, `Clone`, `Default`, `PartialEq`,
//! `serde::Serialize` and `serde::Deserialize`, omits `None` fields when serialized, and keeps
//! the visibility of the struct and of each field. A container `#[serde(rename_all = ...)]` and
//! field `#[serde(rename = ...)]` carry over.
//!
//! # Field attributes
//!
//! Each field is compared by a strategy module providing `diff(before, after, ctx)` and
//! `apply(target, change)`, as `serde(with = ...)` does for serialization:
//!
//! - no attribute: [`replace`], `PartialEq`, storing the new value.
//! - `#[diff(nested)]`: [`nested`], the field's own [`Diff`], storing its `Output`.
//! - `#[diff(threshold = expr)]`: [`threshold`], ignoring changes smaller than `expr`.
//! - `#[diff(with = path)]`: `path::diff` and `path::apply`; their change type is the field type
//!   unless `output = Type` is given.
//! - `#[diff(skip)]`: the field is left out.
//!
//! `nested` and `with` receive `ctx` unless `context = expr` gives the value they receive
//! instead, and `#[diff(attr(...))]` adds `#[...]` to the generated field.

pub use struct_diff_derive::Diff;

use std::fmt;

/// A value whose differences from another value of its type can be taken and applied.
pub trait Diff {
  /// Settings that the comparison depends on, such as tolerances.
  type Context: ?Sized;
  /// The changes between two values.
  type Output;

  /// The changes from `self` to `other`, or `None` when there are none.
  fn diff(
    &self,
    other: &Self,
    ctx: &Self::Context,
  ) -> Option<Self::Output>;

  /// Applies changes taken by [`Self::diff`] from a value equal to `self`.
  fn apply(
    &mut self,
    diff: &Self::Output,
  ) -> Result<(), ApplyError>;
}

/// Changes that do not fit the value they are applied to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyError {
  path: Vec<&'static str>,
  message: String,
}

impl ApplyError {
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      path: Vec::new(),
      message: message.into(),
    }
  }

  /// The error as raised within `field` of the enclosing value.
  pub fn at(
    mut self,
    field: &'static str,
  ) -> Self {
    self.path.insert(0, field);
    self
  }
}

impl fmt::Display for ApplyError {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    if !self.path.is_empty() {
      write!(f, "{}: ", self.path.join("."))?;
    }
    f.write_str(&self.message)
  }
}

impl std::error::Error for ApplyError {}

/// Compares with `PartialEq`, recording the new value; the default strategy.
pub mod replace {
  use super::ApplyError;

  pub fn diff<T: PartialEq + Clone, C: ?Sized>(
    before: &T,
    after: &T,
    _ctx: &C,
  ) -> Option<T> {
    (before != after).then(|| after.clone())
  }

  pub fn apply<T: Clone>(
    target: &mut T,
    value: &T,
  ) -> Result<(), ApplyError> {
    target.clone_from(value);
    Ok(())
  }
}

/// Compares with the field type's own [`Diff`](super::Diff).
pub mod nested {
  use super::{ApplyError, Diff};

  pub fn diff<T: Diff>(
    before: &T,
    after: &T,
    ctx: &T::Context,
  ) -> Option<T::Output> {
    before.diff(after, ctx)
  }

  pub fn apply<T: Diff>(
    target: &mut T,
    diff: &T::Output,
  ) -> Result<(), ApplyError> {
    target.apply(diff)
  }
}

/// Compares numbers, recording the new value when it differs by at least the tolerance.
///
/// A tolerance of zero compares exactly.
pub mod threshold {
  /// Numbers whose differences can be measured against a tolerance.
  pub trait Threshold: PartialEq + Clone {
    /// Whether `self` and `other` differ by at least `tolerance`.
    fn differs_by(
      &self,
      other: &Self,
      tolerance: &Self,
    ) -> bool;
  }

  macro_rules! impl_float {
    ($($ty:ty),*) => {$(
      impl Threshold for $ty {
        fn differs_by(&self, other: &Self, tolerance: &Self) -> bool {
          self != other && (self - other).abs() >= *tolerance
        }
      }
    )*};
  }
  impl_float!(f32, f64);

  macro_rules! impl_integer {
    ($($ty:ty),*) => {$(
      impl Threshold for $ty {
        fn differs_by(&self, other: &Self, tolerance: &Self) -> bool {
          self != other && self.abs_diff(*other) >= tolerance.unsigned_abs()
        }
      }
    )*};
  }
  impl_integer!(i8, i16, i32, i64, i128, isize);

  macro_rules! impl_unsigned {
    ($($ty:ty),*) => {$(
      impl Threshold for $ty {
        fn differs_by(&self, other: &Self, tolerance: &Self) -> bool {
          self != other && self.abs_diff(*other) >= *tolerance
        }
      }
    )*};
  }
  impl_unsigned!(u8, u16, u32, u64, u128, usize);

  pub fn diff<T: Threshold>(
    before: &T,
    after: &T,
    tolerance: &T,
  ) -> Option<T> {
    before.differs_by(after, tolerance).then(|| after.clone())
  }

  pub use super::replace::apply;
}

#[doc(hidden)]
pub mod __private {
  pub use serde;
}
