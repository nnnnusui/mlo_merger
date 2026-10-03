use std::collections::HashMap;
use std::io;

use super::meta_resource::{
  MetaDataBlock, MetaEnumInfo, MetaResource, MetaSchemaCatalog, MetaStructureEntry,
  MetaStructureInfo,
};
use super::xml_tree::{XmlElement, parse_xml};

const ARRAY_INFO_HASH: u32 = 0x100;
const STRUCTURE: u8 = 0x05;
const STRUCTURE_POINTER: u8 = 0x07;
const ARRAY: u8 = 0x52;
const META_STRING_TYPE: u32 = 0x10;
const META_BYTE_TYPE: u32 = 0x11;
const META_USHORT_TYPE: u32 = 0x13;
const META_UINT_TYPE: u32 = 0x15;
const META_FLOAT_TYPE: u32 = 0x21;
const META_HASH_TYPE: u32 = 0x4a;
const META_POINTER_TYPE: u32 = 0x07;

/// Reconstructs META data blocks from YMAP XML using schemas collected from binary resources.
pub fn meta_from_xml(
  xml: &str,
  catalog: &MetaSchemaCatalog,
) -> io::Result<MetaResource> {
  let root = parse_xml(xml)?;
  let structures = &catalog.structures;
  let enums = &catalog.enums;
  let names = &catalog.hash_names;

  let root_hash = parse_name_hash(&root.name)?;
  if !structures.contains_key(&root_hash) {
    return Err(invalid_data(&format!("YMAP root schema '{}' is unavailable", root.name)));
  }

  let mut data_blocks = vec![MetaDataBlock {
    structure_name_hash: root_hash,
    data: Vec::new(),
  }];
  data_blocks[0].data =
    build_structure(&root, root_hash, structures, enums, names, &mut data_blocks)?;

  Ok(MetaResource {
    root_block_index: 1,
    structures: structures.values().cloned().collect(),
    enums: enums.values().cloned().collect(),
    data_blocks,
    name: root.attributes.get("name").cloned(),
  })
}

