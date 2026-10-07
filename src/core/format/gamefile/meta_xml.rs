use std::{collections::HashMap, fmt::Write as _, io};

use serde_json::{Map, Number, Value};

use super::meta_resource::{MetaResource, MetaStructureEntry, MetaStructureInfo};
use super::resource_file::Rsc7Resource;

const ARRAY_INFO_HASH: u32 = 0x0000_0100;
const STRUCTURE: u8 = 0x05;
const STRUCTURE_POINTER: u8 = 0x07;
const ARRAY: u8 = 0x52;

const KNOWN_NAMES: &[&str] = &[
  "CMapTypes",
  "CMapData",
  "CEntityDef",
  "CMloInstanceDef",
  "rage__fwContainerLodDef",
  "BoxOccluder",
  "OccludeModel",
  "rage__fwInstancedMapData",
  "CTimeCycleModifier",
  "CCarGen",
  "CLODLight",
  "CDistantLODLight",
  "rage__fwPropInstanceListDef",
  "rage__fwGrassInstanceListDef",
  "rage__fwGrassInstanceListDef__InstanceData",
  "FloatXYZ",
  "x",
  "y",
  "z",
  "POINTER",
  "STRING",
  "HASH",
  "UINT",
  "USHORT",
  "BYTE",
  "FLOAT",
  "VECTOR3",
  "VECTOR4",
  "name",
  "parent",
  "flags",
  "contentFlags",
  "streamingExtentsMin",
  "streamingExtentsMax",
  "entitiesExtentsMin",
  "entitiesExtentsMax",
  "entities",
  "containerLods",
  "boxOccluders",
  "occludeModels",
  "physicsDictionaries",
  "instancedData",
  "timeCycleModifiers",
  "carGenerators",
  "LODLightsSOA",
  "LODLights",
  "DistantLODLightsSOA",
  "block",
  "archetypeName",
  "guid",
  "position",
  "Position",
  "NormalX",
  "NormalY",
  "Color",
  "Scale",
  "Ao",
  "Pad",
  "ScenarioType",
  "IgnoreMaxInRange",
  "NoSpawn",
  "StationaryReactions",
  "OnlySpawnInSameInterior",
  "SpawnedPedIsArrestable",
  "ActivateVehicleSiren",
  "AggressiveVehicleDriving",
  "LandVehicleOnArrival",
  "IgnoreThreatsIfLosNotClear",
  "EventsInRadiusTriggerDisputes",
  "AerialVehiclePoint",
  "TerritorialScenario",
  "EndScenarioIfPlayerWithinRadius",
  "EventsInRadiusTriggerThreatResponse",
  "TaxiPlaneOnGround",
  "FlyOffToOblivion",
  "InWater",
  "AllowInvestigation",
  "OpenDoor",
  "PreciseUseTime",
  "NoRespawnUntilStreamedOut",
  "NoVehicleSpawnMaxDistance",
  "ExtendedRange",
  "ShortRange",
  "HighPriority",
  "IgnoreLoitering",
  "UseSearchlight",
  "ResetNoCollisionOnCleanUp",
  "CheckCrossedArrivalPlane",
  "UseVehicleFrontForArrival",
  "IgnoreWeatherRestrictions",
  "Group",
  "ModelSet",
  "AvailabilityInMpSp",
  "Flags",
  "Radius",
  "TimeTillPedLeaves",
  "iTimeStartOverride",
  "iTimeEndOverride",
  "offsetPosition",
  "enableLimitAngle",
  "startsLocked",
  "canBreak",
  "limitAngle",
  "posn",
  "colour",
  "flashiness",
  "intensity",
  "boneTag",
  "lightType",
  "groupId",
  "timeFlags",
  "cullingPlane",
  "shadowBlur",
  "padding1",
  "padding2",
  "padding3",
  "volIntensity",
  "volSizeScale",
  "volOuterColour",
  "lightHash",
  "volOuterIntensity",
  "coronaSize",
  "volOuterExponent",
  "lightFadeDistance",
  "shadowFadeDistance",
  "specularFadeDistance",
  "volumetricFadeDistance",
  "shadowNearClip",
  "coronaZBias",
  "tangent",
  "coneInnerAngle",
  "extents",
  "projectedTextureKey",
  "doorTargetRatio",
  "audioHash",
  "instances",
  "floorId",
  "defaultEntitySets",
  "numExitPortals",
  "MLOInstflags",
  "rotation",
  "scaleXY",
  "scaleZ",
  "parentIndex",
  "lodDist",
  "childLodDist",
  "lodLevel",
  "numChildren",
  "priorityLevel",
  "extensions",
  "ambientOcclusionMultiplier",
  "artificialAmbientOcclusion",
  "tintValue",
  "iCenterX",
  "iCenterY",
  "iCenterZ",
  "iCosZ",
  "iLength",
  "iWidth",
  "iHeight",
  "iSinZ",
  "bmin",
  "bmax",
  "dataSize",
  "verts",
  "numVertsInBytes",
  "numTris",
  "minExtents",
  "maxExtents",
  "percentage",
  "range",
  "startHour",
  "endHour",
  "orientX",
  "orientY",
  "perpendicularLength",
  "carModel",
  "bodyColorRemap1",
  "bodyColorRemap2",
  "bodyColorRemap3",
  "bodyColorRemap4",
  "popGroup",
  "livery",
  "version",
  "exportedBy",
  "owner",
  "time",
  "direction",
  "falloff",
  "falloffExponent",
  "timeAndStateFlags",
  "hash",
  "coneInnerAngle",
  "coneOuterAngle",
  "coneOuterAngleOrCapExt",
  "coronaIntensity",
  "RGBI",
  "numStreetLights",
  "category",
  "ImapLink",
  "PropInstanceList",
  "GrassInstanceList",
  "BatchAABB",
  "ScaleRange",
  "LodFadeStartDist",
  "LodInstFadeRange",
  "OrientToTerrain",
  "InstanceList",
  "min",
  "max",
  "normalX",
  "normalY",
  "color",
  "scale",
  "ao",
  "pad",
  "LODTYPES_DEPTH_ORPHANHD",
  "LODTYPES_DEPTH_HD",
  "LODTYPES_DEPTH_LOD",
  "LODTYPES_DEPTH_SLOD1",
  "LODTYPES_DEPTH_SLOD2",
  "LODTYPES_DEPTH_SLOD3",
  "LODTYPES_DEPTH_SLOD4",
  "PRI_REQUIRED",
  "PRI_OPTIONAL_HIGH",
  "PRI_OPTIONAL_MEDIUM",
  "PRI_OPTIONAL_LOW",
];

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

