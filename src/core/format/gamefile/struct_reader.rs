//! Schema-driven, typed field access shared by META and PSO structures.
//!
//! Model decoders are written once against [`StructReader`] and work for any container whose
//! embedded schemas name their fields.

use std::{fmt, io};

/// A structure field's name and its hash, the key schemas store; see [`field!`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldName {
  pub name: &'static str,
  pub hash: u32,
}

impl FieldName {
  /// `name` hashed as schemas do: `hash_XXXXXXXX` stands for the hash `XXXXXXXX` of an unknown
  /// name, any other name for its Jenkins hash.
  pub const fn new(name: &'static str) -> Self {
    Self {
      name,
      hash: field_hash(name),
    }
  }
}

impl fmt::Display for FieldName {
  fn fmt(
    &self,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    formatter.write_str(self.name)
  }
}

/// The hash of a field name; see [`FieldName::new`]. Panics, at compile time for [`field!`], on
/// a `hash_` name that is not eight hexadecimal digits.
const fn field_hash(name: &str) -> u32 {
  let bytes = name.as_bytes();
  let prefix = b"hash_";
  let mut explicit = bytes.len() == prefix.len() + 8;
  let mut index = 0;
  while explicit && index < prefix.len() {
    explicit = bytes[index] == prefix[index];
    index += 1;
  }
  if !explicit {
    return super::meta::jenk_hash(name);
  }
  let mut hash = 0u32;
  while index < bytes.len() {
    let digit = match bytes[index] {
      byte @ b'0'..=b'9' => byte - b'0',
      byte @ b'a'..=b'f' => byte - b'a' + 10,
      byte @ b'A'..=b'F' => byte - b'A' + 10,
      _ => panic!("hash_ field names need eight hexadecimal digits"),
    };
    hash = hash << 4 | digit as u32;
    index += 1;
  }
  hash
}

/// The [`FieldName`] of a literal, hashed at compile time.
macro_rules! field {
  ($name:literal) => {{
    const FIELD: $crate::core::format::gamefile::struct_reader::FieldName =
      $crate::core::format::gamefile::struct_reader::FieldName::new($name);
    FIELD
  }};
}
pub(crate) use field;

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
    name: FieldName,
  ) -> io::Result<Option<Scalar>>;

  /// Reads a float vector field, or `None` when it is absent.
  fn vector(
    &self,
    name: FieldName,
  ) -> io::Result<Option<Vec<f32>>>;

  /// Reads a nested structure field; `None` when absent or a null pointer.
  fn child(
    &self,
    name: FieldName,
  ) -> io::Result<Option<Self>>;

  /// Reads a structure array field; absent fields read as empty.
  fn children(
    &self,
    name: FieldName,
  ) -> io::Result<Vec<Self>>;

  /// Reads a primitive, hash or byte array field; absent fields read as empty.
  fn values(
    &self,
    name: FieldName,
  ) -> io::Result<Vec<Scalar>>;

  /// Reads a required scalar field as `T`.
  fn get<T: FromScalar>(
    &self,
    name: FieldName,
  ) -> io::Result<T> {
    let value = self
      .scalar(name)?
      .ok_or_else(|| invalid(&format!("{}.{name} is missing", self.type_name())))?;
    T::from_scalar(value).map_err(|error| self.context(name, error))
  }

  /// Reads an optional scalar field as `T`, defaulting when the schema has no such field.
  fn get_or_default<T: FromScalar + Default>(
    &self,
    name: FieldName,
  ) -> io::Result<T> {
    match self.scalar(name)? {
      Some(value) => T::from_scalar(value).map_err(|error| self.context(name, error)),
      None => Ok(T::default()),
    }
  }

  /// Reads a primitive or hash array field as `T`; absent fields read as empty.
  fn list<T: FromScalar>(
    &self,
    name: FieldName,
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
    name: FieldName,
    error: io::Error,
  ) -> io::Error {
    invalid(&format!("{}.{name}: {error}", self.type_name()))
  }
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod field_name_tests {
  use super::*;

  #[test]
  fn field_names_hash_as_schemas_name_them() {
    // Ordinary names take the Jenkins hash, computed here at run time.
    for name in ["position", "InstanceList", "CExtensionDefDoor", "rage__phVerletClothCustomBounds"]
    {
      let runtime = {
        let mut hash = 0u32;
        for byte in name.bytes() {
          hash = hash.wrapping_add(byte as u32);
          hash = hash.wrapping_add(hash << 10);
          hash ^= hash >> 6;
        }
        hash = hash.wrapping_add(hash << 3);
        hash ^= hash >> 11;
        hash.wrapping_add(hash << 15)
      };
      assert_eq!(FieldName::new(name).hash, runtime, "{name}");
    }
    // `hash_` names stand for the hash itself, in either case.
    assert_eq!(field!("hash_0000004A").hash, 0x4a);
    assert_eq!(field!("hash_DEADbeef").hash, 0xdead_beef);
    // A name merely starting with `hash_` is an ordinary name.
    assert_eq!(
      FieldName::new("hash_x").hash,
      crate::core::format::gamefile::meta::jenk_hash("hash_x")
    );
    assert_eq!(field!("Scale").to_string(), "Scale");
  }
}
