use std::{
  collections::{BTreeSet, HashMap},
  fs, io,
  path::{Path, PathBuf},
};

use super::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  meta_xml::{meta_to_xml, ymap_to_xml},
  resource_file::Rsc7Resource,
  xml_meta_builder::meta_from_xml,
  xml_tree::parse_xml,
  ynd::{xml_to_ynd, ynd_to_xml},
};
use crate::core::format::ybn::{xml_to_ybn, ybn_to_xml};
use walkdir::WalkDir;

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

mod batch;
mod format;
mod names;
mod resource;

pub(crate) use batch::prune_managed_ymap_outputs;
pub use batch::{convert_files_from_xml, convert_files_to_xml};
pub use format::NativeResourceFormat;
pub(crate) use names::load_vanilla_hash_names;
pub use resource::{resource_to_xml, xml_to_resource};

#[cfg(test)]
use batch::discover_schema_inputs;
#[cfg(test)]
mod tests;