/// Serializes a parsed META resource to the schema-driven XML representation.
pub fn meta_to_xml(
  meta: &MetaResource,
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<String> {
  if meta.root_block_index <= 0 {
    return Err(invalid_data("META resource has no root data block"));
  }
  let root_index = meta.root_block_index as usize - 1;
  let root_block = meta
    .data_blocks
    .get(root_index)
    .ok_or_else(|| invalid_data("META root data block is missing"))?;
  let mut names = known_hash_names();
  names.extend(meta.hash_names());
  names.extend(shared_hash_names.iter().map(|(hash, name)| (*hash, name.clone())));

  let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  let root_name = resolve_name(root_block.structure_name_hash, &names);
  write_open_tag(&mut output, 0, &root_name, meta.name.as_deref().map(|name| ("name", name)));
  write_structure(meta, root_index, 0, root_block.structure_name_hash, &names, 1, &mut output)?;
  write_close_tag(&mut output, 0, &root_name);
  Ok(output)
}

/// Returns built-in field, structure, and enum names shared by META and PSO XML.
pub(crate) fn known_hash_names() -> HashMap<u32, String> {
  KNOWN_NAMES.iter().map(|name| (jenk_hash(name), (*name).to_string())).collect()
}

fn write_structure(
  meta: &MetaResource,
  block_index: usize,
  offset: usize,
  structure_hash: u32,
  names: &HashMap<u32, String>,
  depth: usize,
  output: &mut String,
) -> io::Result<()> {
  let block = meta
    .data_blocks
    .get(block_index)
    .ok_or_else(|| invalid_data("META structure references a missing data block"))?;
  let fallback;
  let structure = match meta.structures.iter().find(|s| s.name_hash == structure_hash) {
    Some(structure) => structure,
    None => {
      fallback = fallback_structure(structure_hash).ok_or_else(|| {
        invalid_data(&format!("META structure schema {structure_hash:08X} is missing"))
      })?;
      &fallback
    }
  };
  let mut array_info: Option<&MetaStructureEntry> = None;

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
    let entry_data = slice(&block.data, field_offset, field_size(entry.data_type))?;
    match entry.data_type {
      ARRAY => write_array(meta, array_info, entry_data, &name, names, depth, output)?,
      0x01 => write_value(output, depth, &name, if entry_data[0] == 0 { "false" } else { "true" }),
      0x10 => write_value(output, depth, &name, &(entry_data[0] as i8).to_string()),
      0x11 => write_value(output, depth, &name, &entry_data[0].to_string()),
      0x12 => write_value(output, depth, &name, &i16_at(entry_data, 0)?.to_string()),
      0x13 => write_value(output, depth, &name, &u16_at(entry_data, 0)?.to_string()),
      0x14 => write_value(output, depth, &name, &i32_at(entry_data, 0)?.to_string()),
      0x15 => write_value(output, depth, &name, &u32_at(entry_data, 0)?.to_string()),
      0x21 => write_value(output, depth, &name, &format_float(f32_at(entry_data, 0)?)),
      0x33 => write_vector(output, depth, &name, entry_data, 3)?,
      0x34 => write_vector(output, depth, &name, entry_data, 4)?,
      0x40 => write_text_pair(
        output,
        depth,
        &name,
        &array_of_chars(block, field_offset, entry.reference_key as usize)?,
      ),
      0x44 => write_text_pair(output, depth, &name, &read_string_pointer(meta, entry_data)?),
      0x4a => write_text(output, depth, &name, &resolve_hash(u32_at(entry_data, 0)?, names)),
      0x50 => {
        write_inline_bytes(output, depth, &name, entry, array_info, &block.data, field_offset)?
      }
      0x59 => write_data_block_pointer(meta, output, depth, &name, entry_data)?,
      0x60 => write_value(output, depth, &name, &entry_data[0].to_string()),
      0x62..=0x65 => write_enum_value(meta, output, depth, &name, entry, entry_data, names)?,
      0x05 => {
        write_open_tag(output, depth, &name, None);
        write_structure(
          meta,
          block_index,
          field_offset,
          entry.reference_key,
          names,
          depth + 1,
          output,
        )?;
        write_close_tag(output, depth, &name);
      }
      _ => {
        return Err(invalid_data(&format!(
          "unsupported META field type 0x{:02X} for {name}",
          entry.data_type
        )));
      }
    }
    array_info = None;
  }
  Ok(())
}

