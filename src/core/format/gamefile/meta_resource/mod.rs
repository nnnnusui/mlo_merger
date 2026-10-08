use std::io;

use super::resource_file::Rsc7Resource;

const META_ROOT_ADDRESS: u64 = 0x5000_0000;
const META_ROOT_SIZE: usize = 0x80;
const MAX_STRING_LENGTH: usize = 1 << 16;
const META_STRING_TYPE: u32 = 0x10;

mod helpers;
mod model;
mod read;
mod write;

use helpers::invalid_data;
pub(crate) use helpers::jenk_hash;
pub use model::{
  MetaDataBlock, MetaEnumEntry, MetaEnumInfo, MetaResource, MetaSchemaCatalog, MetaStructureEntry,
  MetaStructureInfo,
};

#[cfg(test)]
mod tests;
