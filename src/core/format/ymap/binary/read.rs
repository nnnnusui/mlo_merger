use std::{collections::HashMap, io};

use crate::core::format::gamefile::{
  meta::MetaResource, pso::PsoResource, resource::Resource, rsc7::Rsc7Resource,
};
use crate::core::format::ymap::model::Ymap;

pub(super) fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Converts a binary RSC7 or PSO YMAP into its typed model.
///
/// RSC7 decodes its META graph; PSO decodes its embedded schemas.
pub fn ymap_to_model(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<Ymap> {
  match Resource::decode(bytes)? {
    Resource::Pso(resource) => pso_to_ymap(&resource, shared_hash_names),
    Resource::Rsc7(resource) => rsc7_to_ymap(&resource, shared_hash_names),
  }
}

/// Converts a decoded RSC7 META YMAP into its typed model.
fn rsc7_to_ymap(
  resource: &Rsc7Resource,
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<Ymap> {
  meta_to_ymap(&MetaResource::parse(resource)?, shared_hash_names)
}

/// Converts a decoded PSO YMAP into its typed model using its embedded schemas.
fn pso_to_ymap(
  resource: &PsoResource,
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<Ymap> {
  let names = resource.names(shared_hash_names);
  super::decode::ymap_from_root(&resource.root_struct(&names)?)
}

fn meta_to_ymap(
  meta: &MetaResource,
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<Ymap> {
  let names = meta.names(shared_hash_names);
  super::decode::ymap_from_root(&meta.root_struct(&names)?)
}

#[cfg(test)]
#[path = "pso_test.rs"]
mod pso_test;
