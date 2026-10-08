use super::*;

/// Converts a binary RSC7 YMAP into CodeWalker-style META XML using its embedded schema.
pub fn ymap_to_xml(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<String> {
  let resource = Rsc7Resource::decode(bytes)?;
  let meta = MetaResource::parse(&resource)?;
  meta_to_xml(&meta, shared_hash_names)
}

/// Converts a binary YMAP directly into its typed model without serializing XML.
pub fn ymap_to_model(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<crate::core::format::ymap::model::Ymap> {
  ymap_to_model_with_entities(bytes, shared_hash_names).map(|(model, _)| model)
}

pub(crate) fn ymap_to_model_with_entities(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<(
  crate::core::format::ymap::model::Ymap,
  Vec<crate::core::format::ymap::model::YmapEntity>,
)> {
  let resource = Rsc7Resource::decode(bytes)?;
  let meta = MetaResource::parse(&resource)?;
  ymap_to_model_with_entities_from_meta(&meta, shared_hash_names)
}

pub(crate) fn ymap_to_model_with_entities_from_meta(
  meta: &MetaResource,
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<(
  crate::core::format::ymap::model::Ymap,
  Vec<crate::core::format::ymap::model::YmapEntity>,
)> {
  let value = meta_to_value(&meta, shared_hash_names)?;
  let mut parsed: crate::core::format::ymap::xml::XmlYmap =
    serde_json::from_value(value).map_err(|error| invalid_data(&error.to_string()))?;
  let entities = std::mem::take(&mut parsed.entities.items)
    .into_iter()
    .map(crate::core::format::ymap::model::YmapEntity::from)
    .collect::<Vec<_>>();
  let mut model: crate::core::format::ymap::model::Ymap = parsed.into();
  model.entity_map = entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
  Ok((model, entities))
}

fn meta_to_value(
  meta: &MetaResource,
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<Value> {
  if meta.root_block_index <= 0 {
    return Err(invalid_data("META resource has no root data block"));
  }
  let root_index = meta.root_block_index as usize - 1;
  let root = meta
    .data_blocks
    .get(root_index)
    .ok_or_else(|| invalid_data("META root data block is missing"))?;
  let mut names = known_hash_names();
  names.extend(meta.hash_names());
  names.extend(shared_hash_names.iter().map(|(hash, name)| (*hash, name.clone())));
  structure_value(meta, root_index, 0, root.structure_name_hash, &names)
}

fn structure_value(
  meta: &MetaResource,
  block_index: usize,
  offset: usize,
  structure_hash: u32,
  names: &HashMap<u32, String>,
) -> io::Result<Value> {
  let block = meta
    .data_blocks
    .get(block_index)
    .ok_or_else(|| invalid_data("META structure references a missing data block"))?;
  let fallback;
  let structure = match meta.structures.iter().find(|item| item.name_hash == structure_hash) {
    Some(structure) => structure,
    None => {
      fallback = fallback_structure(structure_hash).ok_or_else(|| {
        invalid_data(&format!("META structure schema {structure_hash:08X} is missing"))
      })?;
      &fallback
    }
  };
  let mut value = Map::new();
  let mut array_info = None;
  for entry in &structure.entries {
    if entry.name_hash == ARRAY_INFO_HASH {
      array_info = Some(entry);
      continue;
    }
    let name = if structure_hash == jenk_hash("CLODLight")
      && entry.data_offset == 72
      && entry.data_type == ARRAY
      && entry.name_hash == 0x4a
    {
      "hash".to_string()
    } else {
      resolve_name(entry.name_hash, names)
    };
    let field_offset = offset
      .checked_add(entry.data_offset as usize)
      .ok_or_else(|| invalid_data("META field offset overflows"))?;
    let bytes = slice(&block.data, field_offset, field_size(entry.data_type))?;
    let field = match entry.data_type {
      ARRAY => array_value(meta, structure_hash, array_info, bytes, &name, names)?,
      0x01 => attr_value(Value::Bool(bytes[0] != 0)),
      0x10 => attr_value(Value::Number((bytes[0] as i8).into())),
      0x11 => attr_value(Value::Number(bytes[0].into())),
      0x12 => attr_value(Value::Number(i16_at(bytes, 0)?.into())),
      0x13 => attr_value(Value::Number(u16_at(bytes, 0)?.into())),
      0x14 => attr_value(Value::Number(i32_at(bytes, 0)?.into())),
      0x15 => attr_value(Value::Number(u32_at(bytes, 0)?.into())),
      0x21 => float_attribute_value(structure_hash, &name, bytes)?,
      0x33 => vector_value(bytes, 3)?,
      0x34 => vector_value(bytes, 4)?,
      0x40 => text_value(array_of_chars(block, field_offset, entry.reference_key as usize)?),
      0x44 => text_value(read_string_pointer(meta, bytes)?),
      0x4a => text_value(resolve_hash(u32_at(bytes, 0)?, names)),
      0x50 => text_field_value(
        structure_hash,
        &name,
        inline_bytes_text(entry, array_info, &block.data, field_offset)?,
      ),
      0x59 => text_field_value(structure_hash, &name, data_block_text(meta, bytes)?),
      0x60 => attr_value(Value::Number(u16_at(bytes, 0)?.into())),
      0x62..=0x65 => text_value(enum_text(meta, entry, bytes, names)?),
      0x05 => structure_value(meta, block_index, field_offset, entry.reference_key, names)?,
      _ => {
        return Err(invalid_data(&format!(
          "unsupported META field type 0x{:02X} for {name}",
          entry.data_type
        )));
      }
    };
    value.insert(name, field);
    array_info = None;
  }
  Ok(Value::Object(value))
}

fn array_value(
  meta: &MetaResource,
  structure_hash: u32,
  array_info: Option<&MetaStructureEntry>,
  descriptor: &[u8],
  name: &str,
  names: &HashMap<u32, String>,
) -> io::Result<Value> {
  let info = array_info
    .ok_or_else(|| invalid_data(&format!("META array {name} has no ARRAYINFO schema")))?;
  let pointer = u64_at(descriptor, 0)?;
  let count = u16_at(descriptor, 8)? as usize;
  let block_id = (pointer & 0xfff) as usize;
  let offset = ((pointer >> 12) & 0xfffff) as usize;
  let mut value = Map::new();
  if info.data_type == STRUCTURE {
    value.insert("@itemType".into(), Value::String(resolve_name(info.reference_key, names)));
    if count > 0 && block_id > 0 {
      let mut items = Vec::with_capacity(count);
      let structure = meta
        .structures
        .iter()
        .find(|structure| structure.name_hash == info.reference_key)
        .ok_or_else(|| invalid_data("META array structure schema is missing"))?;
      let mut current_block = block_id - 1;
      let mut current_offset = offset;
      for _ in 0..count {
        items.push(structure_value(
          meta,
          current_block,
          current_offset,
          info.reference_key,
          names,
        )?);
        current_offset += structure.structure_size as usize;
        if current_offset >= meta.data_blocks[current_block].data.len() {
          current_offset -= meta.data_blocks[current_block].data.len();
          current_block += 1;
        }
      }
      let item_key = if (structure_hash == jenk_hash("rage__fwInstancedMapData")
        && name == "GrassInstanceList")
        || (structure_hash == jenk_hash("rage__fwGrassInstanceListDef") && name == "InstanceList")
      {
        "$value"
      } else {
        "Item"
      };
      value.insert(item_key.into(), Value::Array(items));
    }
    return Ok(Value::Object(value));
  }
  if info.data_type == STRUCTURE_POINTER {
    if count > 0 && block_id > 0 {
      let pointer_block = meta
        .data_blocks
        .get(block_id - 1)
        .ok_or_else(|| invalid_data("META pointer array references a missing block"))?;
      let mut items = Vec::with_capacity(count);
      for index in 0..count {
        let pointer = u64_at(slice(&pointer_block.data, offset + index * 8, 8)?, 0)?;
        let target_block_id = (pointer & 0xfff) as usize;
        let target_offset = ((pointer >> 12) & 0xfffff) as usize;
        if target_block_id == 0 {
          items.push(Value::Object(Map::new()));
          continue;
        }
        let target = meta
          .data_blocks
          .get(target_block_id - 1)
          .ok_or_else(|| invalid_data("META structure pointer references a missing block"))?;
        let mut item = structure_value(
          meta,
          target_block_id - 1,
          target_offset,
          target.structure_name_hash,
          names,
        )?;
        item
          .as_object_mut()
          .ok_or_else(|| invalid_data("META pointer target is not a structure"))?
          .insert("@type".into(), Value::String(resolve_name(target.structure_name_hash, names)));
        items.push(item);
      }
      value.insert("Item".into(), Value::Array(items));
    }
    return Ok(Value::Object(value));
  }
  if count == 0 || block_id == 0 {
    return Ok(Value::Object(value));
  }
  let data = meta
    .data_blocks
    .get(block_id - 1)
    .ok_or_else(|| invalid_data("META primitive array references a missing block"))?;
  let stride = match info.data_type {
    0x11 => 1,
    0x13 => 2,
    0x15 | 0x4a | 0x21 => 4,
    _ => return Err(invalid_data(&format!("unsupported META array element type for {name}"))),
  };
  if info.data_type == 0x4a {
    let items = (0..count)
      .map(|index| {
        u32_at(slice(&data.data, offset + index * stride, stride)?, 0)
          .map(|hash| Value::String(resolve_hash(hash, names)))
      })
      .collect::<io::Result<Vec<_>>>()?;
    value.insert("Item".into(), Value::Array(items));
  } else {
    let text = (0..count)
      .map(|index| {
        let bytes = slice(&data.data, offset + index * stride, stride)?;
        Ok(match info.data_type {
          0x11 => bytes[0].to_string(),
          0x13 => u16_at(bytes, 0)?.to_string(),
          0x15 => u32_at(bytes, 0)?.to_string(),
          0x21 => format_float(f32_at(bytes, 0)?),
          _ => unreachable!(),
        })
      })
      .collect::<io::Result<Vec<_>>>()?
      .join(" ");
    value.insert("$value".into(), Value::String(text));
  }
  if structure_hash == jenk_hash("CDistantLODLight") && name == "RGBI" {
    return Ok(Value::Object(value));
  }
  Ok(Value::Object(value))
}

fn attr_value(value: Value) -> Value {
  Value::Object(Map::from_iter([("@value".into(), value)]))
}

fn float_attribute_value(
  structure_hash: u32,
  name: &str,
  bytes: &[u8],
) -> io::Result<Value> {
  let value = f32_at(bytes, 0)?;
  let attribute =
    if structure_hash == jenk_hash("rage__fwGrassInstanceListDef") && name == "OrientToTerrain" {
      Value::String(format_float(value))
    } else {
      number_value(value)?
    };
  Ok(attr_value(attribute))
}

fn text_value(text: String) -> Value {
  Value::String(text)
}

fn text_field_value(
  structure_hash: u32,
  name: &str,
  text: String,
) -> Value {
  let value_list_field = structure_hash == jenk_hash("rage__fwGrassInstanceListDef__InstanceData")
    && matches!(name, "Position" | "Color" | "Pad");
  let bytes_field = name == "verts";
  if value_list_field || bytes_field {
    Value::Object(Map::from_iter([("$value".into(), Value::String(text))]))
  } else {
    text_value(text)
  }
}

fn number_value(value: f32) -> io::Result<Value> {
  Number::from_f64(value as f64)
    .map(Value::Number)
    .ok_or_else(|| invalid_data("META contains a non-finite float"))
}

fn vector_value(
  bytes: &[u8],
  count: usize,
) -> io::Result<Value> {
  let mut value = Map::new();
  for (index, component) in ["x", "y", "z", "w"].iter().take(count).enumerate() {
    value.insert(format!("@{component}"), number_value(f32_at(bytes, index * 4)?)?);
  }
  Ok(Value::Object(value))
}

fn inline_bytes_text(
  entry: &MetaStructureEntry,
  array_info: Option<&MetaStructureEntry>,
  data: &[u8],
  offset: usize,
) -> io::Result<String> {
  let info =
    array_info.ok_or_else(|| invalid_data("META inline byte array is missing ARRAYINFO"))?;
  let count = entry.reference_key as usize;
  let stride = match info.data_type {
    0x12 | 0x13 => 2,
    0x14 | 0x15 | 0x21 => 4,
    _ => 1,
  };
  let bytes = slice(data, offset, count.saturating_mul(stride))?;
  Ok(match info.data_type {
    0x10 => bytes.iter().map(|byte| (*byte as i8).to_string()).collect::<Vec<_>>().join(" "),
    0x11 => bytes.iter().map(u8::to_string).collect::<Vec<_>>().join(" "),
    0x12 => (0..count)
      .map(|index| i16_at(bytes, index * stride).map(|value| value.to_string()))
      .collect::<io::Result<Vec<_>>>()?
      .join(" "),
    0x13 => (0..count)
      .map(|index| u16_at(bytes, index * stride).map(|value| value.to_string()))
      .collect::<io::Result<Vec<_>>>()?
      .join(" "),
    0x14 => (0..count)
      .map(|index| i32_at(bytes, index * stride).map(|value| value.to_string()))
      .collect::<io::Result<Vec<_>>>()?
      .join(" "),
    0x15 => (0..count)
      .map(|index| u32_at(bytes, index * stride).map(|value| value.to_string()))
      .collect::<io::Result<Vec<_>>>()?
      .join(" "),
    0x21 => (0..count)
      .map(|index| f32_at(bytes, index * stride).map(format_float))
      .collect::<io::Result<Vec<_>>>()?
      .join(" "),
    _ => bytes.iter().map(|byte| format!("{byte:02X}")).collect(),
  })
}

fn data_block_text(
  meta: &MetaResource,
  bytes: &[u8],
) -> io::Result<String> {
  let pointer = u64_at(bytes, 0)?;
  let block_id = (pointer & 0xfff) as usize;
  if block_id == 0 {
    return Ok(String::new());
  }
  let block = meta
    .data_blocks
    .get(block_id - 1)
    .ok_or_else(|| invalid_data("META data pointer references a missing block"))?;
  Ok(
    block
      .data
      .chunks(32)
      .map(|chunk| chunk.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" "))
      .collect::<Vec<_>>()
      .join("\n"),
  )
}

fn enum_text(
  meta: &MetaResource,
  entry: &MetaStructureEntry,
  bytes: &[u8],
  names: &HashMap<u32, String>,
) -> io::Result<String> {
  let enum_info = meta.enums.iter().find(|info| info.name_hash == entry.reference_key);
  let raw = if entry.data_type == 0x64 { i16_at(bytes, 0)? as i32 } else { i32_at(bytes, 0)? };
  Ok(if matches!(entry.data_type, 0x63 | 0x65) {
    enum_info
      .into_iter()
      .flat_map(|info| info.entries.iter())
      .filter(|item| item.value >= 0 && raw & (1 << item.value) != 0)
      .map(|item| resolve_name(item.name_hash, names))
      .collect::<Vec<_>>()
      .join(", ")
  } else {
    enum_info
      .and_then(|info| info.entries.iter().find(|item| item.value == raw))
      .map(|item| resolve_name(item.name_hash, names))
      .unwrap_or_else(|| raw.to_string())
  })
}

#[cfg(test)]
mod tests {
  #[test]
  fn grass_orient_to_terrain_float_uses_its_integer_string_adapter() {
    let orient = 1.0f32.to_le_bytes();
    let regular = 1.0f32.to_le_bytes();
    assert_eq!(
      super::float_attribute_value(
        super::jenk_hash("rage__fwGrassInstanceListDef"),
        "OrientToTerrain",
        &orient
      )
      .unwrap(),
      serde_json::json!({"@value": "1"})
    );
    assert_eq!(
      super::float_attribute_value(
        super::jenk_hash("rage__fwGrassInstanceListDef"),
        "lodDist",
        &regular
      )
      .unwrap(),
      serde_json::json!({"@value": 1.0})
    );
  }
}
