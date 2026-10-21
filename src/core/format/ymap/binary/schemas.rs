//! The META schemas every YMAP is written with, read from `schemas.json`.
//!
//! Each structure and enum of RSC7 YMAPs has one layout across every vanilla version, so YMAPs
//! are written with these, whatever schemas the YMAP they came from embeds. The table is taken
//! from the latest vanilla YMAPs; see the tests for checking and regenerating it.

use std::{io, sync::LazyLock};

use serde::{Deserialize, Serialize};

use crate::core::format::gamefile::meta::{
  MetaEnumEntry, MetaEnumInfo, MetaSchemaCatalog, MetaStructureEntry, MetaStructureInfo,
};

/// The schemas of every YMAP structure and enum, keyed by name hash.
pub(super) fn schemas() -> &'static MetaSchemaCatalog {
  static SCHEMAS: LazyLock<MetaSchemaCatalog> = LazyLock::new(|| {
    SchemaFile::parse(include_str!("schemas.json")).expect("schemas.json is a valid schema table")
  });
  &SCHEMAS
}

/// `schemas.json`: schemas sorted by name hash, hashes spelled as eight hex digits, with their
/// names where known.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct SchemaFile {
  structures: Vec<Structure>,
  enums: Vec<Enum>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Structure {
  hash: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  name: Option<String>,
  structure_key: u32,
  unknown_8: u32,
  unknown_12: u32,
  unknown_28: i16,
  size: u32,
  entries: Vec<Entry>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Entry {
  hash: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  name: Option<String>,
  offset: u32,
  data_type: u8,
  unknown: u8,
  reference_type_index: i16,
  reference_key: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Enum {
  hash: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  name: Option<String>,
  enum_key: u32,
  unknown_20: u32,
  entries: Vec<EnumValue>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct EnumValue {
  hash: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  name: Option<String>,
  value: i32,
}

#[cfg(test)]
fn hex(hash: u32) -> String {
  format!("{hash:08X}")
}

fn hash(text: &str) -> io::Result<u32> {
  u32::from_str_radix(text, 16)
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{text}: {error}")))
}

impl SchemaFile {
  fn parse(text: &str) -> io::Result<MetaSchemaCatalog> {
    let file: Self = serde_json::from_str(text)?;
    let mut catalog = MetaSchemaCatalog::default();
    for structure in file.structures {
      let schema = MetaStructureInfo {
        name_hash: hash(&structure.hash)?,
        structure_key: structure.structure_key,
        unknown_8: structure.unknown_8,
        unknown_12: structure.unknown_12,
        unknown_28: structure.unknown_28,
        structure_size: structure.size,
        entries: structure
          .entries
          .iter()
          .map(|entry| {
            Ok(MetaStructureEntry {
              name_hash: hash(&entry.hash)?,
              data_offset: entry.offset,
              data_type: entry.data_type,
              unknown: entry.unknown,
              reference_type_index: entry.reference_type_index,
              reference_key: hash(&entry.reference_key)?,
            })
          })
          .collect::<io::Result<_>>()?,
      };
      catalog.structures.insert(schema.name_hash, schema);
    }
    for schema in file.enums {
      let schema = MetaEnumInfo {
        name_hash: hash(&schema.hash)?,
        enum_key: schema.enum_key,
        unknown_20: schema.unknown_20,
        entries: schema
          .entries
          .iter()
          .map(|entry| {
            Ok(MetaEnumEntry {
              name_hash: hash(&entry.hash)?,
              value: entry.value,
            })
          })
          .collect::<io::Result<_>>()?,
      };
      catalog.enums.insert(schema.name_hash, schema);
    }
    Ok(catalog)
  }

  /// The table of `catalog`, naming hashes `names` knows.
  #[cfg(test)]
  fn from_catalog(
    catalog: &MetaSchemaCatalog,
    names: &std::collections::HashMap<u32, String>,
  ) -> Self {
    let name = |hash: u32| names.get(&hash).cloned();
    let mut structures = catalog.structures.values().collect::<Vec<_>>();
    structures.sort_by_key(|schema| schema.name_hash);
    let mut enums = catalog.enums.values().collect::<Vec<_>>();
    enums.sort_by_key(|schema| schema.name_hash);
    Self {
      structures: structures
        .into_iter()
        .map(|schema| Structure {
          hash: hex(schema.name_hash),
          name: name(schema.name_hash),
          structure_key: schema.structure_key,
          unknown_8: schema.unknown_8,
          unknown_12: schema.unknown_12,
          unknown_28: schema.unknown_28,
          size: schema.structure_size,
          entries: schema
            .entries
            .iter()
            .map(|entry| Entry {
              hash: hex(entry.name_hash),
              name: name(entry.name_hash),
              offset: entry.data_offset,
              data_type: entry.data_type,
              unknown: entry.unknown,
              reference_type_index: entry.reference_type_index,
              reference_key: hex(entry.reference_key),
            })
            .collect(),
        })
        .collect(),
      enums: enums
        .into_iter()
        .map(|schema| Enum {
          hash: hex(schema.name_hash),
          name: name(schema.name_hash),
          enum_key: schema.enum_key,
          unknown_20: schema.unknown_20,
          entries: schema
            .entries
            .iter()
            .map(|entry| EnumValue {
              hash: hex(entry.name_hash),
              name: name(entry.name_hash),
              value: entry.value,
            })
            .collect(),
        })
        .collect(),
    }
  }

  /// The table as `schemas.json` spells it: one line per entry, so changes read as line diffs.
  #[cfg(test)]
  fn to_text(&self) -> String {
    let mut text = String::from("{\n  \"structures\": [\n");
    for (index, structure) in self.structures.iter().enumerate() {
      let mut head = serde_json::to_value(structure).expect("structures serialize");
      head.as_object_mut().expect("structures are objects").remove("entries");
      let head = head.to_string();
      text.push_str(&format!("    {},\n      \"entries\": [\n", &head[..head.len() - 1]));
      let entries = structure.entries.iter().map(line).collect::<Vec<_>>();
      text.push_str(&format!("        {}\n      ]\n    }}", entries.join(",\n        ")));
      text.push_str(if index + 1 < self.structures.len() { ",\n" } else { "\n" });
    }
    text.push_str("  ],\n  \"enums\": [\n");
    for (index, schema) in self.enums.iter().enumerate() {
      let mut head = serde_json::to_value(schema).expect("enums serialize");
      head.as_object_mut().expect("enums are objects").remove("entries");
      let head = head.to_string();
      text.push_str(&format!("    {},\n      \"entries\": [\n", &head[..head.len() - 1]));
      let entries = schema.entries.iter().map(line).collect::<Vec<_>>();
      text.push_str(&format!("        {}\n      ]\n    }}", entries.join(",\n        ")));
      text.push_str(if index + 1 < self.enums.len() { ",\n" } else { "\n" });
    }
    text.push_str("  ]\n}\n");
    text
  }
}

/// A table record on one line.
#[cfg(test)]
fn line(record: &impl Serialize) -> String {
  serde_json::to_string(record).expect("schema records serialize")
}

#[cfg(test)]
mod tests {
  use std::{fs, path::Path};

  use super::*;
  use crate::core::format::gamefile::{
    meta::{MetaResource, xml::known_hash_names},
    resource::Resource,
  };

  #[test]
  fn the_table_parses_and_holds_the_ymap_root() {
    let schemas = schemas();
    assert!(
      schemas.structures.contains_key(&crate::core::format::gamefile::meta::jenk_hash("CMapData"))
    );
    assert!(schemas.structures.values().all(|schema| !schema.entries.is_empty()));
  }

  /// Every latest vanilla RSC7 YMAP embeds schemas of the table's layouts, and the table holds
  /// exactly those; `MLO_WRITE_SAMPLES=1` rewrites it from them.
  #[test]
  #[ignore = "requires local asset/vanilla-cache"]
  fn the_table_holds_the_latest_vanilla_schemas() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
    let mut catalog = MetaSchemaCatalog::default();
    let mut conflicts = Vec::new();
    for entry in fs::read_dir(directory).unwrap() {
      let path = entry.unwrap().path();
      let Ok(Resource::Rsc7(resource)) = Resource::decode(&fs::read(&path).unwrap()) else {
        continue;
      };
      let meta = MetaResource::parse(&resource).unwrap();
      for schema in &meta.structures {
        if catalog.structures.get(&schema.name_hash).is_some_and(|known| known != schema) {
          conflicts.push(format!("{} {:08X}", path.display(), schema.name_hash));
        }
      }
      for schema in &meta.enums {
        if catalog.enums.get(&schema.name_hash).is_some_and(|known| known != schema) {
          conflicts.push(format!("{} enum {:08X}", path.display(), schema.name_hash));
        }
      }
      catalog.add_resource(&meta);
    }
    assert!(conflicts.is_empty(), "schemas with several layouts: {conflicts:?}");
    let text = SchemaFile::from_catalog(&catalog, known_hash_names()).to_text();
    let path =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core/format/ymap/binary/schemas.json");
    if std::env::var_os("MLO_WRITE_SAMPLES").is_some() {
      fs::write(&path, &text).unwrap();
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), text, "rerun with MLO_WRITE_SAMPLES=1");
    let parsed = SchemaFile::parse(&text).unwrap();
    assert_eq!(parsed.structures, catalog.structures);
    assert_eq!(parsed.enums, catalog.enums);
  }
}
