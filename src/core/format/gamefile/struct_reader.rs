//! Schema-driven, typed field access shared by META and PSO structures.
//!
//! Model decoders are written once against [`StructReader`] and work for any container whose
//! embedded schemas name their fields.

use std::{fmt, io};

/// A primitive field value widened to a common representation.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
  Bool(bool),
  Int(i64),
  Float(f32),
  /// Strings, resolved hashes, enum names and named flag lists.
  Text(String),
}

impl fmt::Display for Scalar {
  fn fmt(
    &self,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    match self {
      Self::Bool(value) => value.fmt(formatter),
      Self::Int(value) => value.fmt(formatter),
      Self::Float(value) => value.fmt(formatter),
      Self::Text(value) => value.fmt(formatter),
    }
  }
}

/// Converts a [`Scalar`] into a model field type.
///
/// Integers accept any integer field kind within range, and floats with no fractional part.
/// Floats accept integer kinds too.
pub trait FromScalar: Sized {
  fn from_scalar(value: Scalar) -> io::Result<Self>;
}

impl FromScalar for Scalar {
  fn from_scalar(value: Scalar) -> io::Result<Self> {
    Ok(value)
  }
}

macro_rules! integer_from_scalar {
  ($($type:ty),*) => {$(
    impl FromScalar for $type {
      fn from_scalar(value: Scalar) -> io::Result<Self> {
        let integer = match value {
          Scalar::Int(value) => value,
          Scalar::Float(value) if value.is_finite() && value.fract() == 0.0 => value as i64,
          other => return Err(invalid(&format!("value {other:?} is not an integer"))),
        };
        Self::try_from(integer).map_err(|_| {
          invalid(&format!("integer {integer} is out of range for {}", stringify!($type)))
        })
      }
    }
  )*};
}

integer_from_scalar!(u8, u16, u32, u64, i8, i16, i32, i64);

impl FromScalar for f32 {
  fn from_scalar(value: Scalar) -> io::Result<Self> {
    match value {
      Scalar::Float(value) => Ok(value),
      Scalar::Int(value) => Ok(value as f32),
      other => Err(invalid(&format!("value {other:?} is not a number"))),
    }
  }
}

impl FromScalar for bool {
  fn from_scalar(value: Scalar) -> io::Result<Self> {
    match value {
      Scalar::Bool(value) => Ok(value),
      other => Err(invalid(&format!("value {other:?} is not a bool"))),
    }
  }
}

impl FromScalar for String {
  fn from_scalar(value: Scalar) -> io::Result<Self> {
    match value {
      Scalar::Text(value) => Ok(value),
      other => Err(invalid(&format!("value {other:?} is not text"))),
    }
  }
}

/// One structure instance read through its embedded schema.
///
/// Fields are looked up by name, or by `hash_XXXXXXXX` for unnamed hashes, so callers stay
/// independent of per-file layouts.
pub trait StructReader: Sized {
  /// The resolved structure name, e.g. `CEntityDef`.
  fn type_name(&self) -> String;

  /// Reads a scalar field, or `None` when the schema has no such field.
  fn scalar(
    &self,
    name: &str,
  ) -> io::Result<Option<Scalar>>;

  /// Reads a float vector field, or `None` when it is absent.
  fn vector(
    &self,
    name: &str,
  ) -> io::Result<Option<Vec<f32>>>;

  /// Reads a nested structure field; `None` when absent or a null pointer.
  fn child(
    &self,
    name: &str,
  ) -> io::Result<Option<Self>>;

  /// Reads a structure array field; absent fields read as empty.
  fn children(
    &self,
    name: &str,
  ) -> io::Result<Vec<Self>>;

  /// Reads a primitive, hash or byte array field; absent fields read as empty.
  fn values(
    &self,
    name: &str,
  ) -> io::Result<Vec<Scalar>>;

  /// Reads a required scalar field as `T`.
  fn get<T: FromScalar>(
    &self,
    name: &str,
  ) -> io::Result<T> {
    let value = self
      .scalar(name)?
      .ok_or_else(|| invalid(&format!("{}.{name} is missing", self.type_name())))?;
    T::from_scalar(value).map_err(|error| self.context(name, error))
  }

  /// Reads an optional scalar field as `T`, defaulting when the schema has no such field.
  fn get_or_default<T: FromScalar + Default>(
    &self,
    name: &str,
  ) -> io::Result<T> {
    match self.scalar(name)? {
      Some(value) => T::from_scalar(value).map_err(|error| self.context(name, error)),
      None => Ok(T::default()),
    }
  }

  /// Reads a primitive or hash array field as `T`; absent fields read as empty.
  fn list<T: FromScalar>(
    &self,
    name: &str,
  ) -> io::Result<Vec<T>> {
    self
      .values(name)?
      .into_iter()
      .map(|value| T::from_scalar(value).map_err(|error| self.context(name, error)))
      .collect()
  }

  /// Prefixes an error with the structure and field it came from.
  fn context(
    &self,
    name: &str,
    error: io::Error,
  ) -> io::Error {
    invalid(&format!("{}.{name}: {error}", self.type_name()))
  }
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
