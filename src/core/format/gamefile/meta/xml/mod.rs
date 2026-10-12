use std::{collections::HashMap, fmt::Write as _, io};

use serde_json::{Map, Number, Value};

use super::{MetaResource, MetaStructureEntry, MetaStructureInfo};

mod helpers;
mod read;
mod schema;
mod value;
mod write;

use helpers::*;
pub use read::meta_from_xml;
pub(crate) use schema::known_hash_names;
use schema::{ARRAY, ARRAY_INFO_HASH, STRUCTURE, STRUCTURE_POINTER, fallback_structure};
pub(crate) use value::meta_to_value;
pub use write::meta_to_xml;

#[cfg(test)]
mod tests;
