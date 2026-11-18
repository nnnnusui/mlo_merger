//! The content sets a game archive holds, and how their files are selected.

use std::{collections::BTreeSet, io};

use serde::{Deserialize, Serialize};

use crate::core::format::gamefile::{
  meta::{MetaResource, jenk_hash},
  resource::Resource,
  xml_tree::parse_xml,
};

#[cfg(test)]
mod tests;

/// A set of game files extracted together into the vanilla archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum ArchiveContent {
  /// Map stream files: YMAP and YBN.
  Map,
  /// Ped clothing and creature metadata YMTs.
  Clothing,
}

impl ArchiveContent {
  /// Every content set, in archive order.
  pub const ALL: [Self; 2] = [Self::Map, Self::Clothing];

  /// The name the manifest and the command line use.
  pub fn name(self) -> &'static str {
    match self {
      Self::Map => "map",
      Self::Clothing => "clothing",
    }
  }

  /// The lowercase extensions of this content's files.
  pub(crate) fn extensions(self) -> &'static [&'static str] {
    match self {
      Self::Map => &["ymap", "ybn"],
      Self::Clothing => &["ymt"],
    }
  }

  /// The content a lowercase file name belongs to, by its extension.
  pub(crate) fn of_file(name: &str) -> Option<Self> {
    let (_, extension) = name.rsplit_once('.')?;
    Self::ALL.into_iter().find(|content| content.extensions().contains(&extension))
  }

  fn classify(
    self,
    bytes: &[u8],
  ) -> io::Result<FileClassification> {
    match self {
      Self::Map => Ok(FileClassification::Included {
        kind: None,
      }),
      Self::Clothing => classify_ymt(bytes),
    }
  }
}

impl std::fmt::Display for ArchiveContent {
  fn fmt(
    &self,
    formatter: &mut std::fmt::Formatter<'_>,
  ) -> std::fmt::Result {
    formatter.write_str(self.name())
  }
}

impl std::str::FromStr for ArchiveContent {
  type Err = String;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::ALL
      .into_iter()
      .find(|content| content.name() == value)
      .ok_or_else(|| format!("Unknown archive content: {value}"))
  }
}

/// Whether an extracted file is stored, and the kind recorded for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileClassification {
  Excluded,
  Included {
    kind: Option<&'static str>,
  },
}

/// The content sets one extraction selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileSelection {
  pub(crate) contents: BTreeSet<ArchiveContent>,
}

impl FileSelection {
  pub(crate) fn new(contents: impl IntoIterator<Item = ArchiveContent>) -> Self {
    Self {
      contents: contents.into_iter().collect(),
    }
  }

  /// The lowercase extensions of every selected content's files.
  pub(crate) fn extensions(&self) -> Vec<&'static str> {
    self.contents.iter().flat_map(|content| content.extensions()).copied().collect()
  }

  /// The selected content of files with the lowercase `extension`.
  pub(crate) fn content(
    &self,
    extension: &str,
  ) -> Option<ArchiveContent> {
    self.contents.iter().copied().find(|content| content.extensions().contains(&extension))
  }

  /// Classifies a file of a selected content's `extension`.
  pub(crate) fn classify(
    &self,
    extension: &str,
    bytes: &[u8],
  ) -> io::Result<FileClassification> {
    match self.content(extension) {
      Some(content) => content.classify(bytes),
      None => Ok(FileClassification::Excluded),
    }
  }
}

/// Classifies supported YMT roots rather than relying on DLC-specific filenames.
/// RBF needs only its root descriptor here; the collected payload is kept raw.
pub(crate) fn classify_ymt(bytes: &[u8]) -> io::Result<FileClassification> {
  let kind = match bytes.get(..4) {
    Some(b"RSC7") | Some(b"PSIN") => match Resource::decode(bytes)? {
      Resource::Rsc7(resource) => {
        let meta = MetaResource::parse(&resource)?;
        let index = meta
          .root_block_index
          .checked_sub(1)
          .and_then(|index| usize::try_from(index).ok())
          .ok_or_else(|| invalid("YMT META root block is invalid"))?;
        let root =
          meta.data_blocks.get(index).ok_or_else(|| invalid("YMT META root block is missing"))?;
        kind_from_hash(root.structure_name_hash)
      }
      Resource::Pso(resource) => kind_from_hash(resource.root_type_hash()?),
    },
    Some(b"RBF0") => {
      if bytes.get(4..6) != Some(&[0, 0]) {
        return Err(invalid("YMT RBF root descriptor is invalid"));
      }
      let length = bytes.get(6..8).ok_or_else(|| invalid("YMT RBF root is truncated"))?;
      let length = usize::from(u16::from_le_bytes([length[0], length[1]]));
      let name =
        bytes.get(8..8 + length).ok_or_else(|| invalid("YMT RBF root name is truncated"))?;
      kind_from_root(name)
    }
    _ => {
      let xml = std::str::from_utf8(bytes)
        .map_err(|_| invalid("Cannot classify unsupported binary YMT format"))?;
      kind_from_root(parse_xml(xml.trim_start_matches('\u{feff}'))?.name.as_bytes())
    }
  };
  Ok(match kind {
    Some(kind) => FileClassification::Included {
      kind: Some(kind),
    },
    None => FileClassification::Excluded,
  })
}

fn kind_from_hash(hash: u32) -> Option<&'static str> {
  if hash == jenk_hash("CPedVariationInfo") {
    Some("ped_variation")
  } else if hash == jenk_hash("CCreatureMetaData") {
    Some("creature_metadata")
  } else {
    None
  }
}

fn kind_from_root(root: &[u8]) -> Option<&'static str> {
  match root {
    b"CPedVariationInfo" => Some("ped_variation"),
    b"CCreatureMetaData" => Some("creature_metadata"),
    _ => None,
  }
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
