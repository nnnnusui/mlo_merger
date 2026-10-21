//! Schema-driven, typed field writing into META structures, the counterpart of
//! [`StructReader`](super::struct_reader::StructReader).
//!
//! Model encoders implement [`WriteFields`]: [`StructWriter`] walks a structure's schema entries
//! in order and asks for each field by name hash, so data blocks are allocated in schema order
//! and fields the model has no value for stay zero.

use std::{collections::BTreeSet, io};

use super::{
  binary_io::{align_16, write as put},
  meta::{
    MetaDataBlock, MetaEnumInfo, MetaSchemaCatalog, MetaStructureEntry, MetaStructureInfo,
    jenk_hash,
  },
};

/// Name hash of the entry describing the items of the array entry after it.
const ARRAY_INFO: u32 = 0x100;
/// Block type of strings.
const STRING_TYPE: u32 = 0x10;
/// Block type of raw byte arrays.
const BYTE_TYPE: u32 = 0x11;
/// The most data blocks a META resource can address.
const MAX_BLOCKS: usize = 0xfff;

/// A model value [`StructWriter`] writes as the type its schema entry declares.
///
/// Integers convert to any integer or float field kind within range; floats with no fractional
/// part convert to unsigned kinds; text converts to unsigned kinds, hashes, enums and strings.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Value<'a> {
  Bool(bool),
  Int(i64),
  Float(f32),
  Text(&'a str),
}

macro_rules! value_from {
  ($variant:ident, $cast:ty: $($type:ty),*) => {
    $(impl From<$type> for Value<'_> {
      fn from(value: $type) -> Self {
        Self::$variant(value as $cast)
      }
    })*
  };
}
value_from!(Int, i64: u8, u16, u32, i16, i32);
value_from!(Float, f32: f32);

impl From<bool> for Value<'_> {
  fn from(value: bool) -> Self {
    Self::Bool(value)
  }
}

impl<'a> From<&'a str> for Value<'a> {
  fn from(value: &'a str) -> Self {
    Self::Text(value)
  }
}

impl<'a> From<&'a String> for Value<'a> {
  fn from(value: &'a String) -> Self {
    Self::Text(value)
  }
}

impl Value<'_> {
  fn integer(self) -> io::Result<i64> {
    match self {
      Self::Int(value) => Ok(value),
      _ => Err(invalid("Expected signed META integer")),
    }
  }

  fn unsigned(self) -> io::Result<u64> {
    let value = match self {
      Self::Int(value) => u64::try_from(value).ok(),
      Self::Text(text) => text.parse().ok(),
      Self::Float(value) => (value.is_finite()
        && value.fract() == 0.0
        && value >= 0.0
        && f64::from(value) <= f64::from(u32::MAX))
      .then_some(value as u64),
      Self::Bool(_) => None,
    };
    value.ok_or_else(|| invalid("Expected unsigned META integer"))
  }

  fn float(self) -> io::Result<f32> {
    let value = match self {
      Self::Float(value) => Some(value),
      Self::Int(value) => Some(value as f32),
      _ => None,
    };
    value.filter(|value| value.is_finite()).ok_or_else(|| invalid("Expected finite META float"))
  }

  /// A hash: a number, `hash_XXXXXXXX`, a decimal string, or the Jenkins hash of a name; empty
  /// text is 0.
  fn hash(self) -> io::Result<u32> {
    match self {
      Self::Int(value) => u32::try_from(value).map_err(invalid),
      Self::Text("") => Ok(0),
      Self::Text(text) => match text.strip_prefix("hash_") {
        Some(hash) => u32::from_str_radix(hash, 16).map_err(invalid),
        None => Ok(text.parse().unwrap_or_else(|_| jenk_hash(text))),
      },
      _ => Err(invalid("Expected META hash")),
    }
  }
}

/// A model type written as one META structure.
pub(crate) trait WriteFields {
  /// Writes `field` when this value has one of its name, and otherwise leaves it absent.
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()>;

  /// The structure name of a value in a pointer array, e.g. `CEntityDef`.
  fn type_name(&self) -> Option<&str> {
    None
  }
}

impl<T: WriteFields> WriteFields for &T {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    (*self).write_field(field)
  }

  fn type_name(&self) -> Option<&str> {
    (*self).type_name()
  }
}

/// Allocates META data blocks for structures written from [`WriteFields`] values.
pub(crate) struct StructWriter<'s> {
  schemas: &'s MetaSchemaCatalog,
  blocks: Vec<MetaDataBlock>,
}

