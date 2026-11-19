//! What an addon clothing resource ships: its collections and their files.
//!
//! A collection is a Ped variation YMT, `{ped}_{collection}.ymt`, and the drawables, textures
//! and cloth streamed under its prefix: `{ped}_{collection}^…` for components and
//! `{ped}_p_{collection}^…` for props. Files may sit anywhere in the resource.

pub mod names;
pub mod variation;

use std::{
  io,
  path::{Path, PathBuf},
};

use names::{AddonFileName, Slot, is_prop_prefix_of};
use variation::{Declared, read_variation};

use crate::platform::fs;

/// A streamed drawable, texture or cloth file and its parsed name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionFile {
  /// The file's path.
  pub path: PathBuf,
  /// Its parsed name.
  pub name: AddonFileName,
}

/// One addon collection of a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
  /// The lowercase component prefix, `{ped}_{collection}`, which the YMT is named after.
  pub prefix: String,
  /// The Ped variation YMT.
  pub variation: PathBuf,
  /// What the YMT declares.
  pub declared: Declared,
  /// The collection's files, sorted by path.
  pub files: Vec<CollectionFile>,
}

impl Collection {
  /// The files naming a drawable or prop the YMT does not declare.
  pub fn undeclared(&self) -> Vec<&CollectionFile> {
    self
      .files
      .iter()
      .filter(|file| {
        let declared = self.declared.slots.iter().find(|(slot, _)| *slot == file.name.slot);
        declared.is_none_or(|(_, drawables)| file.name.index as usize >= drawables.len())
      })
      .collect()
  }
}

/// The clothing a resource ships.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceClothing {
  /// Its addon collections, sorted by prefix.
  pub collections: Vec<Collection>,
  /// Drawables, textures and cloth without a prefix, sorted by path.
  pub loose: Vec<CollectionFile>,
  /// Drawables, textures and cloth whose prefix names no collection of the resource, sorted by
  /// path.
  pub unmatched: Vec<CollectionFile>,
}

impl ResourceClothing {
  /// Reads the collections and clothing files below `resource`.
  pub fn read(resource: &Path) -> io::Result<Self> {
    let mut files = Vec::new();
    let mut collections = Vec::new();
    let mut stack = vec![resource.to_path_buf()];
    while let Some(directory) = stack.pop() {
      for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
          stack.push(path);
          continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if let Some(name) = AddonFileName::parse(&file_name) {
          files.push(CollectionFile {
            path,
            name,
          });
        } else if let Some(stem) = file_name.strip_suffix(".ymt") {
          let bytes = fs::read(&path)?;
          // Other YMTs, such as creature metadata, are no collection.
          let Ok(root) = read_variation(&bytes) else {
            continue;
          };
          let declared = Declared::of(&root).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", path.display()))
          })?;
          collections.push(Collection {
            prefix: stem.to_owned(),
            variation: path,
            declared,
            files: Vec::new(),
          });
        }
      }
    }
    collections.sort_by(|a, b| a.prefix.cmp(&b.prefix));
    if let Some(pair) = collections.windows(2).find(|pair| pair[0].prefix == pair[1].prefix) {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
          "Collection {} is declared twice: {} and {}",
          pair[0].prefix,
          pair[0].variation.display(),
          pair[1].variation.display()
        ),
      ));
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut clothing = Self {
      collections,
      ..Self::default()
    };
    for file in files {
      let Some(prefix) = file.name.prefix.as_deref() else {
        clothing.loose.push(file);
        continue;
      };
      let collection = clothing.collections.iter_mut().find(|collection| match file.name.slot {
        Slot::Component(_) => collection.prefix == prefix,
        Slot::Prop(_) => is_prop_prefix_of(prefix, &collection.prefix),
      });
      match collection {
        Some(collection) => collection.files.push(file),
        None => clothing.unmatched.push(file),
      }
    }
    Ok(clothing)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::game_archive::Staging;

  const VARIATION: &str = r#"<CPedVariationInfo>
 <availComp>255 255 255 255 255 255 255 255 255 255 255 0</availComp>
 <aComponentData3 itemType="CPVComponentData">
  <Item>
   <aDrawblData3 itemType="CPVDrawblData">
    <Item><aTexData itemType="CPVTextureData"><Item /></aTexData></Item>
   </aDrawblData3>
  </Item>
 </aComponentData3>
 <propInfo>
  <aAnchors itemType="CAnchorProps">
   <Item><props>1</props><anchor>ANCHOR_HEAD</anchor></Item>
  </aAnchors>
 </propInfo>