fn build_structure(
  node: &XmlElement,
  structure_hash: u32,
  structures: &HashMap<u32, MetaStructureInfo>,
  enums: &HashMap<u32, MetaEnumInfo>,
  names: &HashMap<u32, String>,
  data_blocks: &mut Vec<MetaDataBlock>,
) -> io::Result<Vec<u8>> {
  let schema = structures
    .get(&structure_hash)
    .ok_or_else(|| invalid_data(&format!("META schema {structure_hash:08X} is unavailable")))?;
  let mut data = vec![0; schema.structure_size as usize];
  let mut array_info: Option<&MetaStructureEntry> = None;

  for entry in &schema.entries {
    if entry.name_hash == ARRAY_INFO_HASH {
      array_info = Some(entry);
      continue;
    }
    let Some(child) = find_child(node, entry.name_hash) else {
      array_info = None;
      continue;
    };
    let offset = entry.data_offset as usize;
    match entry.data_type {
      ARRAY => {
        let info = array_info.ok_or_else(|| invalid_data("META array has no ARRAYINFO entry"))?;
        let descriptor = build_array(child, info, structures, enums, names, data_blocks)?;
        copy_at(&mut data, offset, &descriptor)?;
      }
      0x01 => data[offset] = u8::from(parse_bool(child)?),
      0x10 => copy_at(&mut data, offset, &(parse_i32(child)? as i8).to_le_bytes())?,
      0x11 | 0x60 => data[offset] = parse_i32(child)? as u8,
      0x12 | 0x64 => copy_at(&mut data, offset, &(parse_i32(child)? as i16).to_le_bytes())?,
      0x13 => copy_at(&mut data, offset, &(parse_i32(child)? as u16).to_le_bytes())?,
      0x14 => copy_at(&mut data, offset, &parse_i32(child)?.to_le_bytes())?,
      0x15 => copy_at(&mut data, offset, &parse_u32(child)?.to_le_bytes())?,
      0x21 => copy_at(&mut data, offset, &parse_f32(child)?.to_le_bytes())?,
      0x33 => write_vector(&mut data, offset, child, 3)?,
      0x34 => write_vector(&mut data, offset, child, 4)?,
      0x40 => write_inline_chars(&mut data, offset, entry.reference_key as usize, child)?,
      0x44 => {
        let text = text_or_value(child);
        let pointer = add_string(data_blocks, &text)?;
        copy_at(&mut data, offset, &pointer.to_le_bytes())?;
        copy_at(&mut data, offset + 8, &(text.len() as u16).to_le_bytes())?;
        copy_at(&mut data, offset + 10, &(text.len() as u16).to_le_bytes())?;
      }
      0x4a => copy_at(&mut data, offset, &parse_hash(&text_or_value(child))?.to_le_bytes())?,
      0x50 => {
        let info =
          array_info.ok_or_else(|| invalid_data("META inline array has no ARRAYINFO entry"))?;
        write_inline_array(&mut data, offset, entry.reference_key as usize, info.data_type, child)?;
      }
      0x59 => {
        let bytes = parse_hex_bytes(&child.text)?;
        let pointer = add_data_block_raw(data_blocks, META_BYTE_TYPE, bytes)?;
        copy_at(&mut data, offset, &pointer.to_le_bytes())?;
      }
      0x62..=0x65 => {
        let enum_value = parse_enum(child, entry.reference_key, entry.data_type, enums, names)?;
        if entry.data_type == 0x64 {
          copy_at(&mut data, offset, &(enum_value as i16).to_le_bytes())?;
        } else {
          copy_at(&mut data, offset, &enum_value.to_le_bytes())?;
        }
      }
      STRUCTURE => {
        let nested =
          build_structure(child, entry.reference_key, structures, enums, names, data_blocks)?;
        copy_at(&mut data, offset, &nested)?;
      }
      _ => {
        return Err(invalid_data(&format!(
          "unsupported XML META field type 0x{:02X}",
          entry.data_type
        )));
      }
    }
    array_info = None;
  }
  if structure_hash == jenk_hash("CDistantLODLight") {
    let position_count = u16::from_le_bytes(data[16..18].try_into().unwrap());
    let color_count = u16::from_le_bytes(data[32..34].try_into().unwrap());
    if position_count != color_count {
      return Err(invalid_data("distant LOD light position/RGBI array counts differ"));
    }
  }
  Ok(data)
}