impl<'s> StructWriter<'s> {
  pub(crate) fn new(schemas: &'s MetaSchemaCatalog) -> Self {
    Self {
      schemas,
      blocks: Vec::new(),
    }
  }

  /// Writes `value` as the root structure `hash` in the first data block, returning the data
  /// blocks and the schemas they use.
  pub(crate) fn root(
    mut self,
    hash: u32,
    value: &impl WriteFields,
  ) -> io::Result<WrittenMeta> {
    self.blocks.push(MetaDataBlock {
      structure_name_hash: hash,
      data: Vec::new(),
    });
    self.blocks[0].data = self.structure(hash, value)?;
    let (structures, enums) = self.used_schemas();
    Ok(WrittenMeta {
      blocks: self.blocks,
      structures,
      enums,
    })
  }

  /// The schemas of the written blocks' structures and the structures inline in those, and of
  /// the enums their fields use, each sorted by name hash; the set vanilla resources embed.
  fn used_schemas(&self) -> (Vec<MetaStructureInfo>, Vec<MetaEnumInfo>) {
    let mut used = self
      .blocks
      .iter()
      .map(|block| block.structure_name_hash)
      .filter(|hash| self.schemas.structures.contains_key(hash))
      .collect::<BTreeSet<_>>();
    let mut pending = used.iter().copied().collect::<Vec<_>>();
    while let Some(hash) = pending.pop() {
      for entry in &self.schemas.structures[&hash].entries {
        if entry.data_type == 0x05
          && entry.name_hash != ARRAY_INFO
          && used.insert(entry.reference_key)
        {
          pending.push(entry.reference_key);
        }
      }
    }
    let enums = used
      .iter()
      .flat_map(|hash| &self.schemas.structures[hash].entries)
      .filter(|entry| matches!(entry.data_type, 0x62..=0x65))
      .filter_map(|entry| {
        self.schemas.enums.get(&entry.reference_key).map(|schema| (entry.reference_key, schema))
      })
      .collect::<std::collections::BTreeMap<_, _>>();
    (
      used.iter().filter_map(|hash| self.schemas.structures.get(hash).cloned()).collect(),
      enums.into_values().cloned().collect(),
    )
  }

  fn schema(
    &self,
    hash: u32,
  ) -> io::Result<&'s MetaStructureInfo> {
    let schemas = self.schemas;
    schemas
      .structures
      .get(&hash)
      .ok_or_else(|| invalid(format!("YMAP META schema {hash:08X} is unavailable")))
  }

  fn structure(
    &mut self,
    hash: u32,
    value: &impl WriteFields,
  ) -> io::Result<Vec<u8>> {
    let schema = self.schema(hash)?;
    let mut bytes = vec![0; schema.structure_size as usize];
    self.fill(schema, value, &mut bytes)?;
    Ok(bytes)
  }

  /// Writes the fields of `value` into `bytes`, one zeroed structure of `schema`.
  fn fill(
    &mut self,
    schema: &'s MetaStructureInfo,
    value: &impl WriteFields,
    bytes: &mut [u8],
  ) -> io::Result<()> {
    let mut array_info = None;
    for entry in &schema.entries {
      if entry.name_hash == ARRAY_INFO {
        array_info = Some(entry);
        continue;
      }
      let mut field = Field {
        writer: self,
        entry,
        array_info: array_info.take(),
        bytes,
      };
      value.write_field(&mut field)?;
    }
    Ok(())
  }

  fn block(
    &mut self,
    hash: u32,
    mut bytes: Vec<u8>,
  ) -> io::Result<u64> {
    align_16(&mut bytes)?;
    self.raw_block(hash, bytes)
  }

  fn raw_block(
    &mut self,
    hash: u32,
    bytes: Vec<u8>,
  ) -> io::Result<u64> {
    if self.blocks.len() >= MAX_BLOCKS {
      return Err(invalid("Too many META data blocks"));
    }
    self.blocks.push(MetaDataBlock {
      structure_name_hash: hash,
      data: bytes,
    });
    Ok(self.blocks.len() as u64)
  }

  fn enum_value(
    &self,
    value: Value<'_>,
    entry: &MetaStructureEntry,
  ) -> io::Result<i32> {
    let text = match value {
      Value::Int(number) => return i32::try_from(number).map_err(invalid),
      Value::Text(text) => text,
      _ => return Err(invalid("Expected META enum")),
    };
    if let Ok(number) = text.parse() {
      return Ok(number);
    }
    let schema = self
      .schemas
      .enums
      .get(&entry.reference_key)
      .ok_or_else(|| invalid("META enum schema unavailable"))?;
    let mut result = 0;
    // Flags with no bit set read as empty text.
    for name in text.split(',').map(str::trim).filter(|name| !name.is_empty()) {
      let hash = Value::Text(name).hash()?;
      let item = schema
        .entries
        .iter()
        .find(|item| item.name_hash == hash)
        .ok_or_else(|| invalid(format!("Unknown META enum {name}")))?;
      if matches!(entry.data_type, 0x63 | 0x65) {
        result |=
          1i32.checked_shl(item.value as u32).ok_or_else(|| invalid("Invalid META flag bit"))?;
      } else {
        result = item.value;
      }
    }
    Ok(result)
  }
}

