use std::{
  collections::HashMap,
  io,
  path::{Path, PathBuf},
  rc::Rc,
};

use crate::core::format::ymap::diff::reference_hash;
use crate::core::format::ymap::{
  model::{Ymap, YmapEntity},
  xml::XmlYmap,
};

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

mod clone;
mod order;
mod references;
mod sources;

pub(super) use clone::patch_clone;
use order::{is_local_parent, runtime_entity_indices};
pub(super) use order::{runtime_entities, valid_local_pair};
pub(super) use references::ParentReferences;
pub(super) use sources::{OriginalMap, SourceMaps};

#[cfg(test)]
mod tests;