fn build_array(
  node: &XmlElement,
  info: &MetaStructureEntry,
  structures: &HashMap<u32, MetaStructureInfo>,
  enums: &HashMap<u32, MetaEnumInfo>,
  names: &HashMap<u32, String>,
  data_blocks: &mut Vec<MetaDataBlock>,
) -> io::Result<Vec<u8>> {
  let items = node
    .children
    .iter()
    .filter(|child| {
      child.name == "Item"
        || (info.data_type == STRUCTURE
          && info.reference_key == jenk_hash("FloatXYZ")
          && child.name == "XmlPositionChildValueAttr")
    })
    .collect::<Vec<_>>();
  if items.len() != node.children.len() && matches!(info.data_type, STRUCTURE | STRUCTURE_POINTER) {
    return Err(invalid_data("META structure array contains unsupported XML item elements"));
  }
  let primitive_array = matches!(info.data_type, 0x11 | 0x13 | 0x15 | 0x21 | 0x4a);
  let text_values = if primitive_array && items.is_empty() {
    node.text.split_whitespace().map(str::to_string).collect::<Vec<_>>()
  } else {
    Vec::new()
  };
  let count = if primitive_array && items.is_empty() { text_values.len() } else { items.len() };
  if count > u16::MAX as usize {
    return Err(invalid_data("META array exceeds the supported item count"));
  }
  if count == 0 {
    return Ok(vec![0; 16]);
  }

  let (block_id, offset) = match info.data_type {
    STRUCTURE => {
      let mut bytes = Vec::new();
      for item in &items {
        bytes.extend_from_slice(&build_structure(
          item,
          info.reference_key,
          structures,
          enums,
          names,
          data_blocks,
        )?);
      }
      (add_data_block(data_blocks, info.reference_key, bytes)?, 0usize)
    }
    STRUCTURE_POINTER => {
      let mut pointers = Vec::new();
      for item in &items {
        let type_name = item
          .attributes
          .get("type")
          .ok_or_else(|| invalid_data("META structure pointer array item has no type attribute"))?;
        let type_hash = parse_name_hash(type_name)?;
        let bytes = build_structure(item, type_hash, structures, enums, names, data_blocks)?;
        let target_block = add_data_block(data_blocks, type_hash, bytes)?;
        pointers.extend_from_slice(&(target_block as u64).to_le_bytes());
      }
      (add_data_block(data_blocks, META_POINTER_TYPE, pointers)?, 0usize)
    }
    0x11 | 0x13 | 0x15 | 0x21 | 0x4a => {
      let mut bytes = Vec::new();
      let values = if items.is_empty() {
        text_values
      } else {
        items.iter().map(|item| text_or_value(item)).collect()
      };
      for value in &values {
        match info.data_type {
          0x11 => bytes.push(
            value.parse::<u8>().map_err(|_| invalid_data("META byte array item is invalid"))?,
          ),
          0x13 => bytes.extend_from_slice(
            &value
              .parse::<u16>()
              .map_err(|_| invalid_data("META ushort array item is invalid"))?
              .to_le_bytes(),
          ),
          0x15 => bytes.extend_from_slice(
            &value
              .parse::<u32>()
              .map_err(|_| invalid_data("META uint array item is invalid"))?
              .to_le_bytes(),
          ),
          0x21 => bytes.extend_from_slice(
            &value
              .parse::<f32>()
              .map_err(|_| invalid_data("META float array item is invalid"))?
              .to_le_bytes(),
          ),
          0x4a => bytes.extend_from_slice(&parse_hash(value)?.to_le_bytes()),
          _ => unreachable!(),
        }
      }
      let type_hash = match info.data_type {
        0x11 => META_BYTE_TYPE,
        0x13 => META_USHORT_TYPE,
        0x15 => META_UINT_TYPE,
        0x21 => META_FLOAT_TYPE,
        _ => META_HASH_TYPE,
      };
      (add_data_block(data_blocks, type_hash, bytes)?, 0usize)
    }
    _ => {
      return Err(invalid_data(&format!(
        "unsupported XML META array type 0x{:02X}",
        info.data_type
      )));
    }
  };

  let pointer = ((offset as u64) << 12) | block_id as u64;
  let mut descriptor = vec![0; 16];
  descriptor[0..8].copy_from_slice(&pointer.to_le_bytes());
  descriptor[8..10].copy_from_slice(&(count as u16).to_le_bytes());
  descriptor[10..12].copy_from_slice(&(count as u16).to_le_bytes());
  Ok(descriptor)
}

fn find_child(
  node: &XmlElement,
  hash: u32,
) -> Option<&XmlElement> {
  node.children.iter().find(|child| parse_name_hash(&child.name).ok() == Some(hash))
}

fn parse_name_hash(name: &str) -> io::Result<u32> {
  if let Some(hash) = name.strip_prefix("hash_") {
    return u32::from_str_radix(hash, 16)
      .map_err(|_| invalid_data("META hash tag contains an invalid hexadecimal hash"));
  }
  Ok(jenk_hash(name))
}

fn parse_bool(node: &XmlElement) -> io::Result<bool> {
  let text = text_or_value(node);
  match text.as_str() {
    "true" | "1" => Ok(true),
    "false" | "0" | "" => Ok(false),
    _ => Err(invalid_data("META boolean XML value is invalid")),
  }
}

fn parse_i32(node: &XmlElement) -> io::Result<i32> {
  text_or_value(node).parse().map_err(|_| invalid_data("META integer XML value is invalid"))
}

fn parse_u32(node: &XmlElement) -> io::Result<u32> {
  let value = text_or_value(node);
  if let Some(hash) = value.strip_prefix("0x") {
    u32::from_str_radix(hash, 16).map_err(|_| invalid_data("META hex integer XML value is invalid"))
  } else {
    value.parse().map_err(|_| invalid_data("META unsigned XML value is invalid"))
  }
}

fn parse_f32(node: &XmlElement) -> io::Result<f32> {
  text_or_value(node).parse().map_err(|_| invalid_data("META float XML value is invalid"))
}

fn text_or_value(node: &XmlElement) -> String {
  node.attributes.get("value").cloned().unwrap_or_else(|| node.text.trim().to_string())
}

