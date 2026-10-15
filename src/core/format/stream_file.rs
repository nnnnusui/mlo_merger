//! Extension-based dispatch from native stream files to their typed models and XML.

use std::{collections::HashMap, fs, io, path::Path};

use crate::core::format::{
  gamefile::{
    meta::{MetaResource, MetaSchemaCatalog, xml::meta_from_xml},
    resource::Resource,
    xml_tree::parse_xml,
    ynd::{xml_to_ynd, ynd_to_xml},
  },
  ybn::{model::Bound, read_ybn, xml_to_ybn, ybn_to_xml},
  ymap::{binary::ymap_to_model, model::Ymap},
};

/// Native stream file families, selected by filename extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
  Ymap,
  Ytyp,
  /// RSC7 META `.ymt`; plain-text XML `.ymt` files pass through unchanged.
  Ymt,
  Ybn,
  Ynd,
}

impl FileType {
  /// Selects the file type from a case-insensitive filename extension.
  pub fn from_path(path: &Path) -> io::Result<Self> {
    match path.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase) {
      Some(extension) if extension == "ymap" => Ok(Self::Ymap),
      Some(extension) if extension == "ytyp" => Ok(Self::Ytyp),
      Some(extension) if extension == "ymt" => Ok(Self::Ymt),
      Some(extension) if extension == "ybn" => Ok(Self::Ybn),
      Some(extension) if extension == "ynd" => Ok(Self::Ynd),
      _ => Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("unsupported stream file extension: {}", path.display()),
      )),
    }
  }

  /// Selects the file type from an XML filename such as `map.ymap.xml` or `map.ymap.pso.xml`.
  pub fn from_xml_path(path: &Path) -> io::Result<Self> {
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or_else(|| invalid_data("XML path has no UTF-8 filename"))?;
    let source_name = name
      .strip_suffix(".xml")
      .or_else(|| name.strip_suffix(".XML"))
      .ok_or_else(|| invalid_data("input filename must end in .xml"))?;
    let source_name = source_name
      .strip_suffix(".pso")
      .or_else(|| source_name.strip_suffix(".PSO"))
      .unwrap_or(source_name);
    Self::from_path(Path::new(source_name))
  }

  /// Returns the lowercase filename extension without the leading dot.
  pub fn extension(self) -> &'static str {
    match self {
      Self::Ymap => "ymap",
      Self::Ytyp => "ytyp",
      Self::Ymt => "ymt",
      Self::Ybn => "ybn",
      Self::Ynd => "ynd",
    }
  }

  /// Whether files of this type are generic META resources described by embedded schemas.
  pub fn is_meta(self) -> bool {
    matches!(self, Self::Ymap | Self::Ytyp | Self::Ymt)
  }

  /// The RSC version used when encoding this type.
  fn rsc_version(self) -> u32 {
    match self {
      Self::Ymap | Self::Ytyp | Self::Ymt => 2,
      Self::Ybn => 43,
      Self::Ynd => 1,
    }
  }
}

/// A decoded stream file model tagged by its file type.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamFile {
  Ymap(Ymap),
  Ybn(Bound),
}

impl StreamFile {
  /// Reads a native stream file and decodes it according to its extension.
  ///
  /// `shared_hash_names` resolves hashes the file itself does not name; YBN ignores it.
  pub fn read(
    path: &Path,
    shared_hash_names: &HashMap<u32, String>,
  ) -> io::Result<Self> {
    Self::decode(FileType::from_path(path)?, &fs::read(path)?, shared_hash_names)
  }

  /// Decodes native stream file bytes as the given file type.
  ///
  /// Only YMAP and YBN have typed models; other types are rejected.
  pub fn decode(
    file_type: FileType,
    bytes: &[u8],
    shared_hash_names: &HashMap<u32, String>,
  ) -> io::Result<Self> {
    match file_type {
      FileType::Ymap => ymap_to_model(bytes, shared_hash_names).map(Self::Ymap),
      FileType::Ybn => read_ybn(bytes).map(Self::Ybn),
      other => Err(invalid_data(&format!("{} has no typed model", other.extension()))),
    }
  }

