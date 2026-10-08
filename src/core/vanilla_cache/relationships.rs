use super::*;

pub(super) fn children_by_parent_hash(
  parent_by_child: &BTreeMap<String, u32>
) -> BTreeMap<String, Vec<String>> {
  let mut children = BTreeMap::new();
  for (child, parent_hash) in parent_by_child {
    if *parent_hash != 0 {
      children.entry(format!("{parent_hash:08x}")).or_insert_with(Vec::new).push(child.clone());
    }
  }
  children
}

pub(crate) fn ymap_parent_hash(path: &Path) -> Result<u32> {
  let bytes = fs::read(path)?;
  if !bytes.starts_with(b"RSC7") {
    let ymap = read_ymap(path)?;
    return Ok(reference_hash(&ymap.parent));
  }
  let resource = Rsc7Resource::decode(&bytes)?;
  let meta = MetaResource::parse(&resource)?;
  let root_index = usize::try_from(meta.root_block_index - 1)?;
  let root = meta.data_blocks.get(root_index).ok_or("YMAP META root block is missing")?;
  let schema = meta
    .structures
    .iter()
    .find(|schema| schema.name_hash == root.structure_name_hash)
    .ok_or("YMAP META root schema is missing")?;
  let parent_name_hash = jenk_hash("parent");
  let field = schema
    .entries
    .iter()
    .find(|field| field.name_hash == parent_name_hash)
    .ok_or("YMAP META root has no parent field")?;
  let offset = field.data_offset as usize;
  match field.data_type {
    0x4a | 0x14 | 0x15 => {
      let bytes = root.data.get(offset..offset + 4).ok_or("YMAP parent field is truncated")?;
      Ok(u32::from_le_bytes(bytes.try_into()?))
    }
    0x44 => {
      let pointer = root.data.get(offset..offset + 10).ok_or("YMAP parent pointer is truncated")?;
      let address = u64::from_le_bytes(pointer[..8].try_into()?);
      let block_id = (address & 0xfff) as usize;
      if block_id == 0 {
        return Ok(0);
      }
      let string_offset = ((address >> 12) & 0xfffff) as usize;
      let length = u16::from_le_bytes(pointer[8..10].try_into()?) as usize;
      let block = meta
        .data_blocks
        .get(block_id - 1)
        .ok_or("YMAP parent string points to a missing META block")?;
      let bytes = block
        .data
        .get(string_offset..string_offset + length)
        .ok_or("YMAP parent string is truncated")?;
      let parent = String::from_utf8_lossy(bytes).trim_end_matches('\0').to_owned();
      Ok(reference_hash(&parent))
    }
    0x40 => {
      let length = field.reference_key as usize;
      let bytes = root.data.get(offset..offset + length).ok_or("YMAP parent text is truncated")?;
      let parent = String::from_utf8_lossy(bytes).trim_end_matches('\0').to_owned();
      Ok(reference_hash(&parent))
    }
    data_type => Err(format!("Unsupported YMAP parent field type 0x{data_type:02x}").into()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::gamefile::meta_resource::{
    MetaDataBlock, MetaStructureEntry, MetaStructureInfo,
  };

  #[test]
  fn relationship_index_groups_children_by_parent_and_omits_root_maps() {
    let parent_hash = reference_hash("hei_ch1_lod");
    let children = children_by_parent_hash(&BTreeMap::from([
      ("hei_ch1_11.ymap".into(), parent_hash),
      ("root.ymap".into(), 0),
      ("second_child.ymap".into(), parent_hash),
    ]));
    assert_eq!(children[&format!("{parent_hash:08x}")], ["hei_ch1_11.ymap", "second_child.ymap"]);
    assert_eq!(children.len(), 1);
  }
  #[test]
  fn reads_parent_hash_directly_from_rsc7_meta_root() {
    let expected = reference_hash("hei_ch1_lod");
    let root_name_hash = jenk_hash("CMapData");
    let meta = MetaResource {
      root_block_index: 1,
      structures: vec![MetaStructureInfo {
        name_hash: root_name_hash,
        structure_key: 0,
        unknown_8: 0,
        unknown_12: 0,
        unknown_28: 0,
        structure_size: 4,
        entries: vec![MetaStructureEntry {
          name_hash: jenk_hash("parent"),
          data_offset: 0,
          data_type: 0x4a,
          unknown: 0,
          reference_type_index: -1,
          reference_key: 0,
        }],
      }],
      enums: vec![],
      data_blocks: vec![MetaDataBlock {
        structure_name_hash: root_name_hash,
        data: expected.to_le_bytes().to_vec(),
      }],
      name: None,
    };
    let path = std::env::temp_dir().join(format!("direct_parent_{}.ymap", std::process::id()));
    fs::write(&path, meta.to_rsc7(2).unwrap().encode().unwrap()).unwrap();
    assert_eq!(ymap_parent_hash(&path).unwrap(), expected);
    fs::remove_file(path).unwrap();
  }
}
