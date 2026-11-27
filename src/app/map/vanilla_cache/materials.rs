//! `materials.json`: the collision surface type names YBN materials index, read from the vanilla
//! archive's `materials.dat` (see [`crate::core::format::ybn::materials`]).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::Result;
use crate::core::format::ybn::materials::{SurfaceMaterials, parse_materials_dat};
use crate::platform::fs;

/// The vanilla cache's surface type names.
pub const MATERIALS_FILE: &str = "materials.json";

/// Where the vanilla cache at `cache_dir` keeps the surface type names.
pub fn materials_path(cache_dir: &Path) -> PathBuf {
  cache_dir.join(MATERIALS_FILE)
}

/// Writes `materials.json` into `output` from the `materials.dat` of the vanilla archive at
/// `vanilla_dir`; nothing when the archive has none.
pub(super) fn write_materials(
  vanilla_dir: &Path,
  output: &Path,
) -> Result<()> {
  let source = crate::core::game_archive::materials_dat_path(vanilla_dir);
  if !fs::is_file(&source) {
    tracing::warn!(
      "The vanilla archive has no {}; YBN surface types stay unnamed until it is regenerated",
      crate::core::game_archive::MATERIALS_DAT
    );
    return Ok(());
  }
  let bytes = fs::read(&source)?;
  let names = parse_materials_dat(&String::from_utf8_lossy(&bytes));
  tracing::info!("Read {} surface types from {}", names.len(), source.display());
  crate::common::json::write_json(
    &materials_path(output),
    &SurfaceMaterials {
      source: crate::core::game_archive::MATERIALS_DAT.to_owned(),
      sha256: hex::encode(Sha256::digest(&bytes)),
      names,
    },
  )?;
  Ok(())
}

/// The surface type names of the vanilla cache at `cache_dir`, by index; none when it holds no
/// `materials.json` or it cannot be read.
pub fn material_names(cache_dir: &Path) -> Vec<String> {
  let path = materials_path(cache_dir);
  if !fs::is_file(&path) {
    return Vec::new();
  }
  match crate::common::json::read_json::<SurfaceMaterials>(&path) {
    Ok(materials) => materials.names,
    Err(error) => {
      tracing::warn!("Cannot read {}: {error}", path.display());
      Vec::new()
    }
  }
}