fn write_array(
  meta: &MetaResource,
  array_info: Option<&MetaStructureEntry>,
  descriptor: &[u8],
  name: &str,
  names: &HashMap<u32, String>,
  depth: usize,
  output: &mut String,
) -> io::Result<()> {
  let info = array_info
    .ok_or_else(|| invalid_data(&format!("META array {name} has no ARRAYINFO schema")))?;
  let pointer = u64_at(descriptor, 0)?;
  let count = u16_at(descriptor, 8)? as usize;
  let block_id = (pointer & 0xfff) as usize;
  let offset = ((pointer >> 12) & 0xfffff) as usize;
  let type_hash = info.reference_key;
  let item_type = resolve_name(type_hash, names);
  match info.data_type {
    STRUCTURE => {
      let Some(block_index) = block_id.checked_sub(1) else {
        return write_empty_array(output, depth, name, Some(("itemType", &item_type)));
      };
      if count == 0 {
        return write_empty_array(output, depth, name, Some(("itemType", &item_type)));
      }
      write_open_tag(output, depth, name, Some(("itemType", &item_type)));
      let structure =
        meta.structures.iter().find(|structure| structure.name_hash == type_hash).ok_or_else(
          || invalid_data(&format!("META array structure {type_hash:08X} is missing")),
        )?;
      let mut current_block = block_index;
      let mut current_offset = offset;
      for _ in 0..count {
        write_open_tag(output, depth + 1, "Item", None);
        write_structure(meta, current_block, current_offset, type_hash, names, depth + 2, output)?;
        write_close_tag(output, depth + 1, "Item");
        current_offset += structure.structure_size as usize;
        if current_offset >= meta.data_blocks[current_block].data.len() {
          current_offset -= meta.data_blocks[current_block].data.len();
          current_block += 1;
        }
      }
      write_close_tag(output, depth, name);
    }
    STRUCTURE_POINTER => {
      if count == 0 || block_id == 0 {
        return write_empty_array(output, depth, name, None);
      }
      let pointer_block = meta
        .data_blocks
        .get(block_id - 1)
        .ok_or_else(|| invalid_data("META pointer array references a missing block"))?;
      write_open_tag(output, depth, name, None);
      for index in 0..count {
        let pointer_offset = offset + index * 8;
        let pointer_value = u64_at(slice(&pointer_block.data, pointer_offset, 8)?, 0)?;
        let target_block_id = (pointer_value & 0xfff) as usize;
        let target_offset = ((pointer_value >> 12) & 0xfffff) as usize;
        if target_block_id == 0 {
          write_empty_array(output, depth + 1, "Item", None)?;
          continue;
        }
        let target_block = meta
          .data_blocks
          .get(target_block_id - 1)
          .ok_or_else(|| invalid_data("META structure pointer references a missing block"))?;
        let target_type = resolve_name(target_block.structure_name_hash, names);
        write_open_tag(output, depth + 1, "Item", Some(("type", &target_type)));
        write_structure(
          meta,
          target_block_id - 1,
          target_offset,
          target_block.structure_name_hash,
          names,
          depth + 2,
          output,
        )?;
        write_close_tag(output, depth + 1, "Item");
      }
      write_close_tag(output, depth, name);
    }
    0x11 | 0x13 | 0x15 | 0x21 | 0x4a => {
      if count == 0 || block_id == 0 {
        return write_empty_array(output, depth, name, None);
      }
      let data_block = meta
        .data_blocks
        .get(block_id - 1)
        .ok_or_else(|| invalid_data("META primitive array references a missing block"))?;
      let (stride, is_hash) = match info.data_type {
        0x11 => (1, false),
        0x13 => (2, false),
        0x15 | 0x4a | 0x21 => (4, info.data_type == 0x4a),
        _ => unreachable!(),
      };
      if is_hash {
        write_open_tag(output, depth, name, None);
        for index in 0..count {
          let value = u32_at(slice(&data_block.data, offset + index * stride, stride)?, 0)?;
          write_text(output, depth + 1, "Item", &resolve_hash(value, names));
        }
        write_close_tag(output, depth, name);
      } else {
        let values = (0..count)
          .map(|index| {
            let bytes = slice(&data_block.data, offset + index * stride, stride)?;
            Ok(match info.data_type {
              0x11 => bytes[0].to_string(),
              0x13 => u16_at(bytes, 0)?.to_string(),
              0x15 => u32_at(bytes, 0)?.to_string(),
              _ => format_float(f32_at(bytes, 0)?),
            })
          })
          .collect::<io::Result<Vec<_>>>()?;
        write_text(output, depth, name, &values.join(" "));
      }
    }
    _ => {
      return Err(invalid_data(&format!(
        "unsupported META array element type 0x{:02X} for {name}",
        info.data_type
      )));
    }
  }
  Ok(())
}

