use super::*;

/// META tables and data blocks embedded in an RSC7 resource.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaResource {
  /// One-based index of the root data block.
  pub root_block_index: i32,
  /// META structure schemas stored in the resource.
  pub structures: Vec<MetaStructureInfo>,
  /// META enum schemas stored in the resource.
  pub enums: Vec<MetaEnumInfo>,
  /// Raw data blocks referenced by the schemas.
  pub data_blocks: Vec<MetaDataBlock>,
  /// Optional META name string.
  pub name: Option<String>,
}

/// A payload-free union of schemas and names collected from one or more META resources.
#[derive(Debug, Clone, Default)]
pub struct MetaSchemaCatalog {
  /// Schemas keyed by structure-name hash.
  pub structures: std::collections::HashMap<u32, MetaStructureInfo>,
  /// Enum definitions keyed by enum-name hash.
  pub enums: std::collections::HashMap<u32, MetaEnumInfo>,
  /// Embedded string names keyed by Jenk hash.
  pub hash_names: std::collections::HashMap<u32, String>,
}

impl MetaSchemaCatalog {
  /// Adds the schema, enum, and embedded-name metadata from a parsed resource.
  pub fn add_resource(
    &mut self,
    resource: &MetaResource,
  ) {
    self
      .structures
      .extend(resource.structures.iter().map(|schema| (schema.name_hash, schema.clone())));
    self.enums.extend(resource.enums.iter().map(|schema| (schema.name_hash, schema.clone())));
    self.hash_names.extend(resource.hash_names());
  }
}

impl MetaResource {
  /// Returns this resource's embedded string names indexed by their Jenk hashes.
  pub fn hash_names(&self) -> std::collections::HashMap<u32, String> {
    let mut names = std::collections::HashMap::new();
    for block in
      self.data_blocks.iter().filter(|block| block.structure_name_hash == META_STRING_TYPE)
    {
      for bytes in block.data.split(|byte| *byte == 0).filter(|bytes| !bytes.is_empty()) {
        if bytes.is_ascii() {
          let name = String::from_utf8_lossy(bytes).into_owned();
          names.entry(jenk_hash(&name)).or_insert(name);
        }
      }
    }
    names
  }
}

/// Schema for one META structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaStructureInfo {
  /// Hash identifying this structure type.
  pub name_hash: u32,
  /// Layout/version key used by the runtime, distinct from the type-name hash.
  pub structure_key: u32,
  /// Structure metadata at record offset 8.
  pub unknown_8: u32,
  /// Reserved structure metadata at record offset 12.
  pub unknown_12: u32,
  /// Reserved structure metadata at record offset 28.
  pub unknown_28: i16,
  /// Byte length of one structure instance.
  pub structure_size: u32,
  /// Fields in declaration order.
  pub entries: Vec<MetaStructureEntry>,
}

/// One field in a META structure schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaStructureEntry {
  /// Hash identifying the field name.
  pub name_hash: u32,
  /// Byte offset within its containing structure.
  pub data_offset: u32,
  /// META field data type identifier.
  pub data_type: u8,
  /// Additional type-specific field metadata.
  pub unknown: u8,
  /// Index into a related type table, when used by the field type.
  pub reference_type_index: i16,
  /// Related structure or enum hash, when used by the field type.
  pub reference_key: u32,
}

/// Schema for one META enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaEnumInfo {
  /// Hash identifying this enum type.
  pub name_hash: u32,
  /// Enum layout/version key stored separately from the name hash.
  pub enum_key: u32,
  /// Reserved enum metadata at record offset 20.
  pub unknown_20: u32,
  /// Enum values in declaration order.
  pub entries: Vec<MetaEnumEntry>,
}

/// One named value in a META enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaEnumEntry {
  /// Hash identifying the enum value name.
  pub name_hash: u32,
  /// Integer value associated with the name.
  pub value: i32,
}

/// Raw bytes for a META data block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaDataBlock {
  /// Hash identifying the structure represented by this block.
  pub structure_name_hash: u32,
  /// Block payload bytes.
  pub data: Vec<u8>,
}