/// The data blocks [`StructWriter::root`] wrote and the schemas they use.
pub(crate) struct WrittenMeta {
  pub(crate) blocks: Vec<MetaDataBlock>,
  pub(crate) structures: Vec<MetaStructureInfo>,
  pub(crate) enums: Vec<MetaEnumInfo>,
}

/// One schema entry of a structure being written, filled by [`WriteFields::write_field`].
pub(crate) struct Field<'f, 'w, 's> {
  writer: &'w mut StructWriter<'s>,
  entry: &'s MetaStructureEntry,
  array_info: Option<&'s MetaStructureEntry>,
  bytes: &'f mut [u8],
}

impl<'s> Field<'_, '_, 's> {
  /// The field's name hash.
  pub(crate) fn name(&self) -> u32 {
    self.entry.name_hash
  }

  fn put(
    &mut self,
    data: &[u8],
  ) -> io::Result<()> {
    put(self.bytes, self.entry.data_offset as usize, data)
  }

  fn array_info(&self) -> io::Result<&'s MetaStructureEntry> {
    self.array_info.ok_or_else(|| invalid("META array lacks ARRAYINFO"))
  }

  /// Writes a scalar field: a boolean, number, hash, enum or string.
  pub(crate) fn value<'v>(
    &mut self,
    value: impl Into<Value<'v>>,
  ) -> io::Result<()> {
    let value = value.into();
    match self.entry.data_type {
      0x01 => match value {
        Value::Bool(value) => self.put(&[u8::from(value)]),
        _ => Err(invalid("Expected META boolean")),
      },
      kind @ (0x10..=0x15 | 0x21 | 0x4a) => self.put(&primitive(kind, value)?),
      0x60 => self.put(&primitive(0x11, value)?),
      0x64 => {
        let value = i16::try_from(self.writer.enum_value(value, self.entry)?).map_err(invalid)?;
        self.put(&value.to_le_bytes())
      }
      0x62 | 0x63 | 0x65 => {
        let value = self.writer.enum_value(value, self.entry)?;
        self.put(&value.to_le_bytes())
      }
      0x40 => {
        let Value::Text(text) = value else {
          return Err(invalid("Expected META inline string"));
        };
        let size = self.entry.reference_key as usize;
        if text.len() >= size && !text.is_empty() {
          return Err(invalid("META inline string is too long"));
        }
        let mut buffer = vec![0; size];
        buffer[..text.len()].copy_from_slice(text.as_bytes());
        self.put(&buffer)
      }
      0x44 => {
        let Value::Text(text) = value else {
          return Err(invalid("Expected META string"));
        };
        let count = u16::try_from(text.len()).map_err(invalid)?;
        let pointer = self.writer.block(STRING_TYPE, text.bytes().chain([0]).collect())?;
        self.put(&descriptor(pointer, count))
      }
      kind => Err(unsupported("field", kind)),
    }
  }

  /// Writes a `Vector3` or `Vector4` field from its first three or four `components`.
  pub(crate) fn vector(
    &mut self,
    components: &[f32],
  ) -> io::Result<()> {
    let count = match self.entry.data_type {
      0x33 => 3,
      0x34 => 4,
      kind => return Err(unsupported("field", kind)),
    };
    let components =
      components.get(..count).ok_or_else(|| invalid("META vector component missing"))?;
    let mut bytes = Vec::with_capacity(count * 4);
    for component in components {
      bytes.extend(Value::Float(*component).float()?.to_le_bytes());
    }
    self.put(&bytes)
  }

  /// Writes an inline structure field.
  pub(crate) fn structure(
    &mut self,
    value: &impl WriteFields,
  ) -> io::Result<()> {
    if self.entry.data_type != 0x05 {
      return Err(unsupported("field", self.entry.data_type));
    }
    let schema = self.writer.schema(self.entry.reference_key)?;
    let offset = self.entry.data_offset as usize;
    let end = offset + schema.structure_size as usize;
    let bytes = self
      .bytes
      .get_mut(offset..end)
      .ok_or_else(|| invalid("Binary write range is out of bounds"))?;
    self.writer.fill(schema, value, bytes)
  }

  /// Writes an array of structures, inline or through pointers to their own blocks.
  pub(crate) fn structures<T: WriteFields>(
    &mut self,
    items: impl ExactSizeIterator<Item = T>,
  ) -> io::Result<()> {
    if self.entry.data_type != 0x52 {
      return Err(unsupported("field", self.entry.data_type));
    }
    let info = self.array_info()?;
    let count = u16::try_from(items.len()).map_err(invalid)?;
    if count == 0 {
      return self.put(&[0; 16]);
    }
    let (kind, bytes) = match info.data_type {
      0x05 => {
        let schema = self.writer.schema(info.reference_key)?;
        let size = schema.structure_size as usize;
        let mut bytes = vec![0; size * count as usize];
        for (item, chunk) in items.zip(bytes.chunks_exact_mut(size)) {
          self.writer.fill(schema, &item, chunk)?;
        }
        (info.reference_key, bytes)
      }
      0x07 => {
        let mut bytes = Vec::with_capacity(8 * count as usize);
        for item in items {
          let name = item.type_name().ok_or_else(|| invalid("META entity type missing"))?;
          let hash = Value::Text(name).hash()?;
          let data = self.writer.structure(hash, &item)?;
          bytes.extend(self.writer.block(hash, data)?.to_le_bytes());
        }
        (0x07, bytes)
      }
      kind => return Err(unsupported("array", kind)),
    };
    let pointer = self.writer.block(kind, bytes)?;
    self.put(&descriptor(pointer, count))
  }

  /// Writes an array of primitives or hashes, inline or in its own block.
  pub(crate) fn values<'v, V: Into<Value<'v>>>(
    &mut self,
    items: impl ExactSizeIterator<Item = V>,
  ) -> io::Result<()> {
    let info = self.array_info()?;
    match self.entry.data_type {
      0x50 => {
        if items.len() != self.entry.reference_key as usize {
          return Err(invalid("META inline array size differs from schema"));
        }
        let mut bytes = Vec::new();
        for item in items {
          bytes.extend(primitive(info.data_type, item.into())?);
        }
        self.put(&bytes)
      }
      0x52 => {
        let count = u16::try_from(items.len()).map_err(invalid)?;
        if count == 0 {
          return self.put(&[0; 16]);
        }
        let kind = match info.data_type {
          kind @ (0x11 | 0x13 | 0x15 | 0x21) => kind,
          0x4a => 0x4a,
          kind => return Err(unsupported("array", kind)),
        };
        let mut bytes = Vec::new();
        for item in items {
          bytes.extend(primitive(kind, item.into())?);
        }
        let pointer = self.writer.block(u32::from(kind), bytes)?;
        self.put(&descriptor(pointer, count))
      }
      kind => Err(unsupported("field", kind)),
    }
  }

  /// Writes a raw byte array field into its own unaligned block.
  pub(crate) fn bytes(
    &mut self,
    bytes: Vec<u8>,
  ) -> io::Result<()> {
    if self.entry.data_type != 0x59 {
      return Err(unsupported("field", self.entry.data_type));
    }
    let pointer = self.writer.raw_block(BYTE_TYPE, bytes)?;
    self.put(&pointer.to_le_bytes())
  }
}