fn write_inline_bytes(
  output: &mut String,
  depth: usize,
  name: &str,
  entry: &MetaStructureEntry,
  array_info: Option<&MetaStructureEntry>,
  data: &[u8],
  offset: usize,
) -> io::Result<()> {
  let info =
    array_info.ok_or_else(|| invalid_data("META inline byte array is missing ARRAYINFO"))?;
  let count = entry.reference_key as usize;
  let stride = match info.data_type {
    0x12 | 0x13 => 2,
    0x14 | 0x15 | 0x21 => 4,
    _ => 1,
  };
  let bytes = slice(data, offset, count.saturating_mul(stride))?;
  let text = match info.data_type {
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
    _ => bytes.iter().map(|byte| format!("{byte:02X}")).collect::<String>(),
  };
  write_text(output, depth, name, &text);
  Ok(())
}

fn write_data_block_pointer(
  meta: &MetaResource,
  output: &mut String,
  depth: usize,
  name: &str,
  bytes: &[u8],
) -> io::Result<()> {
  let pointer = u64_at(bytes, 0)?;
  let block_id = (pointer & 0xfff) as usize;
  if block_id == 0 {
    return write_empty_array(output, depth, name, None);
  }
  let block = meta
    .data_blocks
    .get(block_id - 1)
    .ok_or_else(|| invalid_data("META data pointer references a missing block"))?;
  let mut lines = Vec::new();
  for chunk in block.data.chunks(32) {
    lines.push(chunk.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" "));
  }
  write_text(output, depth, name, &lines.join("\n"));
  Ok(())
}

fn write_enum_value(
  meta: &MetaResource,
  output: &mut String,
  depth: usize,
  name: &str,
  entry: &MetaStructureEntry,
  bytes: &[u8],
  names: &HashMap<u32, String>,
) -> io::Result<()> {
  let enum_info = meta.enums.iter().find(|info| info.name_hash == entry.reference_key);
  let raw = if entry.data_type == 0x64 { i16_at(bytes, 0)? as i32 } else { i32_at(bytes, 0)? };
  let value = if matches!(entry.data_type, 0x63 | 0x65) {
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
  };
  write_text(output, depth, name, &value);
  Ok(())
}

