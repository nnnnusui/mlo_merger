use std::{collections::HashMap, fmt::Write as _, io};

use super::{MetaResource, MetaStructureEntry, MetaStructureInfo};
use crate::core::format::gamefile::hash_names::HashNames;

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

#[cfg(test)]
mod tests;