</CPedVariationInfo>"#;

  #[test]
  fn resource_files_group_into_their_collections() {
    let root = std::env::temp_dir().join(format!("clothing_collection_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let _cleanup = Staging(root.clone());
    let write = |path: &str, bytes: &[u8]| {
      let path = root.join(path);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, bytes).unwrap();
    };
    write("stream/MP_M_Freemode_01_mp_m_smpl_pack.ymt", VARIATION.as_bytes());
    write("stream/creaturemetadata/mp_creaturemetadata_smpl_pack.ymt", b"<CCreatureMetaData />");
    let component = "stream/mp_m_freemode_01_mp_m_smpl_pack";
    write(&format!("{component}/mp_m_freemode_01_mp_m_smpl_pack^jbib_000_u.ydd"), b"");
    write(&format!("{component}/mp_m_freemode_01_mp_m_smpl_pack^jbib_diff_000_a_uni.ytd"), b"");
    write(&format!("{component}/mp_m_freemode_01_mp_m_smpl_pack^jbib_001_u.ydd"), b"");
    write("stream/props/mp_m_freemode_01_p_mp_m_smpl_pack^p_head_000.ydd", b"");
    write("stream/mp_m_freemode_01_mp_m_other^lowr_000_r.ydd", b"");
    write("loose/feet_000_u.ydd", b"");
    write("fxmanifest.lua", b"");

    let clothing = ResourceClothing::read(&root).unwrap();
    assert_eq!(clothing.collections.len(), 1);
    let collection = &clothing.collections[0];
    assert_eq!(collection.prefix, "mp_m_freemode_01_mp_m_smpl_pack");
    let names =
      |files: &[CollectionFile]| files.iter().map(|file| file.name.file_name()).collect::<Vec<_>>();
    assert_eq!(
      names(&collection.files),
      [
        "mp_m_freemode_01_mp_m_smpl_pack^jbib_000_u.ydd",
        "mp_m_freemode_01_mp_m_smpl_pack^jbib_001_u.ydd",
        "mp_m_freemode_01_mp_m_smpl_pack^jbib_diff_000_a_uni.ytd",
        "mp_m_freemode_01_p_mp_m_smpl_pack^p_head_000.ydd",
      ]
    );
    assert_eq!(
      collection.undeclared().iter().map(|file| file.name.file_name()).collect::<Vec<_>>(),
      ["mp_m_freemode_01_mp_m_smpl_pack^jbib_001_u.ydd"]
    );
    assert_eq!(names(&clothing.loose), ["feet_000_u.ydd"]);
    assert_eq!(names(&clothing.unmatched), ["mp_m_freemode_01_mp_m_other^lowr_000_r.ydd"]);

    write("other/mp_m_freemode_01_mp_m_smpl_pack.ymt", VARIATION.as_bytes());
    assert!(ResourceClothing::read(&root).unwrap_err().to_string().contains("declared twice"));
  }

  /// Every local resource reads, and its collections stream only what their YMTs declare.
  #[test]
  #[ignore = "requires local asset/clothing/source"]
  fn local_resources_read() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/clothing/source");
    let mut resources = vec![];
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
      for entry in fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
          continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with('[') && name.ends_with(']') {
          stack.push(path);
        } else {
          resources.push(path);
        }
      }
    }
    let mut report = Vec::new();
    for resource in resources {
      let clothing = ResourceClothing::read(&resource).unwrap();
      let undeclared: usize =
        clothing.collections.iter().map(|collection| collection.undeclared().len()).sum();
      report.push(format!(
        "{}: {} collections, {} files, {} undeclared, {} loose, {} unmatched",
        resource.file_name().unwrap().to_string_lossy(),
        clothing.collections.len(),
        clothing.collections.iter().map(|collection| collection.files.len()).sum::<usize>(),
        undeclared,
        clothing.loose.len(),
        clothing.unmatched.len()
      ));
    }
    report.sort();
    println!("{}", report.join("\n"));
    assert!(!report.is_empty());
  }
}