fn read_string_pointer(
  meta: &MetaResource,
  bytes: &[u8],
) -> io::Result<String> {
  let pointer = u64_at(bytes, 0)?;
  let block_id = (pointer & 0xfff) as usize;
  if block_id == 0 {
    return Ok(String::new());
  }
  let offset = ((pointer >> 12) & 0xfffff) as usize;
  let count = u16_at(bytes, 8)? as usize;
  let block = meta
    .data_blocks
    .get(block_id - 1)
    .ok_or_else(|| invalid_data("META string pointer references a missing block"))?;
  Ok(String::from_utf8_lossy(slice(&block.data, offset, count)?).trim_end_matches('\0').to_string())
}

fn array_of_chars(
  block: &super::meta_resource::MetaDataBlock,
  offset: usize,
  count: usize,
) -> io::Result<String> {
  let bytes = slice(&block.data, offset, count)?;
  Ok(String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string())
}

fn field_size(data_type: u8) -> usize {
  match data_type {
    0x01 | 0x10 | 0x11 => 1,
    0x12 | 0x13 | 0x60 | 0x64 => 2,
    0x14 | 0x15 | 0x21 | 0x4a | 0x62 | 0x63 | 0x65 => 4,
    0x07 | 0x34 | 0x44 | 0x52 => 16,
    0x33 => 12,
    0x05 | 0x40 | 0x50 => 0,
    0x59 => 8,
    _ => 0,
  }
}

fn write_vector(
  output: &mut String,
  depth: usize,
  name: &str,
  bytes: &[u8],
  count: usize,
) -> io::Result<()> {
  let components = ["x", "y", "z", "w"];
  let mut attributes = Vec::new();
  for (index, component) in components.iter().take(count).enumerate() {
    attributes.push((*component, format_float(f32_at(bytes, index * 4)?)));
  }
  write_indent(output, depth);
  write!(output, "<{name}").unwrap();
  for (key, value) in attributes {
    write!(output, " {key}=\"{}\"", escape_attr(&value)).unwrap();
  }
  output.push_str(" />\n");
  Ok(())
}

fn write_value(
  output: &mut String,
  depth: usize,
  name: &str,
  value: &str,
) {
  write_indent(output, depth);
  writeln!(output, "<{name} value=\"{}\" />", escape_attr(value)).unwrap();
}

fn write_text(
  output: &mut String,
  depth: usize,
  name: &str,
  text: &str,
) {
  if text.is_empty() {
    write_indent(output, depth);
    writeln!(output, "<{name} />").unwrap();
  } else {
    write_indent(output, depth);
    write!(output, "<{name}>").unwrap();
    super::xml_tree::write_text_content(output, depth, text);
    writeln!(output, "</{name}>").unwrap();
  }
}

fn write_text_pair(
  output: &mut String,
  depth: usize,
  name: &str,
  text: &str,
) {
  write_indent(output, depth);
  write!(output, "<{name}>").unwrap();
  super::xml_tree::write_text_content(output, depth, text);
  writeln!(output, "</{name}>").unwrap();
}

fn write_empty_array(
  output: &mut String,
  depth: usize,
  name: &str,
  attribute: Option<(&str, &str)>,
) -> io::Result<()> {
  write_indent(output, depth);
  write!(output, "<{name}").unwrap();
  if let Some((key, value)) = attribute {
    write!(output, " {key}=\"{}\"", escape_attr(value)).unwrap();
  }
  output.push_str(" />\n");
  Ok(())
}

fn write_open_tag(
  output: &mut String,
  depth: usize,
  name: &str,
  attribute: Option<(&str, &str)>,
) {
  write_indent(output, depth);
  write!(output, "<{name}").unwrap();
  if let Some((key, value)) = attribute {
    write!(output, " {key}=\"{}\"", escape_attr(value)).unwrap();
  }
  output.push_str(">\n");
}

fn write_close_tag(
  output: &mut String,
  depth: usize,
  name: &str,
) {
  write_indent(output, depth);
  writeln!(output, "</{name}>").unwrap();
}

fn write_indent(
  output: &mut String,
  depth: usize,
) {
  output.extend(std::iter::repeat_n(' ', depth));
}

fn escape_attr(value: &str) -> String {
  value.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}

fn resolve_hash(
  hash: u32,
  names: &HashMap<u32, String>,
) -> String {
  if hash == 0 { String::new() } else { resolve_name(hash, names) }
}

