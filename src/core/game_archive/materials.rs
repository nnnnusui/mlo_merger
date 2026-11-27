//! The title update's `materials.dat`, which names the collision surface types YBN materials
//! index (see [`crate::core::format::ybn::materials`]).
//!
//! The title update replaces the base game's file
//! (`update/update.rpf/common/data/materials/materials.dat`). It is the same in the title update
//! of every FiveM game build from 3095 to 3889, and DLC packs declare none, so the vanilla
//! archive keeps the installed title update's once, as it is, at its root.

use std::path::{Path, PathBuf};

use super::Result;
use crate::platform::fs;

/// The game path of the `materials.dat` the archive keeps.
pub(crate) const MATERIALS_SOURCE: &str = "update/update.rpf/common/data/materials/materials.dat";

/// The vanilla archive's copy of [`MATERIALS_SOURCE`].
pub const MATERIALS_DAT: &str = "materials.dat";

/// Where the vanilla archive at `vanilla_dir` keeps `materials.dat`.
pub fn materials_dat_path(vanilla_dir: &Path) -> PathBuf {
  vanilla_dir.join(MATERIALS_DAT)
}

/// Writes `bytes`, the title update's `materials.dat`, into `output`.
pub(crate) fn write_materials(
  output: &Path,
  bytes: &[u8],
) -> Result<()> {
  fs::write(materials_dat_path(output), bytes)?;
  Ok(())
}