fn parse_hash(value: &str) -> io::Result<u32> {
  if value.is_empty() {
    return Ok(0);
  }
  if let Some(hash) = value.strip_prefix("hash_") {
    return u32::from_str_radix(hash, 16)
      .map_err(|_| invalid_data("META hash XML value is invalid"));
  }
  if let Ok(hash) = value.parse::<u32>() {
    return Ok(hash);
  }
  Ok(jenk_hash(value))
}

fn parse_enum(
  node: &XmlElement,
  enum_hash: u32,
  data_type: u8,
  enums: &HashMap<u32, MetaEnumInfo>,
  names: &HashMap<u32, String>,
) -> io::Result<i32> {
  let value = text_or_value(node);
  if let Ok(number) = value.parse::<i32>() {
    return Ok(number);
  }
  let enum_info = enums
    .get(&enum_hash)
    .ok_or_else(|| invalid_data(&format!("META enum {enum_hash:08X} is unavailable")))?;
  if matches!(data_type, 0x63 | 0x65) {
    let mut flags = 0i32;
    for token in value.split(',').map(str::trim).filter(|token| !token.is_empty()) {
      let hash = parse_name_hash(token)?;
      let item = enum_info
        .entries
        .iter()
        .find(|entry| {
          entry.name_hash == hash || names.get(&entry.name_hash).is_some_and(|name| name == token)
        })
        .ok_or_else(|| invalid_data(&format!("META enum entry '{token}' is unavailable")))?;
      flags |= 1 << item.value;
    }
    Ok(flags)
  } else {
    let hash = parse_name_hash(&value)?;
    enum_info
      .entries
      .iter()
      .find(|entry| {
        entry.name_hash == hash || names.get(&entry.name_hash).is_some_and(|name| name == &value)
      })
      .map(|entry| entry.value)
      .ok_or_else(|| invalid_data(&format!("META enum entry '{value}' is unavailable")))
  }
}

fn write_vector(
  data: &mut [u8],
  offset: usize,
  node: &XmlElement,
  components: usize,
) -> io::Result<()> {
  for (index, name) in ["x", "y", "z", "w"].iter().take(components).enumerate() {
    let value = node
      .attributes
      .get(*name)
      .ok_or_else(|| invalid_data(&format!("META vector lacks '{name}'")))?
      .parse::<f32>()
      .map_err(|_| invalid_data("META vector component is invalid"))?;
    copy_at(data, offset + index * 4, &value.to_le_bytes())?;
  }
  Ok(())
}

fn write_inline_chars(
  data: &mut [u8],
  offset: usize,
  length: usize,
  node: &XmlElement,
) -> io::Result<()> {
  let text = text_or_value(node);
  let bytes = text.as_bytes();
  if bytes.len() > length {
    return Err(invalid_data("META inline string exceeds schema length"));
  }
  copy_at(data, offset, bytes)
}

fn write_inline_array(
  data: &mut [u8],
  offset: usize,
  count: usize,
  data_type: u8,
  node: &XmlElement,
) -> io::Result<()> {
  let values = if !node.children.is_empty() {
    node.children.iter().map(text_or_value).collect::<Vec<_>>()
  } else {
    node.text.split_whitespace().map(str::to_string).collect::<Vec<_>>()
  };
  if values.len() != count {
    return Err(invalid_data("META inline array length does not match its schema"));
  }
  for (index, value) in values.iter().enumerate() {
    let position = offset
      + index
        * match data_type {
          0x12 | 0x13 => 2,
          0x14 | 0x15 | 0x21 => 4,
          _ => 1,
        };
    match data_type {
      0x10 => copy_at(
        data,
        position,
        &(value.parse::<i8>().map_err(|_| invalid_data("META sbyte is invalid"))?).to_le_bytes(),
      )?,
      0x11 => copy_at(
        data,
        position,
        &(value.parse::<u8>().map_err(|_| invalid_data("META byte is invalid"))?).to_le_bytes(),
      )?,
      0x12 => copy_at(
        data,
        position,
        &(value.parse::<i16>().map_err(|_| invalid_data("META short is invalid"))?).to_le_bytes(),
      )?,
      0x13 => copy_at(
        data,
        position,
        &(value.parse::<u16>().map_err(|_| invalid_data("META ushort is invalid"))?).to_le_bytes(),
      )?,
      0x14 => copy_at(
        data,
        position,
        &(value.parse::<i32>().map_err(|_| invalid_data("META int is invalid"))?).to_le_bytes(),
      )?,
      0x15 => copy_at(
        data,
        position,
        &(value.parse::<u32>().map_err(|_| invalid_data("META uint is invalid"))?).to_le_bytes(),
      )?,
      0x21 => copy_at(
        data,
        position,
        &(value.parse::<f32>().map_err(|_| invalid_data("META float is invalid"))?).to_le_bytes(),
      )?,
      0x4a => copy_at(data, position, &parse_hash(value)?.to_le_bytes())?,
      _ => {
        return Err(invalid_data(&format!("unsupported META inline array type 0x{data_type:02X}")));
      }
    }
  }
  Ok(())
}