fn resolve_name(
  hash: u32,
  names: &HashMap<u32, String>,
) -> String {
  names.get(&hash).cloned().unwrap_or_else(|| format!("hash_{hash:08X}"))
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

fn format_float(value: f32) -> String {
  if value == 0.0 && value.is_sign_positive() { "0".to_string() } else { value.to_string() }
}

fn slice(
  bytes: &[u8],
  offset: usize,
  length: usize,
) -> io::Result<&[u8]> {
  let end = offset.checked_add(length).ok_or_else(|| invalid_data("META data range overflows"))?;
  bytes.get(offset..end).ok_or_else(|| invalid_data("META data range is out of bounds"))
}

fn u16_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u16> {
  Ok(u16::from_le_bytes(slice(bytes, offset, 2)?.try_into().unwrap()))
}

fn i16_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i16> {
  Ok(i16::from_le_bytes(slice(bytes, offset, 2)?.try_into().unwrap()))
}

fn u32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_le_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

fn i32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i32> {
  Ok(i32::from_le_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

fn u64_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u64> {
  Ok(u64::from_le_bytes(slice(bytes, offset, 8)?.try_into().unwrap()))
}

fn f32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<f32> {
  Ok(f32::from_le_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn fallback_structure(hash: u32) -> Option<MetaStructureInfo> {
  let hash_name = |name: &str| jenk_hash(name);
  let entry = |name: &str, offset, data_type, reference_key| MetaStructureEntry {
    name_hash: hash_name(name),
    data_offset: offset,
    data_type,
    unknown: 0,
    reference_type_index: 0,
    reference_key,
  };
  let array_info = |data_type, reference_key| MetaStructureEntry {
    name_hash: ARRAY_INFO_HASH,
    data_offset: 0,
    data_type,
    unknown: 0,
    reference_type_index: 0,
    reference_key,
  };
  let structure = |name: &str, size, entries| MetaStructureInfo {
    name_hash: hash_name(name),
    structure_key: 0,
    unknown_8: 0,
    unknown_12: 0,
    unknown_28: 0,
    structure_size: size,
    entries,
  };

  match hash {
    value if value == hash_name("rage__fwInstancedMapData") => Some(structure(
      "rage__fwInstancedMapData",
      48,
      vec![
        entry("ImapLink", 8, 0x4a, 0),
        array_info(STRUCTURE, hash_name("rage__fwPropInstanceListDef")),
        entry("PropInstanceList", 16, ARRAY, 0),
        array_info(STRUCTURE, hash_name("rage__fwGrassInstanceListDef")),
        entry("GrassInstanceList", 32, ARRAY, 0),
      ],
    )),
    value if value == hash_name("rage__spdAABB") => Some(structure(
      "rage__spdAABB",
      32,
      vec![entry("min", 0, 0x34, 0), entry("max", 16, 0x34, 0)],
    )),
    value if value == hash_name("rage__fwGrassInstanceListDef") => Some(structure(
      "rage__fwGrassInstanceListDef",
      96,
      vec![
        entry("BatchAABB", 0, 0x05, hash_name("rage__spdAABB")),
        entry("ScaleRange", 32, 0x33, 0),
        entry("archetypeName", 48, 0x4a, 0),
        entry("lodDist", 52, 0x15, 0),
        entry("LodFadeStartDist", 56, 0x21, 0),
        entry("LodInstFadeRange", 60, 0x21, 0),
        entry("OrientToTerrain", 64, 0x21, 0),
        array_info(STRUCTURE, hash_name("rage__fwGrassInstanceListDef__InstanceData")),
        entry("InstanceList", 72, ARRAY, 0),
      ],
    )),
    value if value == hash_name("rage__fwGrassInstanceListDef__InstanceData") => Some(structure(
      "rage__fwGrassInstanceListDef__InstanceData",
      16,
      vec![
        array_info(0x13, 0),
        entry("Position", 0, 0x50, 3),
        entry("NormalX", 6, 0x11, 0),
        entry("NormalY", 7, 0x11, 0),
        array_info(0x11, 0),
        entry("Color", 8, 0x50, 4),
        entry("Scale", 11, 0x11, 0),
        entry("Ao", 12, 0x11, 0),
        array_info(0x11, 0),
        entry("Pad", 13, 0x50, 8),
      ],
    )),
    value if value == hash_name("FloatXYZ") => Some(structure(
      "FloatXYZ",
      12,
      vec![entry("x", 0, 0x21, 0), entry("y", 4, 0x21, 0), entry("z", 8, 0x21, 0)],
    )),
    value if value == hash_name("CLODLight") => Some(structure(
      "CLODLight",
      136,
      vec![
        array_info(STRUCTURE, hash_name("FloatXYZ")),
        entry("direction", 8, ARRAY, 0),
        array_info(0x21, 0),
        entry("falloff", 24, ARRAY, 0),
        array_info(0x21, 0),
        entry("falloffExponent", 40, ARRAY, 0),
        array_info(0x15, 0),
        entry("timeAndStateFlags", 56, ARRAY, 0),
        array_info(0x15, 0),
        entry("hash", 72, ARRAY, 0),
        array_info(0x11, 0),
        entry("coneInnerAngle", 88, ARRAY, 0),
        array_info(0x11, 0),
        entry("coneOuterAngleOrCapExt", 104, ARRAY, 0),
        array_info(0x11, 0),
        entry("coronaIntensity", 120, ARRAY, 0),
      ],
    )),
    value if value == hash_name("CDistantLODLight") => Some(structure(
      "CDistantLODLight",
      48,
      vec![
        array_info(STRUCTURE, hash_name("FloatXYZ")),
        entry("position", 8, ARRAY, 0),
        array_info(0x15, 0),
        entry("RGBI", 24, ARRAY, 0),
        entry("numStreetLights", 40, 0x13, 0),
        entry("category", 42, 0x13, 0),
      ],
    )),
    value if value == hash_name("CLightAttrDef") => Some(structure(
      "CLightAttrDef",
      160,
      vec![
        array_info(0x21, 0),
        entry("posn", 8, 0x50, 3),
        array_info(0x11, 0),
        entry("colour", 20, 0x50, 3),
        entry("flashiness", 23, 0x11, 0),
        entry("intensity", 24, 0x21, 0),
        entry("flags", 28, 0x15, 0),
        entry("boneTag", 32, 0x12, 0),
        entry("lightType", 34, 0x11, 0),
        entry("groupId", 35, 0x11, 0),
        entry("timeFlags", 36, 0x15, 0),
        entry("falloff", 40, 0x21, 0),
        entry("falloffExponent", 44, 0x21, 0),
        array_info(0x21, 0),
        entry("cullingPlane", 48, 0x50, 4),
        entry("shadowBlur", 64, 0x11, 0),
        entry("padding1", 65, 0x11, 0),
        entry("padding2", 66, 0x12, 0),
        entry("padding3", 68, 0x15, 0),
        entry("volIntensity", 72, 0x21, 0),
        entry("volSizeScale", 76, 0x21, 0),
        array_info(0x11, 0),
        entry("volOuterColour", 80, 0x50, 3),
        entry("lightHash", 83, 0x11, 0),
        entry("volOuterIntensity", 84, 0x21, 0),
        entry("coronaSize", 88, 0x21, 0),
        entry("volOuterExponent", 92, 0x21, 0),
        entry("lightFadeDistance", 96, 0x11, 0),
        entry("shadowFadeDistance", 97, 0x11, 0),
        entry("specularFadeDistance", 98, 0x11, 0),
        entry("volumetricFadeDistance", 99, 0x11, 0),
        entry("shadowNearClip", 100, 0x21, 0),
        entry("coronaIntensity", 104, 0x21, 0),
        entry("coronaZBias", 108, 0x21, 0),
        array_info(0x21, 0),
        entry("direction", 112, 0x50, 3),
        array_info(0x21, 0),
        entry("tangent", 124, 0x50, 3),
        entry("coneInnerAngle", 136, 0x21, 0),
        entry("coneOuterAngle", 140, 0x21, 0),
        array_info(0x21, 0),
        entry("extents", 144, 0x50, 3),
        entry("projectedTextureKey", 156, 0x15, 0),
      ],
    )),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use super::{meta_to_xml, ymap_to_xml};
  use crate::core::format::gamefile::{
    meta_resource::{MetaDataBlock, MetaResource, MetaStructureEntry, MetaStructureInfo},
    resource_file::Rsc7Resource,
    test_support::{sample_ymap_binary, sample_ymap_xml},
  };

  #[test]
  fn decodes_ymap_binary_directly_into_the_typed_model() {
    let entry = |name: &str, offset, data_type, reference_key| MetaStructureEntry {
      name_hash: super::jenk_hash(name),
      data_offset: offset,
      data_type,
      unknown: 0,
      reference_type_index: 0,
      reference_key,
    };
    let block_hash = super::jenk_hash("CMapDataBlock");
    let root_hash = super::jenk_hash("CMapData");
    let structures = vec![
      MetaStructureInfo {
        name_hash: root_hash,
        structure_key: 0,
        unknown_8: 0,
        unknown_12: 0,
        unknown_28: 0,
        structure_size: 104,
        entries: vec![
          entry("name", 0, 0x4a, 0),
          entry("parent", 4, 0x4a, 0),
          entry("flags", 8, 0x15, 0),
          entry("contentFlags", 12, 0x15, 0),
          entry("streamingExtentsMin", 16, 0x33, 0),
          entry("streamingExtentsMax", 28, 0x33, 0),
          entry("entitiesExtentsMin", 40, 0x33, 0),
          entry("entitiesExtentsMax", 52, 0x33, 0),
          MetaStructureEntry {
            name_hash: 0x100,
            data_offset: 0,
            data_type: 0x05,
            unknown: 0,
            reference_type_index: 0,
            reference_key: super::jenk_hash("CEntityDef"),
          },
          entry("entities", 64, 0x52, 0),
          entry("block", 80, 0x05, block_hash),
        ],
      },
      MetaStructureInfo {
        name_hash: block_hash,
        structure_key: 0,
        unknown_8: 0,
        unknown_12: 0,
        unknown_28: 0,
        structure_size: 24,
        entries: vec![
          entry("version", 0, 0x15, 0),
          entry("flags", 4, 0x15, 0),
          entry("name", 8, 0x4a, 0),
          entry("exportedBy", 12, 0x4a, 0),
          entry("owner", 16, 0x4a, 0),
          entry("time", 20, 0x4a, 0),
        ],
      },
    ];
    let mut data = vec![0; 104];
    data[8..12].copy_from_slice(&7u32.to_le_bytes());
    data[12..16].copy_from_slice(&11u32.to_le_bytes());
    let meta = MetaResource {
      root_block_index: 1,
      structures,
      enums: vec![],
      data_blocks: vec![MetaDataBlock {
        structure_name_hash: root_hash,
        data,
      }],
      name: Some("fixture".into()),
    };
    let bytes = meta.to_rsc7(2).unwrap().encode().unwrap();

    let model = super::ymap_to_model(&bytes, &HashMap::new()).unwrap();
    assert_eq!(model.name, "");
    assert_eq!(model.flags, 7);
    assert_eq!(model.content_flags, 11);
    assert!(model.entity_map.is_empty());
    assert_eq!(model.block.name, "");
    let xml = super::ymap_to_xml(&bytes, &HashMap::new()).unwrap();
    let xml_model: crate::core::format::ymap::model::Ymap =
      quick_xml::de::from_str::<crate::core::format::ymap::xml::XmlYmap>(&xml).unwrap().into();
    assert_eq!(model, xml_model);
  }

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

  #[test]
  fn serializes_sample_occlusion_ymap_using_embedded_schema() {
    let xml = sample_ymap_xml("occlusion/occlusion.ymap.xml");
    let bytes = sample_ymap_binary(&xml);
    let xml = ymap_to_xml(&bytes, &HashMap::new()).unwrap();

    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>"));
    assert!(xml.contains("<boxOccluders itemType=\"BoxOccluder\">"));
    assert!(xml.contains("<iCenterX value=\"-1567\" />"));
    assert!(xml.contains("<occludeModels"));
  }

  #[test]
  fn lod_light_hash_array_uses_its_canonical_name() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/source/isacb_mechanic_littlesoul/stream/metadata/mek_lodlights_lodlights.ymap");
    let bytes = std::fs::read(fixture).unwrap();
    let resource = Rsc7Resource::decode(&bytes).unwrap();
    let meta = MetaResource::parse(&resource).unwrap();
    let xml = meta_to_xml(&meta, &HashMap::new()).unwrap();
    assert!(xml.contains("<hash>"));
    assert!(!xml.contains("hash_0000004A"));
    let parsed: crate::core::format::ymap::xml::XmlYmap = quick_xml::de::from_str(&xml).unwrap();
    let model: crate::core::format::ymap::model::Ymap = parsed.into();
    assert_eq!(model.lod_lights.len(), 184);
    assert_eq!(model.lod_lights[0].hash, "1571135");
  }

  #[test]
  fn serializes_ytyp_rsc_meta_using_the_shared_meta_writer() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp");
    let bytes = std::fs::read(fixture).unwrap();
    let resource = Rsc7Resource::decode(&bytes).unwrap();
    let meta = MetaResource::parse(&resource).unwrap();
    let xml = meta_to_xml(&meta, &meta.hash_names()).unwrap();

    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<CMapTypes"));
  }

  #[test]
  fn serializes_all_checked_in_ymap_resources() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
    let mut converted = 0;

    for entry in std::fs::read_dir(fixtures).unwrap() {
      let path = entry.unwrap().path();
      if path.extension().is_some_and(|extension| extension == "ymap") {
        let bytes = std::fs::read(&path).unwrap();
        ymap_to_xml(&bytes, &HashMap::new()).unwrap_or_else(|error| {
          panic!("failed to serialize {}: {error}", path.display());
        });
        converted += 1;
      }
    }

    assert!(converted > 0);
  }
}
