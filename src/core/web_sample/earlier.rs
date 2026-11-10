//! The earlier versions of two resources' files, which the sample's overwrites were written
//! against, and the source cache's history of them.

use super::{block_01, find, guid, sha256, workspace_files};
use crate::platform::fs;

/// The overwrites written against the earlier versions of two resources' `smpl_block_01.ymap`,
/// and those versions with their diff documents, kept in the source cache's history as a
/// pipeline run on them would have left them. The overwrite of `smpl_street_tweaks` reverts its
/// planter and its cone, which the current version no longer adds, so it is rewritten; that of
/// `smpl_block_rework` reverts its lamp's colour, which the current version changes otherwise,
/// so it is left to be written again.
pub(super) fn earlier_versions() -> &'static [(String, Vec<u8>)] {
  static FILES: std::sync::OnceLock<Vec<(String, Vec<u8>)>> = std::sync::OnceLock::new();
  FILES.get_or_init(|| {
    let root = std::env::temp_dir().join(format!("mlo_web_sample_earlier_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for file in workspace_files(true) {
      let path = root.join(&file.path);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, file.bytes).unwrap();
    }
    let asset = root.join("asset");
    crate::core::pipeline::Pipeline {
      source_dir: asset.join("source"),
      game_dir: asset.join("missing-game"),
      magic: asset.join("magic.dat"),
      vanilla_dir: asset.join("vanilla"),
      vanilla_cache_dir: asset.join("vanilla-cache"),
      source_cache_dir: asset.join("source-cache"),
      exclusions: None,
      merged_dir: asset.join("merged"),
      overwrite_dir: None,
      overwrite_cache_dir: asset.join("overwrite-cache"),
      output_dir: asset.join("merged_mlo"),
      gamebuild: None,
      force: false,
    }
    .run()
    .unwrap();
    let lamp = find(&block_01("smpl_block_01"), guid::LAMP);
    let mut files = Vec::new();
    for (resource, skip) in [
      (
        "smpl_street_tweaks",
        serde_json::json!({"entities": [
          {"Added": {"entity": {"guid": guid::PLANTER}}},
          {"Added": {"entity": {"guid": guid::CONE}}}
        ]}),
      ),
      ("smpl_block_rework", serde_json::json!({"entities": [{"Modified": {"base_index": lamp}}]})),
    ] {
      let cache = asset.join("source-cache");
      let stream_path =
        if resource == "smpl_block_rework" { "maps/smpl_block_01.ymap" } else { "smpl_block_01.ymap" };
      let bytes = fs::read(cache.join(format!("resources/{resource}/{stream_path}"))).unwrap();
      let mut document: serde_json::Value = serde_json::from_slice(
        &fs::read(cache.join(format!("diff/{resource}/{stream_path}.diff.json"))).unwrap(),
      )
      .unwrap();
      // Modification times are the run's; the documents are compared by content.
      for file in ["target", "base"] {
        document[file]["modified_seconds"] = 0.into();
        document[file]["modified_nanos"] = 0.into();
      }
      let sha = sha256(&bytes);
      let history = crate::core::source_cache::history::version_path(resource, stream_path, &sha);
      let diff = crate::core::source_cache::history::diff_path(resource, stream_path, &sha);
      let fingerprint = |file: &str| {
        serde_json::json!({"sha256": document[file]["sha256"], "size": document[file]["size"]})
      };
      let overwrite = serde_json::json!({
        "schema_version": crate::core::overwrite::StreamFileOverwrite::SCHEMA_VERSION,
        "target": fingerprint("target"),
        "base": fingerprint("base"),
        "skip": skip,
      });
      files.push((format!("asset/source-cache/{history}"), bytes));
      files.push((
        format!("asset/source-cache/{diff}"),
        serde_json::to_vec_pretty(&document).unwrap(),
      ));
      files.push((
        format!("asset/overwrite/resources/{resource}/{stream_path}.diff.json"),
        serde_json::to_vec_pretty(&overwrite).unwrap(),
      ));
    }
    let _ = fs::remove_dir_all(&root);
    files
  })
}
