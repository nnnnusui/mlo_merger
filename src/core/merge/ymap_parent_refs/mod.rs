use std::{
  collections::HashMap,
  io,
  path::{Path, PathBuf},
  rc::Rc,
};

use crate::core::format::ymap::reference_hash;
use crate::core::format::ymap::{
  model::{Ymap, YmapEntity},
  relation::{is_local_parent, runtime_entity_indices},
  xml::XmlYmap,
};

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

mod clone;
mod references;
mod sources;

pub(super) use crate::core::format::ymap::relation::runtime_entities;
pub(super) use clone::patch_clone;
pub(super) use references::ParentReferences;
pub(super) use sources::{OriginalMap, SourceMaps};

#[cfg(test)]
mod tests;
