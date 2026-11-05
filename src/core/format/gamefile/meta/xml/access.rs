//! [`StructReader`] over META structures and their embedded schemas.
//!
//! Field interpretation follows `meta_to_value`, which shares these helpers.

use std::borrow::Cow;

use super::*;
use crate::core::format::gamefile::struct_reader::{FieldName, Scalar, StructReader};

/// One structure instance inside a META resource.
#[derive(Clone)]
pub struct MetaStruct<'a> {
  meta: &'a MetaResource,
  names: &'a HashNames<'a>,
  structure: Cow<'a, MetaStructureInfo>,
  kind: u32,
  block: usize,
  offset: usize,
}

impl MetaResource {
  /// Collects known names, this resource's string blocks and caller-provided names.
  pub fn names<'a>(
    &self,
    shared_names: &'a HashMap<u32, String>,
  ) -> HashNames<'a> {
    HashNames::new(shared_names, self.hash_names(), known_hash_names())
  }

  /// Returns the root structure, resolving hashes through `names` (see [`Self::names`]).
  pub fn root_struct<'a>(
    &'a self,
    names: &'a HashNames<'a>,
  ) -> io::Result<MetaStruct<'a>> {
    if self.root_block_index <= 0 {
      return Err(invalid_data("META resource has no root data block"));
    }
    let block = self.root_block_index as usize - 1;
    let root =
      self.data_blocks.get(block).ok_or_else(|| invalid_data("META root data block is missing"))?;
    MetaStruct::new(self, names, block, 0, root.structure_name_hash)
  }
}

impl<'a> MetaStruct<'a> {
  fn new(
    meta: &'a MetaResource,
    names: &'a HashNames<'a>,
    block: usize,
    offset: usize,
    kind: u32,
  ) -> io::Result<Self> {
    if block >= meta.data_blocks.len() {
      return Err(invalid_data("META structure references a missing data block"));
    }
    let structure = match meta.structures.iter().find(|item| item.name_hash == kind) {
      Some(structure) => Cow::Borrowed(structure),
      None => Cow::Owned(
        fallback_structure(kind)
          .ok_or_else(|| invalid_data(&format!("META structure schema {kind:08X} is missing")))?,
      ),
    };
    Ok(Self {
      meta,
      names,
      structure,
      kind,
      block,
      offset,
    })
  }

  fn data(&self) -> &'a [u8] {
    &self.meta.data_blocks[self.block].data
  }

  /// Finds a named field and the ARRAYINFO entry that describes its elements, if any.
  fn field(
    &self,
    name: FieldName,
  ) -> io::Result<Option<(MetaStructureEntry, Option<MetaStructureEntry>)>> {
    let hash = name.hash;
    let entries = &self.structure.entries;
    Ok(entries.iter().enumerate().find_map(|(index, entry)| {
      (entry.name_hash == hash && entry.name_hash != ARRAY_INFO_HASH).then(|| {
        let info = index
          .checked_sub(1)
          .map(|previous| &entries[previous])
          .filter(|previous| previous.name_hash == ARRAY_INFO_HASH);
        (entry.clone(), info.cloned())
      })
    }))
  }

  fn location(
    &self,
    entry: &MetaStructureEntry,
  ) -> io::Result<usize> {
    self
      .offset
      .checked_add(entry.data_offset as usize)
      .ok_or_else(|| invalid_data("META field offset overflows"))
  }

  fn bytes(
    &self,
    entry: &MetaStructureEntry,
  ) -> io::Result<&'a [u8]> {
    slice(self.data(), self.location(entry)?, field_size(entry.data_type))
  }

  /// Resolves an array descriptor to `(block index, offset, count)`; `None` when empty.
  fn array(
    &self,
    entry: &MetaStructureEntry,
  ) -> io::Result<Option<(usize, usize, usize)>> {
    let descriptor = self.bytes(entry)?;
    let pointer = u64_at(descriptor, 0)?;
    let count = u16_at(descriptor, 8)? as usize;
    let block = (pointer & 0xfff) as usize;
    let offset = ((pointer >> 12) & 0xfffff) as usize;
    Ok((count > 0 && block > 0).then(|| (block - 1, offset, count)))
  }

  fn array_info(
    &self,
    name: FieldName,
    entry: &MetaStructureEntry,
    info: Option<MetaStructureEntry>,
  ) -> io::Result<MetaStructureEntry> {
    if entry.data_type != ARRAY {
      return Err(self.context(name, invalid_data("META field is not an array")));
    }
    info.ok_or_else(|| self.context(name, invalid_data("META array has no ARRAYINFO schema")))
  }

  fn nested(
    &self,
    block: usize,
    offset: usize,
    kind: u32,
  ) -> io::Result<Self> {
    Self::new(self.meta, self.names, block, offset, kind)
  }
}