/// A 16-byte array or string descriptor: the pointer, then the count and capacity.
fn descriptor(
  pointer: u64,
  count: u16,
) -> [u8; 16] {
  let mut descriptor = [0; 16];
  descriptor[..8].copy_from_slice(&pointer.to_le_bytes());
  descriptor[8..10].copy_from_slice(&count.to_le_bytes());
  descriptor[10..12].copy_from_slice(&count.to_le_bytes());
  descriptor
}

fn primitive(
  kind: u8,
  value: Value<'_>,
) -> io::Result<Vec<u8>> {
  Ok(match kind {
    0x10 => i8::try_from(value.integer()?).map_err(invalid)?.to_le_bytes().to_vec(),
    0x11 => u8::try_from(value.unsigned()?).map_err(invalid)?.to_le_bytes().to_vec(),
    0x12 => i16::try_from(value.integer()?).map_err(invalid)?.to_le_bytes().to_vec(),
    0x13 => u16::try_from(value.unsigned()?).map_err(invalid)?.to_le_bytes().to_vec(),
    0x14 => i32::try_from(value.integer()?).map_err(invalid)?.to_le_bytes().to_vec(),
    0x15 => u32::try_from(value.unsigned()?).map_err(invalid)?.to_le_bytes().to_vec(),
    0x21 => value.float()?.to_le_bytes().to_vec(),
    0x4a => value.hash()?.to_le_bytes().to_vec(),
    _ => return Err(invalid("Unsupported META inline primitive")),
  })
}

