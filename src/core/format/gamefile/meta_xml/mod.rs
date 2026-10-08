use std::{collections::HashMap, fmt::Write as _, io};

use serde_json::{Map, Number, Value};

use super::meta_resource::{MetaResource, MetaStructureEntry, MetaStructureInfo};
use super::resource_file::Rsc7Resource;

mod helpers;
mod model;
mod schema;
mod write;

use helpers::*;
pub use model::{ymap_to_model, ymap_to_xml};
pub(crate) use model::{ymap_to_model_with_entities, ymap_to_model_with_entities_from_meta};
pub(crate) use schema::known_hash_names;
use schema::{ARRAY, ARRAY_INFO_HASH, STRUCTURE, STRUCTURE_POINTER, fallback_structure};
pub use write::meta_to_xml;

#[cfg(test)]
mod tests;