impl StructReader for MetaStruct<'_> {
  fn type_name(&self) -> String {
    resolve_name(self.kind, self.names)
  }

  fn scalar(
    &self,
    name: FieldName,
  ) -> io::Result<Option<Scalar>> {
    let Some((entry, _)) = self.field(name)? else {
      return Ok(None);
    };
    let read = || -> io::Result<Scalar> {
      let bytes = self.bytes(&entry)?;
      Ok(match entry.data_type {
        0x01 => Scalar::Bool(bytes[0] != 0),
        0x10 => Scalar::Int(bytes[0] as i8 as i64),
        0x11 => Scalar::Int(bytes[0] as i64),
        0x12 => Scalar::Int(i16_at(bytes, 0)? as i64),
        0x13 | 0x60 => Scalar::Int(u16_at(bytes, 0)? as i64),
        0x14 => Scalar::Int(i32_at(bytes, 0)? as i64),
        0x15 => Scalar::Int(u32_at(bytes, 0)? as i64),
        0x21 => Scalar::Float(f32_at(bytes, 0)?),
        0x40 => Scalar::Text(array_of_chars(
          &self.meta.data_blocks[self.block],
          self.location(&entry)?,
          entry.reference_key as usize,
        )?),
        0x44 => Scalar::Text(read_string_pointer(self.meta, bytes)?),
        0x4a => Scalar::Text(resolve_hash(u32_at(bytes, 0)?, self.names)),
        0x62..=0x65 => Scalar::Text(enum_text(self.meta, &entry, bytes, self.names)?),
        kind => return Err(invalid_data(&format!("META field type 0x{kind:02X} is not a scalar"))),
      })
    };
    read().map(Some).map_err(|error| self.context(name, error))
  }

  fn vector(
    &self,
    name: FieldName,
  ) -> io::Result<Option<Vec<f32>>> {
    let Some((entry, _)) = self.field(name)? else {
      return Ok(None);
    };
    let count = match entry.data_type {
      0x33 => 3,
      0x34 => 4,
      kind => {
        return Err(
          self
            .context(name, invalid_data(&format!("META field type 0x{kind:02X} is not a vector"))),
        );
      }
    };
    let bytes = self.bytes(&entry)?;
    (0..count).map(|index| f32_at(bytes, index * 4)).collect::<io::Result<_>>().map(Some)
  }

  fn child(
    &self,
    name: FieldName,
  ) -> io::Result<Option<Self>> {
    let Some((entry, _)) = self.field(name)? else {
      return Ok(None);
    };
    if entry.data_type != STRUCTURE {
      return Err(self.context(name, invalid_data("META field is not an embedded structure")));
    }
    self.nested(self.block, self.location(&entry)?, entry.reference_key).map(Some)
  }

  fn children(
    &self,
    name: FieldName,
  ) -> io::Result<Vec<Self>> {
    let Some((entry, info)) = self.field(name)? else {
      return Ok(Vec::new());
    };
    let info = self.array_info(name, &entry, info)?;
    let Some((mut block, mut offset, count)) = self.array(&entry)? else {
      return Ok(Vec::new());
    };
    match info.data_type {
      STRUCTURE => {
        let size = self
          .meta
          .structures
          .iter()
          .find(|structure| structure.name_hash == info.reference_key)
          .ok_or_else(|| {
            self.context(name, invalid_data("META array structure schema is missing"))
          })?
          .structure_size as usize;
        // Every element shares one schema, found once.
        let first = self.nested(block, offset, info.reference_key)?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
          if block >= self.meta.data_blocks.len() {
            return Err(invalid_data("META structure references a missing data block"));
          }
          items.push(MetaStruct {
            block,
            offset,
            ..first.clone()
          });
          offset += size;
          let length = self.meta.data_blocks.get(block).map_or(0, |block| block.data.len());
          if offset >= length {
            offset -= length;
            block += 1;
          }
        }
        Ok(items)
      }
      STRUCTURE_POINTER => {
        let pointers = &self
          .meta
          .data_blocks
          .get(block)
          .ok_or_else(|| invalid_data("META pointer array references a missing block"))?
          .data;
        (0..count)
          .map(|index| {
            let pointer = u64_at(slice(pointers, offset + index * 8, 8)?, 0)?;
            let target = (pointer & 0xfff) as usize;
            if target == 0 {
              return Err(
                self.context(name, invalid_data("META pointer array holds a null pointer")),
              );
            }
            let kind = self
              .meta
              .data_blocks
              .get(target - 1)
              .ok_or_else(|| invalid_data("META structure pointer references a missing block"))?
              .structure_name_hash;
            self.nested(target - 1, ((pointer >> 12) & 0xfffff) as usize, kind)
          })
          .collect()
      }
      _ => Err(self.context(name, invalid_data("META array does not hold structures"))),
    }
  }

  fn values(
    &self,
    name: FieldName,
  ) -> io::Result<Vec<Scalar>> {
    let mut values = Vec::new();
    self.visit_values(name, &mut |value| {
      values.push(value);
      Ok(())
    })?;
    Ok(values)
  }

  fn list<T: crate::core::format::gamefile::struct_reader::FromScalar>(
    &self,
    name: FieldName,
  ) -> io::Result<Vec<T>> {
    // Converted as read, without collecting the elements as scalars first.
    let mut values = Vec::new();
    self
      .visit_values(name, &mut |value| {
        values.push(T::from_scalar(value)?);
        Ok(())
      })
      .map_err(|error| self.context(name, error))?;
    Ok(values)
  }
}

