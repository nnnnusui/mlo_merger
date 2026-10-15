//! Extension-based dispatch from native stream files to their typed models.

use std::{collections::HashMap, fs, io, path::Path};

use crate::core::format::{
  ybn::{model::Bound, read_ybn},
  ymap::{binary::ymap_to_model, model::Ymap},
};

/// Native stream file families that decode into a typed model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
  Ymap,
  Ybn,
}

impl FileType {
  /// Selects the file type from a case-insensitive filename extension.
  pub fn from_path(path: &Path) -> Option<Self> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("ymap") {
      Some(Self::Ymap)
    } else if extension.eq_ignore_ascii_case("ybn") {
      Some(Self::Ybn)
    } else {
      None
    }
  }

  /// Returns the lowercase filename extension without the leading dot.
  pub fn extension(self) -> &'static str {
    match self {
      Self::Ymap => "ymap",
      Self::Ybn => "ybn",
    }
  }
}

/// A decoded stream file model tagged by its file type.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamFileModel {
  Ymap(Ymap),
  Ybn(Bound),
}

impl StreamFileModel {
  pub fn file_type(&self) -> FileType {
    match self {
      Self::Ymap(_) => FileType::Ymap,
      Self::Ybn(_) => FileType::Ybn,
    }
  }
}

/// Reads a native stream file and decodes it according to its extension.
pub fn read_stream_file(path: &Path) -> io::Result<StreamFileModel> {
  let file_type = FileType::from_path(path).ok_or_else(|| {
    io::Error::new(
      io::ErrorKind::InvalidInput,
      format!("unsupported stream file extension: {}", path.display()),
    )
  })?;
  decode_stream_file(file_type, &fs::read(path)?)
}

/// Decodes native stream file bytes as the given file type.
pub fn decode_stream_file(
  file_type: FileType,
  bytes: &[u8],
) -> io::Result<StreamFileModel> {
  match file_type {
    FileType::Ymap => ymap_to_model(bytes, &HashMap::new()).map(StreamFileModel::Ymap),
    FileType::Ybn => read_ybn(bytes).map(StreamFileModel::Ybn),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn file_type_from_path_ignores_extension_case() {
    assert_eq!(FileType::from_path(Path::new("a/b.YMAP")), Some(FileType::Ymap));
    assert_eq!(FileType::from_path(Path::new("b.ybn")), Some(FileType::Ybn));
    assert_eq!(FileType::from_path(Path::new("b.ytyp")), None);
    assert_eq!(FileType::from_path(Path::new("b")), None);
  }
}
