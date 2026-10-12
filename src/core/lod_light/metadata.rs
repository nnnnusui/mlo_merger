use super::{Result, read_map, resource::ResourceFile};
use crate::core::format::{
  gamefile::{
    meta::{MetaResource, jenk_hash},
    rsc7::Rsc7Resource,
  },
  ymap::diff::reference_hash,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "LightSourceMetadata"))]
pub(super) struct LightMetadata {
  pub parent: u32,
  pub has_lod: bool,
  pub has_distant: bool,
}

pub(super) fn read(input: &ResourceFile) -> Result<LightMetadata> {
  let bytes = input.read()?;
  if !bytes.starts_with(b"RSC7") {
    let model = read_map(input)?.model;
    return Ok(LightMetadata {
      parent: reference_hash(&model.parent),
      has_lod: !model.lod_lights.is_empty(),
      has_distant: !model.distant_lod_lights.items.is_empty(),
    });
  }
  let resource = Rsc7Resource::decode(&bytes)?;
  let meta = MetaResource::parse(&resource)?;
  let root =
    meta.data_blocks.get(usize::try_from(meta.root_block_index - 1)?).ok_or("Missing YMAP root")?;
  let schema = meta
    .structures
    .iter()
    .find(|schema| schema.name_hash == root.structure_name_hash)
    .ok_or("Missing YMAP root schema")?;
  let parent = schema
    .entries
    .iter()
    .find(|field| field.name_hash == jenk_hash("parent"))
    .ok_or("Missing YMAP parent field")?;
  let parent = if matches!(parent.data_type, 0x4a | 0x14 | 0x15) {
    u32::from_le_bytes(
      root
        .data
        .get(parent.data_offset as usize..parent.data_offset as usize + 4)
        .ok_or("Truncated YMAP parent")?
        .try_into()?,
    )
  } else {
    reference_hash(&read_map(input)?.model.parent)
  };
  let count = |name: &str, array_name: &str| -> Result<usize> {
    let Some(field) = schema.entries.iter().find(|field| field.name_hash == jenk_hash(name)) else {
      return Ok(0);
    };
    if field.data_type != 0x05 {
      return Err("Unsupported light SOA layout".into());
    }
    let offset = meta
      .structures
      .iter()
      .find(|schema| schema.name_hash == field.reference_key)
      .and_then(|schema| {
        schema.entries.iter().find(|entry| entry.name_hash == jenk_hash(array_name))
      })
      .map(|entry| entry.data_offset)
      .unwrap_or(8) as usize;
    let offset = field.data_offset as usize + offset + 8;
    Ok(u16::from_le_bytes(
      root.data.get(offset..offset + 2).ok_or("Truncated light SOA descriptor")?.try_into()?,
    ) as usize)
  };
  Ok(LightMetadata {
    parent,
    has_lod: count("LODLightsSOA", "direction")? > 0,
    has_distant: count("DistantLODLightsSOA", "position")? > 0,
  })
}