impl MetaStruct<'_> {
  /// Passes each element of the primitive or hash array field `name` to `visit`, in order;
  /// an absent field has none.
  fn visit_values(
    &self,
    name: FieldName,
    visit: &mut dyn FnMut(Scalar) -> io::Result<()>,
  ) -> io::Result<()> {
    let Some((entry, info)) = self.field(name)? else {
      return Ok(());
    };
    match entry.data_type {
      // Fixed-size inline array; elements default to raw bytes like `inline_bytes_text`.
      0x50 => {
        let info = info.ok_or_else(|| {
          self.context(name, invalid_data("META inline array is missing ARRAYINFO"))
        })?;
        let width = match info.data_type {
          0x12 | 0x13 => 2,
          0x14 | 0x15 | 0x21 => 4,
          _ => 1,
        };
        let count = entry.reference_key as usize;
        let bytes = slice(self.data(), self.location(&entry)?, count.saturating_mul(width))?;
        for bytes in bytes.chunks_exact(width) {
          visit(element(info.data_type, bytes, self.names)?)?;
        }
        Ok(())
      }
      // Pointer to a whole data block holding raw bytes.
      0x59 => {
        let block = (u64_at(self.bytes(&entry)?, 0)? & 0xfff) as usize;
        if block == 0 {
          return Ok(());
        }
        let block = self
          .meta
          .data_blocks
          .get(block - 1)
          .ok_or_else(|| invalid_data("META data pointer references a missing block"))?;
        for byte in &block.data {
          visit(Scalar::Int(*byte as i64))?;
        }
        Ok(())
      }
      _ => {
        let info = self.array_info(name, &entry, info)?;
        let Some((block, offset, count)) = self.array(&entry)? else {
          return Ok(());
        };
        let width = match info.data_type {
          0x11 => 1,
          0x13 => 2,
          0x15 | 0x4a | 0x21 => 4,
          kind => {
            return Err(self.context(
              name,
              invalid_data(&format!("unsupported META array element type 0x{kind:02X}")),
            ));
          }
        };
        let data = &self
          .meta
          .data_blocks
          .get(block)
          .ok_or_else(|| invalid_data("META primitive array references a missing block"))?
          .data;
        for index in 0..count {
          visit(element(info.data_type, slice(data, offset + index * width, width)?, self.names)?)?;
        }
        Ok(())
      }
    }
  }
}

/// Reads one array element of a primitive META type; unknown types read as a raw byte.
fn element(
  data_type: u8,
  bytes: &[u8],
  names: &HashNames,
) -> io::Result<Scalar> {
  Ok(match data_type {
    0x10 => Scalar::Int(bytes[0] as i8 as i64),
    0x12 => Scalar::Int(i16_at(bytes, 0)? as i64),
    0x13 => Scalar::Int(u16_at(bytes, 0)? as i64),
    0x14 => Scalar::Int(i32_at(bytes, 0)? as i64),
    0x15 => Scalar::Int(u32_at(bytes, 0)? as i64),
    0x21 => Scalar::Float(f32_at(bytes, 0)?),
    0x4a => Scalar::Text(resolve_hash(u32_at(bytes, 0)?, names)),
    _ => Scalar::Int(bytes[0] as i64),
  })
}
