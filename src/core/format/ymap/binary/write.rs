//! Public YMAP META and compressed RSC7 encoding entry points.

use super::super::model::{Ymap, YmapEntity};
use super::{meta::Writer, model::prepare};
use crate::core::format::gamefile::meta_resource::{
  MetaDataBlock, MetaResource, MetaSchemaCatalog, jenk_hash,
};
use std::io;

/// Builds native META blocks directly from a YMAP model, preserving the supplied entity order.
///
/// Schema layouts must come from compatible vanilla YMAP resources.
///
/// ```no_run
/// # use mlo_merger::core::format::{ymap::{binary::to_meta, model::Ymap}, gamefile::meta_resource::MetaSchemaCatalog};
/// # fn build(model: &Ymap, catalog: &MetaSchemaCatalog) -> std::io::Result<()> {
/// let entities = model.entity_map.values().cloned().collect::<Vec<_>>();
/// let meta = to_meta(model, &entities, catalog)?;
/// # Ok(()) }
/// ```
pub fn to_meta(
  model: &Ymap,
  entities: &[YmapEntity],
  catalog: &MetaSchemaCatalog,
) -> io::Result<MetaResource> {
  let value = prepare(model, entities)?;
  let root = jenk_hash("CMapData");
  let mut writer = Writer {
    catalog,
    blocks: vec![MetaDataBlock {
      structure_name_hash: root,
      data: Vec::new(),
    }],
  };
  writer.blocks[0].data = writer.structure(root, &value)?;
  let mut structures = catalog.structures.values().cloned().collect::<Vec<_>>();
  structures.sort_by_key(|schema| schema.name_hash);
  let mut enums = catalog.enums.values().cloned().collect::<Vec<_>>();
  enums.sort_by_key(|schema| schema.name_hash);
  Ok(MetaResource {
    root_block_index: 1,
    structures,
    enums,
    data_blocks: writer.blocks,
    name: None,
  })
}

/// Encodes a typed YMAP and ordered entities directly into a compressed RSC7 binary.
///
/// ```no_run
/// # use mlo_merger::core::format::{ymap::{binary::write_ymap, model::Ymap}, gamefile::meta_resource::MetaSchemaCatalog};
/// # fn save(model: &Ymap, catalog: &MetaSchemaCatalog) -> std::io::Result<()> {
/// let entities = model.entity_map.values().cloned().collect::<Vec<_>>();
/// std::fs::write("map.ymap", write_ymap(model, &entities, catalog)?)?;
/// # Ok(()) }
/// ```
pub fn write_ymap(
  model: &Ymap,
  entities: &[YmapEntity],
  catalog: &MetaSchemaCatalog,
) -> io::Result<Vec<u8>> {
  to_meta(model, entities, catalog)?.to_rsc7(2)?.encode()
}
