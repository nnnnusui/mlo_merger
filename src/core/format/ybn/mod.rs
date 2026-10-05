//! Structured YBN collision models, codecs and polygon differences.

use crate::core::format::gamefile::{
  resource_file::Rsc7Resource,
  xml_tree::{XmlElement, parse_xml, write_text_content},
};
use std::{
  collections::{HashMap, HashSet},
  io,
};

mod binary;
pub mod diff;
pub mod model;
pub mod xml;

use binary::*;
#[cfg(test)]
use diff::legacy::*;
use model::{Bound, Geometry, Material, Polygon, Triangle};
use xml::helpers::*;
#[cfg(test)]
use xml::{read_bound_xml, write_bound_xml};

pub use binary::read_ybn_root as read_ybn;
pub use diff::merge_ybn_deltas;
pub use xml::{xml_to_ybn, ybn_to_xml};

const BASE: u64 = 0x5000_0000;
const ROOT_OFFSET: usize = 0;
const BOUNDS_SIZE: usize = 112;
const GEOMETRY_SIZE: usize = 304;
const GEOMETRY_BVH_SIZE: usize = 336;
const COMPOSITE_SIZE: usize = 176;
const YBN_POLYGON_MATCH_TOLERANCE: f32 = 0.005;

#[cfg(test)]
mod tests;