fn parse_hex_bytes(text: &str) -> io::Result<Vec<u8>> {
  let tokens = text.split_whitespace().collect::<Vec<_>>();
  if tokens.len() > 1 || tokens.first().is_some_and(|token| token.starts_with("0x")) {
    return tokens
      .iter()
      .map(|token| {
        let value = token.strip_prefix("0x").unwrap_or(token);
        u8::from_str_radix(value, 16)
          .map_err(|_| invalid_data("META byte array contains invalid hex"))
      })
      .collect();
  }
  let compact = tokens.first().copied().unwrap_or("");
  let compact = compact.strip_prefix("0x").unwrap_or(compact);
  if compact.len() % 2 != 0 {
    return Err(invalid_data("META compact hex byte array has odd length"));
  }
  (0..compact.len())
    .step_by(2)
    .map(|offset| {
      u8::from_str_radix(&compact[offset..offset + 2], 16)
        .map_err(|_| invalid_data("META byte array contains invalid hex"))
    })
    .collect()
}

fn add_string(
  data_blocks: &mut Vec<MetaDataBlock>,
  text: &str,
) -> io::Result<u64> {
  let mut bytes = text.as_bytes().to_vec();
  bytes.push(0);
  let block_id = add_data_block(data_blocks, META_STRING_TYPE, bytes)?;
  Ok(block_id as u64)
}

fn add_data_block(
  data_blocks: &mut Vec<MetaDataBlock>,
  structure_name_hash: u32,
  mut data: Vec<u8>,
) -> io::Result<u16> {
  if data_blocks.len() >= 0xfff {
    return Err(invalid_data("META resource exceeds the supported data block count"));
  }
  let id = (data_blocks.len() + 1) as u16;
  let padding = (16 - data.len() % 16) % 16;
  data.resize(data.len() + padding, 0);
  data_blocks.push(MetaDataBlock {
    structure_name_hash,
    data,
  });
  Ok(id)
}

fn add_data_block_raw(
  data_blocks: &mut Vec<MetaDataBlock>,
  structure_name_hash: u32,
  data: Vec<u8>,
) -> io::Result<u16> {
  if data_blocks.len() >= 0xfff {
    return Err(invalid_data("META resource exceeds the supported data block count"));
  }
  let id = (data_blocks.len() + 1) as u16;
  data_blocks.push(MetaDataBlock {
    structure_name_hash,
    data,
  });
  Ok(id)
}

