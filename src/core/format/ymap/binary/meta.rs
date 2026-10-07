//! Allocates META structures, arrays, strings and enum fields from schema layouts.

use super::values::{field_name, float, hash_value, invalid, primitive, put, unsigned, vector};
use crate::core::format::gamefile::meta_resource::{
  MetaDataBlock, MetaSchemaCatalog, MetaStructureEntry, jenk_hash,
};
use serde_json::Value;
use std::{collections::HashMap, io};

pub(super) struct Writer<'a> {
  pub(super) catalog: &'a MetaSchemaCatalog,
  pub(super) blocks: Vec<MetaDataBlock>,
}

impl Writer<'_> {
  pub(super) fn structure(
    &mut self,
    hash: u32,
    value: &Value,
  ) -> io::Result<Vec<u8>> {
    let schema = self
      .catalog
      .structures
      .get(&hash)
      .ok_or_else(|| invalid(format!("YMAP META schema {hash:08X} is unavailable")))?;
    let mut fields = HashMap::new();
    for (name, value) in
      value.as_object().ok_or_else(|| invalid("META structure value is not an object"))?
    {
      let name = field_name(name);
      fields.insert(jenk_hash(&name), value);
      if let Some(first) = name.chars().next() {
        fields.insert(
          jenk_hash(&format!("{}{}", first.to_ascii_uppercase(), &name[first.len_utf8()..])),
          value,
        );
      }
    }
    if let Some(value) = fields.get(&jenk_hash("hash")).copied() {
      fields.insert(0x4a, value);
    }
    let mut bytes = vec![0; schema.structure_size as usize];
    let mut array_info = None;
    for entry in &schema.entries {
      if entry.name_hash == 0x100 {
        array_info = Some(entry);
        continue;
      }
      let Some(value) = fields.get(&entry.name_hash) else {
        array_info = None;
        continue;
      };
      let offset = entry.data_offset as usize;
      let data = match entry.data_type {
        0x01 => vec![u8::from(value.as_bool().ok_or_else(|| invalid("Expected META boolean"))?)],
        0x10..=0x15 | 0x21 => primitive(entry.data_type, value)?,
        0x60 => primitive(0x11, value)?,
        0x64 => {
          i16::try_from(self.enum_value(value, entry)?).map_err(invalid)?.to_le_bytes().to_vec()
        }
        0x33 | 0x34 => vector(value, if entry.data_type == 0x33 { 3 } else { 4 })?,
        0x4a => hash_value(value)?.to_le_bytes().to_vec(),
        0x05 => self.structure(entry.reference_key, value)?,
        0x52 => {
          self.array(value, array_info.ok_or_else(|| invalid("META array lacks ARRAYINFO"))?)?
        }
        0x50 => {
          let info = array_info.ok_or_else(|| invalid("META inline array lacks ARRAYINFO"))?;
          let items = value.as_array().ok_or_else(|| invalid("Expected META inline array"))?;
          if items.len() != entry.reference_key as usize {
            return Err(invalid("META inline array size differs from schema"));
          }
          let mut bytes = Vec::new();
          for item in items {
            bytes.extend(primitive(info.data_type, item)?);
          }
          bytes
        }
        0x59 => {
          let items = value.as_array().ok_or_else(|| invalid("Expected raw META byte array"))?;
          let bytes = items
            .iter()
            .map(|value| u8::try_from(unsigned(value)?).map_err(invalid))
            .collect::<io::Result<Vec<_>>>()?;
          self.raw_block(0x11, bytes)?.to_le_bytes().to_vec()
        }
        0x40 => {
          let text =
            value.as_str().ok_or_else(|| invalid("Expected META inline string"))?.as_bytes();
          let mut buffer = vec![0; entry.reference_key as usize];
          if text.len() >= buffer.len() && !text.is_empty() {
            return Err(invalid("META inline string is too long"));
          }
          buffer[..text.len()].copy_from_slice(text);
          buffer
        }
        0x44 => {
          let text = value.as_str().ok_or_else(|| invalid("Expected META string"))?;
          let count = u16::try_from(text.len()).map_err(invalid)?;
          let pointer = self.block(0x10, text.as_bytes().iter().copied().chain([0]).collect())?;
          let mut descriptor = vec![0; 16];
          descriptor[..8].copy_from_slice(&pointer.to_le_bytes());
          descriptor[8..10].copy_from_slice(&count.to_le_bytes());
          descriptor[10..12].copy_from_slice(&count.to_le_bytes());
          descriptor
        }
        0x62 | 0x63 | 0x65 => self.enum_value(value, entry)?.to_le_bytes().to_vec(),
        kind => return Err(invalid(format!("Unsupported direct YMAP META field type {kind:02X}"))),
      };
      put(&mut bytes, offset, &data)?;
      array_info = None;
    }
    Ok(bytes)
  }

  fn array(
    &mut self,
    value: &Value,
    info: &MetaStructureEntry,
  ) -> io::Result<Vec<u8>> {
    let items = value.as_array().ok_or_else(|| invalid("Expected META array"))?;
    let count = u16::try_from(items.len()).map_err(invalid)?;
    if count == 0 {
      return Ok(vec![0; 16]);
    }
    let mut bytes = Vec::new();
    for item in items {
      match info.data_type {
        0x05 => bytes.extend(self.structure(info.reference_key, item)?),
        0x07 => {
          let kind = item
            .get("entity_type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("META entity type missing"))?;
          let hash = hash_value(&Value::String(kind.into()))?;
          let data = self.structure(hash, item)?;
          let pointer = self.block(hash, data)?;
          bytes.extend(pointer.to_le_bytes());
        }
        0x11 => bytes.extend(primitive(0x11, item)?),
        0x13 => bytes.extend(primitive(0x13, item)?),
        0x15 => bytes.extend(u32::try_from(unsigned(item)?).map_err(invalid)?.to_le_bytes()),
        0x21 => bytes.extend(float(item)?.to_le_bytes()),
        0x4a => bytes.extend(hash_value(item)?.to_le_bytes()),
        kind => return Err(invalid(format!("Unsupported direct YMAP META array type {kind:02X}"))),
      }
    }
    let kind = match info.data_type {
      0x05 => info.reference_key,
      0x07 => 0x07,
      0x11 => 0x11,
      0x13 => 0x13,
      0x15 => 0x15,
      0x21 => 0x21,
      _ => 0x4a,
    };
    let pointer = self.block(kind, bytes)?;
    let mut descriptor = vec![0; 16];
    descriptor[..8].copy_from_slice(&pointer.to_le_bytes());
    descriptor[8..10].copy_from_slice(&count.to_le_bytes());
    descriptor[10..12].copy_from_slice(&count.to_le_bytes());
    Ok(descriptor)
  }

  fn block(
    &mut self,
    hash: u32,
    mut bytes: Vec<u8>,
  ) -> io::Result<u64> {
    crate::core::format::gamefile::binary_io::align_16(&mut bytes)?;
    self.raw_block(hash, bytes)
  }

  fn raw_block(
    &mut self,
    hash: u32,
    bytes: Vec<u8>,
  ) -> io::Result<u64> {
    if self.blocks.len() >= 0xfff {
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
    value: &Value,
    entry: &MetaStructureEntry,
  ) -> io::Result<i32> {
    if let Some(number) = value.as_i64() {
      return i32::try_from(number).map_err(invalid);
    }
    let text = value.as_str().ok_or_else(|| invalid("Expected META enum"))?;
    if let Ok(number) = text.parse() {
      return Ok(number);
    }
    let schema = self
      .catalog
      .enums
      .get(&entry.reference_key)
      .ok_or_else(|| invalid("META enum schema unavailable"))?;
    let mut result = 0;
    for name in text.split(',').map(str::trim) {
      let hash = hash_value(&Value::String(name.into()))?;
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