  /// Reads a native stream file and converts it to CodeWalker-style XML by its extension.
  pub fn read_to_xml(
    path: &Path,
    shared_hash_names: &HashMap<u32, String>,
  ) -> io::Result<String> {
    Self::decode_to_xml(FileType::from_path(path)?, &fs::read(path)?, shared_hash_names)
  }

  /// Converts native stream file bytes to CodeWalker-style XML.
  pub fn decode_to_xml(
    file_type: FileType,
    bytes: &[u8],
    shared_hash_names: &HashMap<u32, String>,
  ) -> io::Result<String> {
    match file_type {
      FileType::Ynd => return ynd_to_xml(bytes, shared_hash_names),
      FileType::Ybn => return ybn_to_xml(bytes),
      FileType::Ymt if !bytes.starts_with(b"RSC7") && !bytes.starts_with(b"PSIN") => {
        if let Ok(xml) = std::str::from_utf8(bytes)
          && xml.trim_start().starts_with('<')
          && parse_xml(xml).is_ok()
        {
          return Ok(xml.to_string());
        }
        let variant = match bytes.get(..4) {
          Some(b"PSIN") | Some(b"PSO ") => "PSO/PSIN",
          Some(b"RBF ") => "RBF",
          _ => "unknown",
        };
        return Err(invalid_data(&format!("Native YMT {variant} variant is not supported yet")));
      }
      FileType::Ymap | FileType::Ytyp | FileType::Ymt => {}
    }
    match Resource::decode(bytes)? {
      Resource::Pso(resource) => resource.to_xml(shared_hash_names),
      Resource::Rsc7(resource) => MetaResource::parse(&resource)?.to_xml(shared_hash_names),
    }
  }

  /// Encodes CodeWalker-style XML into native stream file bytes.
  ///
  /// META types resolve their structures through `catalog`; plain-text YMT passes through.
  pub fn encode_xml(
    file_type: FileType,
    xml: &str,
    catalog: &MetaSchemaCatalog,
  ) -> io::Result<Vec<u8>> {
    match file_type {
      FileType::Ynd => return xml_to_ynd(xml),
      FileType::Ybn => return xml_to_ybn(xml),
      FileType::Ymap | FileType::Ytyp | FileType::Ymt => {}
    }
    let meta = match meta_from_xml(xml, catalog) {
      Ok(meta) => meta,
      Err(_error) if file_type == FileType::Ymt && parse_xml(xml).is_ok() => {
        return Ok(xml.as_bytes().to_vec());
      }
      Err(error) => return Err(error),
    };
    meta.to_rsc7(file_type.rsc_version())?.encode()
  }

  pub fn file_type(&self) -> FileType {
    match self {
      Self::Ymap(_) => FileType::Ymap,
      Self::Ybn(_) => FileType::Ybn,
    }
  }
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn file_type_from_path_ignores_extension_case() {
    assert_eq!(FileType::from_path(Path::new("a/b.YMAP")).unwrap(), FileType::Ymap);
    assert_eq!(FileType::from_path(Path::new("b.ybn")).unwrap(), FileType::Ybn);
    assert_eq!(FileType::from_path(Path::new("b.Ymt")).unwrap(), FileType::Ymt);
    assert!(FileType::from_path(Path::new("b.ydr")).is_err());
    assert!(FileType::from_path(Path::new("b")).is_err());
  }

  #[test]
  fn file_type_from_xml_path_strips_xml_and_pso_suffixes() {
    assert_eq!(FileType::from_xml_path(Path::new("a/b.ymap.pso.xml")).unwrap(), FileType::Ymap);
    assert_eq!(FileType::from_xml_path(Path::new("b.ytyp.XML")).unwrap(), FileType::Ytyp);
    assert!(FileType::from_xml_path(Path::new("b.ymap")).is_err());
  }
}