fn unsupported(
  what: &str,
  kind: u8,
) -> io::Error {
  invalid(format!("Unsupported direct YMAP META {what} type {kind:02X}"))
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entry(
    name: &str,
    data_offset: u32,
    data_type: u8,
  ) -> MetaStructureEntry {
    MetaStructureEntry {
      name_hash: jenk_hash(name),
      data_offset,
      data_type,
      unknown: 0,
      reference_type_index: 0,
      reference_key: 0,
    }
  }

  fn schemas(entries: Vec<MetaStructureEntry>) -> MetaSchemaCatalog {
    let hash = jenk_hash("Root");
    let mut schemas = MetaSchemaCatalog::default();
    schemas.structures.insert(
      hash,
      MetaStructureInfo {
        name_hash: hash,
        structure_key: 1,
        unknown_8: 0,
        unknown_12: 0,
        unknown_28: 0,
        structure_size: 16,
        entries,
      },
    );
    schemas
  }

  /// Writes `value` to every field named `flags`, `parentIndex` or `scaleXY`.
  struct Fields(Value<'static>, Value<'static>, Value<'static>);

  impl WriteFields for Fields {
    fn write_field(
      &self,
      field: &mut Field<'_, '_, '_>,
    ) -> io::Result<()> {
      match field.name() {
        name if name == jenk_hash("flags") => field.value(self.0),
        name if name == jenk_hash("parentIndex") => field.value(self.1),
        name if name == jenk_hash("scaleXY") => field.value(self.2),
        _ => Ok(()),
      }
    }
  }

  fn write(
    entries: Vec<MetaStructureEntry>,
    value: Fields,
  ) -> io::Result<Vec<u8>> {
    let schemas = schemas(entries);
    Ok(StructWriter::new(&schemas).root(jenk_hash("Root"), &value)?.blocks.remove(0).data)
  }

  #[test]
  fn writes_fields_at_their_offsets_and_leaves_absent_ones_zero() {
    let entries = vec![
      entry("flags", 0, 0x15),
      entry("parentIndex", 4, 0x14),
      entry("scaleXY", 8, 0x21),
      entry("unknown", 12, 0x15),
    ];
    let bytes = write(entries, Fields(Value::Int(9), Value::Int(-1), Value::Float(1.25))).unwrap();
    assert_eq!(&bytes[..4], &9u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &(-1i32).to_le_bytes());
    assert_eq!(&bytes[8..12], &1.25f32.to_le_bytes());
    assert_eq!(&bytes[12..], &[0; 4]);
  }

  #[test]
  fn rejects_values_outside_the_field_kind() {
    let entries = |kind| vec![entry("flags", 0, kind)];
    let fields = |value| Fields(value, Value::Int(0), Value::Int(0));
    assert!(write(entries(0x11), fields(Value::Int(256))).is_err());
    assert!(write(entries(0x13), fields(Value::Int(65536))).is_err());
    assert!(write(entries(0x13), fields(Value::Float(1.5))).is_err());
    assert!(write(entries(0x14), fields(Value::Float(1.0))).is_err());
    assert!(write(entries(0x21), fields(Value::Float(f32::NAN))).is_err());
    // Unsigned kinds take whole floats and decimal text, as model fields spell them.
    assert_eq!(write(entries(0x13), fields(Value::Float(7.0))).unwrap()[..2], 7u16.to_le_bytes());
    assert_eq!(write(entries(0x15), fields(Value::Text("12"))).unwrap()[..4], 12u32.to_le_bytes());
  }

  #[test]
  fn hashes_names_and_reads_spelled_hashes() {
    let entries = vec![entry("flags", 0, 0x4a)];
    let hash = |text| {
      let bytes = write(entries.clone(), Fields(Value::Text(text), Value::Int(0), Value::Int(0)));
      u32::from_le_bytes(bytes.unwrap()[..4].try_into().unwrap())
    };
    assert_eq!(hash("prop"), jenk_hash("prop"));
    assert_eq!(hash("hash_0000ABCD"), 0xABCD);
    assert_eq!(hash("42"), 42);
    assert_eq!(hash(""), 0);
  }
}
