//! Overwrites: the README's example 24, with the stores' consistency.

use super::*;
use crate::app::map::overwrite_cache::{SourceUpdate, SourceUpdateOutcome};

/// The record of the overwrite of `resource`'s `stream_path` brought up to date.
fn update(
  workspace: &Workspace,
  resource: &str,
  stream_path: &str,
) -> SourceUpdate {
  let directory =
    workspace.map(&format!("overwrite-cache/source_updates/{resource}/{stream_path}"));
  let record = fs::read_dir(directory).unwrap().next().unwrap().unwrap().path();
  serde_json::from_slice(&fs::read(record).unwrap()).unwrap()
}

/// Example 24: The first pipeline run leaves out one overwrite of every reason, those written
/// against earlier versions of the resources' files included, and rewrites the one it can.
#[test]
fn ignored_overwrites_name_every_reason() {
  let (workspace, latest) = Workspace::latest("overwrites_ignored");
  use crate::app::map::overwrite::IgnoredOverwriteReason::*;
  let ignored = latest
    .warnings
    .ignored_overwrites
    .iter()
    .map(|overwrite| (overwrite.resource.as_deref(), overwrite.file.as_str(), overwrite.reason))
    .collect::<Vec<_>>();
  assert_eq!(
    ignored,
    [
      (Some("smpl_block_rework"), "maps/smpl_block_01.ymap", SourceChanged),
      (Some("smpl_custom_props"), "smpl_custom_old.ymap", MissingSource),
      (Some("smpl_park_tweaks"), "smpl_block_02.ymap", SourceChanged),
      (Some("smpl_street_tweaks"), "smpl_props.ytyp", UnsupportedFormat),
      (None, "0000-base/ymap/smpl_block_99.ymap", MissingVanilla),
      (None, "0001-update/ymap/smpl_block_02.ymap", VanillaChanged),
    ]
  );

  // The overwrite of `smpl_street_tweaks`, written against its earlier version, is rewritten
  // against the current one: it still reverts the planter, and no longer names the cone.
  let tweaks = update(&workspace, "smpl_street_tweaks", "smpl_block_01.ymap");
  assert_eq!(tweaks.outcome, SourceUpdateOutcome::Rewritten);
  let rewritten =
    workspace.json("overwrite/resources/smpl_street_tweaks/smpl_block_01.ymap.diff.json");
  let current = workspace.diff("smpl_street_tweaks", "smpl_block_01.ymap");
  let planter = super::super::earlier::added_index(&current, guid::PLANTER);
  assert_eq!(
    rewritten["skip"],
    serde_json::json!({"entities": [{"Added": {"target_index": planter}}]})
  );
  let block = workspace.merged_ymap("smpl_block_01.ymap");
  assert!(!guids(&block).contains(&guid::PLANTER));
  // That of `smpl_block_rework` reverted the lamp's colour, which the current version changes
  // otherwise, so it is left to be written again.
  let rework = update(&workspace, "smpl_block_rework", "maps/smpl_block_01.ymap");
  assert_eq!(rework.outcome, SourceUpdateOutcome::Manual);
  // That of `smpl_park_tweaks` reverted its removal of an oak and its bin; the current version
  // removes the other oak and places the bin elsewhere, so it is left too, its record listing
  // the removal and the addition it reverted.
  let park = update(&workspace, "smpl_park_tweaks", "smpl_block_02.ymap");
  assert_eq!(park.outcome, SourceUpdateOutcome::Manual);
  let reverted = serde_json::to_value(&park.reverted).unwrap()["entities"].clone();
  let variants = reverted
    .as_array()
    .unwrap()
    .iter()
    .map(|item| item.as_object().unwrap().keys().next().unwrap().clone())
    .collect::<Vec<_>>();
  assert_eq!(variants, ["Removed", "Added"]);
}

/// Every store is consistent with its metadata after a run. An overwrite changed since, and an
/// earlier version gone from the history, show in the overwrite cache's check.
#[test]
fn the_stores_check_clean() {
  let (workspace, latest) = Workspace::latest("overwrites_check");
  // The merged stream files, the custom maps, which the source cache collects too, and the files
  // only one resource ships as made from the latest vanilla versions.
  assert_eq!(latest.deploy.copied, 20);
  let check = || {
    crate::app::map::Check {
      vanilla_dir: workspace.asset("vanilla"),
      vanilla_cache_dir: workspace.map("vanilla-cache"),
      source_cache_dir: workspace.map("source-cache"),
      overwrite_dir: Some(workspace.map("overwrite")),
      overwrite_cache_dir: workspace.map("overwrite-cache"),
      merged_dir: workspace.map("merged"),
      output_dir: workspace.map("merged_mlo"),
      deep: true,
    }
    .run()
    .unwrap()
    .issues
    .into_iter()
    .map(|issue| (issue.store, issue.kind, issue.path))
    .collect::<Vec<_>>()
  };
  assert_eq!(check(), []);
  let rework = update(&workspace, "smpl_block_rework", "maps/smpl_block_01.ymap");
  let rework_overwrite =
    workspace.map("overwrite/resources/smpl_block_rework/maps/smpl_block_01.ymap.diff.json");
  let original = fs::read(&rework_overwrite).unwrap();
  fs::write(&rework_overwrite, [&original[..], b"\n"].concat()).unwrap();
  let earlier = workspace.map("source-cache").join(rework.before.source.as_ref().unwrap());
  fs::remove_file(&earlier).unwrap();
  let record = format!(
    "source_updates/smpl_block_rework/maps/smpl_block_01.ymap/{}.json",
    rework.before.sha256
  );
  let issues = check();
  assert!(issues.contains(&(
    "overwrite-cache".into(),
    crate::core::check::CheckIssueKind::Stale,
    "added_stream_files.json".into()
  )));
  assert!(issues.contains(&(
    "overwrite-cache".into(),
    crate::core::check::CheckIssueKind::Missing,
    record
  )));
}
