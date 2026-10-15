use std::{collections::HashMap, fmt::Write as _, io};

use super::{MetaResource, MetaStructureEntry, MetaStructureInfo};

mod access;
mod helpers;
mod read;
mod schema;
mod write;

pub use access::MetaStruct;
use helpers::*;
pub use read::meta_from_xml;
pub(crate) use schema::known_hash_names;
use schema::{ARRAY, ARRAY_INFO_HASH, STRUCTURE, STRUCTURE_POINTER, fallback_structure};
pub use write::meta_to_xml;

#[cfg(test)]
mod tests;
