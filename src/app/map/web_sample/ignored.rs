//! Overwrites the pipeline leaves out, other than those written against earlier versions of the
//! resources' files: one of each [`IgnoredOverwriteReason`] but `SourceChanged`.
//!
//! [`IgnoredOverwriteReason`]: crate::app::map::overwrite::IgnoredOverwriteReason

use super::{block_02, entity, find, fit_extents, guid, position, sha256, ymap, ymap_bytes};
use crate::app::map::overwrite::{OverwriteFingerprint, StreamFileOverwrite, VanillaOverwrite};
use serde::Serialize;

/// The overwrites, each by its path below the workspace:
///
/// - `vanilla_changed`: one of the title update's `smpl_block_02.ymap`, which later stages
///   replace. It removes the fountain, which the latest stage still has, and the second oak,
///   which `smpl_summer` swaps for the kiosk, by base indices that name other entities there.
/// - `missing_source`: one of `smpl_custom_old.ymap`, which an earlier version of
///   `smpl_custom_props` shipped and the current one does not.
/// - `missing_vanilla`: one of `smpl_block_99.ymap`, which no stage of the vanilla archive has.
/// - `unsupported_format`: one leaving out a `smpl_street_tweaks` YTYP, a format overwrites do not
///   support.
pub(super) fn overwrites() -> Vec<(String, Vec<u8>)> {
  let fingerprint = |bytes: &[u8]| OverwriteFingerprint {
    sha256: sha256(bytes),
    size: bytes.len() as u64,
  };
  let mut files = Vec::new();
  let mut push = |path: &str, document: Vec<u8>| {
    files.push((format!("asset/map/overwrite/{path}.diff.json"), document));
  };

  let update = block_02(1);
  let removed = [guid::OAK_2, guid::FOUNTAIN].map(|guid| find(&update, guid));
  push(
    "vanilla/0001-update/ymap/smpl_block_02.ymap",
    json(&vanilla_overwrite(fingerprint(&ymap_bytes(&update)), &removed)),
  );

  let mut old = ymap("smpl_custom_old", "", 0, 1);
  old.entities =
    vec![entity("smpl_sign_board", guid::OLD_SIGN, position(-7092.0, 7096.0, 20.0), 0.0)];
  fit_extents(&mut old);
  push(
    "resources/smpl_custom_props/smpl_custom_old.ymap",
    json(&StreamFileOverwrite {
      schema_version: StreamFileOverwrite::SCHEMA_VERSION,
      ignore: false,
      target: Some(fingerprint(&ymap_bytes(&old))),
      base: Some(OverwriteFingerprint::empty_baseline().unwrap()),
      skip: Some(serde_json::json!({"entities": [{"Added": {"target_index": 0}}]})),
    }),
  );

  let mut block_99 = ymap("smpl_block_99", "", 0, 1);
  block_99.entities =
    vec![entity("smpl_bench", guid::BLOCK_99_BENCH, position(-7090.0, 7090.0, 20.0), 0.0)];
  fit_extents(&mut block_99);
  push(
    "vanilla/0000-base/ymap/smpl_block_99.ymap",
    json(&vanilla_overwrite(fingerprint(&ymap_bytes(&block_99)), &[0])),
  );

  push(
    "resources/smpl_street_tweaks/smpl_props.ytyp",
    json(&StreamFileOverwrite {
      schema_version: StreamFileOverwrite::SCHEMA_VERSION,
      ignore: true,
      target: None,
      base: None,
      skip: None,
    }),
  );
  files
}

/// A vanilla overwrite of the file `base` removing its entities at `removed`.
fn vanilla_overwrite(
  base: OverwriteFingerprint,
  removed: &[usize],
) -> VanillaOverwrite {
  let entities = removed
    .iter()
    .map(|base_index| serde_json::json!({"Removed": {"base_index": base_index}}))
    .collect::<Vec<_>>();
  VanillaOverwrite {
    schema_version: VanillaOverwrite::SCHEMA_VERSION,
    base,
    diffs: serde_json::from_value(serde_json::json!({"entities": entities})).unwrap(),
  }
}

fn json(document: &impl Serialize) -> Vec<u8> {
  serde_json::to_vec_pretty(document).unwrap()
}
