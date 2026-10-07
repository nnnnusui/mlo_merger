//! Checked primitive values and native META field-name mappings.

pub(super) use crate::core::format::gamefile::binary_io::write as put;
use crate::core::format::gamefile::meta_resource::jenk_hash;
use serde_json::Value;
use std::io;

pub(super) fn field_name(name: &str) -> String {
  if name == "batch_aabb" {
    return "BatchAABB".into();
  }
  let mut words = name.split('_');
  let mut output = words.next().unwrap_or_default().to_owned();
  for word in words {
    let mut characters = word.chars();
    if let Some(first) = characters.next() {
      output.extend(first.to_uppercase());
      output.extend(characters);
    }
  }
  output
}
pub(super) fn hash_value(value: &Value) -> io::Result<u32> {
  if let Some(number) = value.as_u64() {
    return u32::try_from(number).map_err(invalid);
  }
  let text = value.as_str().ok_or_else(|| invalid("Expected META hash"))?;
  if text.is_empty() {
    return Ok(0);
  }
  if let Some(hash) = text.strip_prefix("hash_") {
    return u32::from_str_radix(hash, 16).map_err(invalid);
  }
  Ok(text.parse().unwrap_or_else(|_| jenk_hash(text)))
}
pub(super) fn integer(value: &Value) -> io::Result<i64> {
  value.as_i64().ok_or_else(|| invalid("Expected signed META integer"))
}
pub(super) fn unsigned(value: &Value) -> io::Result<u64> {
  value
    .as_u64()
    .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    .or_else(|| {
      value
        .as_f64()
        .filter(|value| {
          value.is_finite() && value.fract() == 0.0 && *value >= 0.0 && *value <= u32::MAX as f64
        })
        .map(|value| value as u64)
    })
    .ok_or_else(|| invalid("Expected unsigned META integer"))
}
pub(super) fn float(value: &Value) -> io::Result<f32> {
  value
    .as_f64()
    .map(|value| value as f32)
    .filter(|value| value.is_finite())
    .ok_or_else(|| invalid("Expected finite META float"))
}
pub(super) fn primitive(
  kind: u8,
  value: &Value,
) -> io::Result<Vec<u8>> {
  Ok(match kind {
    0x10 => (i8::try_from(integer(value)?).map_err(invalid)?).to_le_bytes().to_vec(),
    0x11 => (u8::try_from(unsigned(value)?).map_err(invalid)?).to_le_bytes().to_vec(),
    0x12 => (i16::try_from(integer(value)?).map_err(invalid)?).to_le_bytes().to_vec(),
    0x13 => (u16::try_from(unsigned(value)?).map_err(invalid)?).to_le_bytes().to_vec(),
    0x14 => (i32::try_from(integer(value)?).map_err(invalid)?).to_le_bytes().to_vec(),
    0x15 => (u32::try_from(unsigned(value)?).map_err(invalid)?).to_le_bytes().to_vec(),
    0x21 => float(value)?.to_le_bytes().to_vec(),
    0x4a => hash_value(value)?.to_le_bytes().to_vec(),
    _ => return Err(invalid("Unsupported META inline primitive")),
  })
}
pub(super) fn vector(
  value: &Value,
  count: usize,
) -> io::Result<Vec<u8>> {
  let mut bytes = Vec::new();
  for name in ["x", "y", "z", "w"].into_iter().take(count) {
    bytes.extend(
      float(value.get(name).ok_or_else(|| invalid("META vector component missing"))?)?
        .to_le_bytes(),
    );
  }
  Ok(bytes)
}
pub(super) fn invalid(error: impl std::fmt::Display) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