fn copy_at(
  target: &mut [u8],
  offset: usize,
  source: &[u8],
) -> io::Result<()> {
  let end = offset
    .checked_add(source.len())
    .ok_or_else(|| invalid_data("META XML field offset overflows"))?;
  target
    .get_mut(offset..end)
    .ok_or_else(|| invalid_data("META XML field exceeds its structure size"))?
    .copy_from_slice(source);
  Ok(())
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn jenk_hash(value: &str) -> u32 {
  let mut hash = 0u32;
  for byte in value.bytes() {
    hash = hash.wrapping_add(byte as u32);
    hash = hash.wrapping_add(hash << 10);
    hash ^= hash >> 6;
  }
  hash = hash.wrapping_add(hash << 3);
  hash ^= hash >> 11;
  hash.wrapping_add(hash << 15)
}

#[cfg(test)]
mod tests {
  use super::meta_from_xml;
  use crate::core::format::gamefile::{
    meta_resource::{MetaResource, MetaSchemaCatalog},
    resource_file::Rsc7Resource,
  };

  #[test]
  fn rebuilds_occlusion_ymap_meta_from_xml() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary_path = base.join("asset/extracted/brofx_mansion_06___apa_ch2_occl_05.ymap");
    let xml_path = base.join("asset/extracted.xml/brofx_mansion_06___apa_ch2_occl_05.ymap.xml");
    let binary = std::fs::read(binary_path).unwrap();
    let source_resource = Rsc7Resource::decode(&binary).unwrap();
    let source_schema = MetaResource::parse(&source_resource).unwrap();
    let xml = std::fs::read_to_string(xml_path).unwrap();

    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&source_schema);
    let rebuilt = meta_from_xml(&xml, &catalog).unwrap();
    assert_eq!(rebuilt.root_block_index, 1);
    assert!(!rebuilt.data_blocks.is_empty());
    let resource = rebuilt.to_rsc7(2).unwrap();
    let bytes = resource.encode().unwrap();
    let decoded = Rsc7Resource::decode(&bytes).unwrap();
    let reparsed = MetaResource::parse(&decoded).unwrap();
    assert_eq!(reparsed.root_block_index, rebuilt.root_block_index);
    assert_eq!(reparsed.structures, rebuilt.structures);
    assert_eq!(reparsed.enums, rebuilt.enums);
    assert_eq!(reparsed.data_blocks, rebuilt.data_blocks);
  }

  #[test]
  fn rebuilds_legacy_distant_light_position_elements() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let bytes =
      std::fs::read(base.join("asset/vanilla/ymap/vw_distlodlights_medium010.ymap")).unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&meta);
    let xml =
      std::fs::read_to_string(base.join("asset/merged.xml/vw_distlodlights_medium010.ymap.xml"))
        .unwrap();
    let mut root = super::parse_xml(&xml).unwrap();
    let distant =
      root.children.iter_mut().find(|child| child.name == "DistantLODLightsSOA").unwrap();
    let position = distant.children.iter_mut().find(|child| child.name == "position").unwrap();
    let expected = position.children.len();
    assert!(expected > 0);
    for child in &mut position.children {
      child.name = "XmlPositionChildValueAttr".into();
    }
    let mut blocks = Vec::new();
    super::build_structure(
      distant,
      super::jenk_hash("CDistantLODLight"),
      &catalog.structures,
      &catalog.enums,
      &catalog.hash_names,
      &mut blocks,
    )
    .unwrap();
    let positions = blocks
      .iter()
      .find(|block| block.structure_name_hash == super::jenk_hash("FloatXYZ"))
      .expect("distant light positions were discarded");
    let stride = catalog.structures[&super::jenk_hash("FloatXYZ")].structure_size as usize;
    assert_eq!(positions.data.len(), (expected * stride).next_multiple_of(16));
  }

  #[test]
  fn rejects_incomplete_or_unrecognized_distant_light_positions() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let bytes =
      std::fs::read(base.join("asset/vanilla/ymap/vw_distlodlights_medium010.ymap")).unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&meta);
    let xml =
      std::fs::read_to_string(base.join("asset/merged.xml/vw_distlodlights_medium010.ymap.xml"))
        .unwrap();
    for unknown_tag in [false, true] {
      let mut root = super::parse_xml(&xml).unwrap();
      let distant =
        root.children.iter_mut().find(|child| child.name == "DistantLODLightsSOA").unwrap();
      let positions = distant.children.iter_mut().find(|child| child.name == "position").unwrap();
      if unknown_tag {
        positions.children[0].name = "UnknownVector".into();
      } else {
        positions.children.pop();
      }
      let error = super::build_structure(
        distant,
        super::jenk_hash("CDistantLODLight"),
        &catalog.structures,
        &catalog.enums,
        &catalog.hash_names,
        &mut Vec::new(),
      )
      .unwrap_err();
      let expected = if unknown_tag { "unsupported XML item" } else { "counts differ" };
      assert!(error.to_string().contains(expected));
    }
  }
}
