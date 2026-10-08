use std::{collections::HashMap, io};

use super::{
  meta_resource::jenk_hash,
  meta_xml::known_hash_names,
  xml_tree::{XmlElement, parse_xml},
};

#[derive(Debug, Clone)]
struct Field {
  name: u32,
  kind: u8,
  subtype: u8,
  offset: usize,
  reference: u32,
}

#[derive(Debug, Clone)]
struct Schema {
  size: usize,
  fields: Vec<Field>,
}

#[derive(Debug)]
struct Block {
  name: u32,
  offset: usize,
  length: usize,
}

/// A big-endian PSO container with embedded schemas and data mappings.
///
/// # Examples
///
/// ```no_run
/// use mlo_merger::core::format::gamefile::pso::PsoResource;
/// let bytes = std::fs::read("asset/vanilla/ymap/id2_17.ymap")?;
/// let resource = PsoResource::parse(&bytes)?;
/// let xml = resource.to_xml(&Default::default())?;
/// let rebuilt = resource.rebuild_xml(&xml)?;
/// assert!(rebuilt.starts_with(b"PSIN"));
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug)]
pub struct PsoResource {
  data: Vec<u8>,
  root: usize,
  blocks: Vec<Block>,
  structures: HashMap<u32, Schema>,
  enums: HashMap<u32, Vec<(u32, i32)>>,
  sections: Vec<([u8; 4], Vec<u8>)>,
}

mod helpers;
mod patch;
mod read;
mod xml;

use helpers::*;

#[cfg(test)]
mod tests;
