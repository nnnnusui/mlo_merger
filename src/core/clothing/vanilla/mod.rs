//! Versioned raw Ped variation YMTs from base game, title updates and DLC archives.

use std::{io, path::Path};

use crate::core::{
  format::gamefile::{meta::MetaResource, resource::Resource, xml_tree::parse_xml},
  game_archive::{BuildGameArchive, FileClassification, FileSelection},
  vanilla::write_hash_index,
};

#[cfg(test)]
mod tests;

pub(crate) const FILES: FileSelection = FileSelection {
  extensions: &["ymt"],
  classify: classify_ymt,
};

/// Compatibility entry point; use [`crate::app::clothing::GenerateVanillaClothing`] for new callers.
#[deprecated(note = "Use mlo_merger::app::clothing::GenerateVanillaClothing")]
#[derive(Debug, Clone)]
pub struct GenerateVanillaClothing {
  /// Installed GTA V Legacy directory.
  pub game_dir: std::path::PathBuf,
  /// Output archive, normally `asset/vanilla-clothing`.
  pub output_dir: std::path::PathBuf,
  /// CodeWalker's `magic.dat`, used with `GTA5.exe` to decrypt the archives.
  pub magic: std::path::PathBuf,
}

#[allow(deprecated)]
impl GenerateVanillaClothing {
  /// Extracts raw supported clothing YMTs in vanilla stage order.
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    generate_archive(&self.game_dir, &self.output_dir, &self.magic)
  }
}

pub(crate) fn generate_archive(
  game_dir: &Path,
  output_dir: &Path,
  magic: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
  BuildGameArchive {
    game_dir: game_dir.to_path_buf(),
    output_dir: output_dir.to_path_buf(),
    magic: magic.to_path_buf(),
    files: FILES,
  }
  .run_with_metadata(&["hash_names.json"], &["rpf_names.json"], write_hash_index)
}

/// Classifies supported YMT roots rather than relying on DLC-specific filenames.
/// RBF needs only its root descriptor here; the collected payload is kept raw.
fn classify_ymt(bytes: &[u8]) -> io::Result<FileClassification> {
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
      kind_from_root_bytes(name)
    }
    _ => {
      let xml = std::str::from_utf8(bytes)
        .map_err(|_| invalid("Cannot classify unsupported binary YMT format"))?;
      kind_from_root(&parse_xml(xml.trim_start_matches('\u{feff}'))?.name)
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
  if hash == crate::core::format::gamefile::meta::jenk_hash("CPedVariationInfo") {
    Some("ped_variation")
  } else if hash == crate::core::format::gamefile::meta::jenk_hash("CCreatureMetaData") {
    Some("creature_metadata")
  } else {
    None
  }
}

fn kind_from_root(root: &str) -> Option<&'static str> {
  kind_from_root_bytes(root.as_bytes())
}

fn kind_from_root_bytes(root: &[u8]) -> Option<&'static str> {
  match root {
    b"CPedVariationInfo" => Some("ped_variation"),
    b"CCreatureMetaData" => Some("creature_metadata"),
    _ => None,
  }
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
