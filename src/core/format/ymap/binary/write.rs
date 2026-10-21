//! Public YMAP META and compressed RSC7 encoding entry points.

use super::super::model::{Ymap, YmapEntity};
use super::{encode::Root, schemas::schemas};
use crate::core::format::gamefile::{
  meta::{MetaResource, jenk_hash},
  struct_writer::StructWriter,
};
use std::io;

/// Builds native META blocks directly from a YMAP model, preserving the supplied entity order.
///
/// Every YMAP is written with the shared YMAP schemas (see `schemas`), embedding those its
/// blocks use.
///
/// ```no_run
/// # use mlo_merger::core::format::ymap::{binary::to_meta, model::Ymap};
/// # fn build(model: &Ymap) -> std::io::Result<()> {
/// let meta = to_meta(model, &model.entities)?;
/// # Ok(()) }
/// ```
pub fn to_meta(
  model: &Ymap,
  entities: &[YmapEntity],
) -> io::Result<MetaResource> {
  let written = StructWriter::new(schemas()).root(
    jenk_hash("CMapData"),
    &Root {
      model,
      entities,
    },
  )?;
  Ok(MetaResource {
    root_block_index: 1,
    structures: written.structures,
    enums: written.enums,
    data_blocks: written.blocks,
    name: None,
  })
}

/// Encodes a typed YMAP and ordered entities directly into a compressed RSC7 binary.
///
/// ```no_run
/// # use mlo_merger::core::format::ymap::{binary::write_ymap, model::Ymap};
/// # fn save(model: &Ymap) -> std::io::Result<()> {
/// std::fs::write("map.ymap", write_ymap(model, &model.entities)?)?;
/// # Ok(()) }
/// ```
pub fn write_ymap(
  model: &Ymap,
  entities: &[YmapEntity],
) -> io::Result<Vec<u8>> {
  to_meta(model, entities)?.to_rsc7(2)?.encode()
}
