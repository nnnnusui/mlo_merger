//! Game builds and DLC content: the README's examples 9 to 11.

use super::*;

/// Example 9: The title update patches `smpl_winter`'s rink once for build 100 and again for build
/// 200, the installed game's, so the two builds read different versions of it.
#[test]
fn title_update_patches_differ_per_build() {
  let workspace = Workspace::new("builds_patches");
  let manifest: serde_json::Value =
    serde_json::from_slice(&fs::read(workspace.asset("vanilla/cache_info.json")).unwrap()).unwrap();
  let builds = manifest["title_updates"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
  assert_eq!(builds, ["100", "200"]);
  let rink = |workspace: &Workspace| {
    let cache = workspace.json("vanilla-cache/cache_info.json");
    cache["files"]["winter_smpl_rink.ymap"]["object"].as_str().unwrap().to_owned()
  };
  workspace.pipeline(None);
  assert_eq!(rink(&workspace), "0002-smpl_winter/update_rpf/200/ymap/winter_smpl_rink.ymap");
  workspace.pipeline(Some("100"));
  assert_eq!(rink(&workspace), "0002-smpl_winter/update_rpf/100/ymap/winter_smpl_rink.ymap");
  // `--read` writes each build's version under the patch holding it.
  for build in ["100", "200"] {
    workspace.read(Some(build));
    let read = workspace.asset(&format!("read-cache/vanilla/0002-smpl_winter/update_rpf/{build}"));
    assert!(read.join("ymap/winter_smpl_rink.ymap.json").is_file(), "{build}");
  }
}

/// Example 10: `smpl_winter`'s change set invalidates the pier's archive, putting nothing in its
/// place, and replaces the square's; the installed game's patch of the pack holds a hut in an
/// archive the pack does not declare.
#[test]
fn dlc_activity_decides_what_the_game_loads() {
  let (workspace, _) = Workspace::latest("builds_dlc_activity");
  let activity = workspace.json("vanilla-cache/dlc_activity.json");
  let unloaded = |archive: &str| activity["archives"][archive]["unloaded"]["kind"].clone();
  assert_eq!(unloaded("platform:/levels/smpl/smpl_pier.rpf"), "invalidated");
  assert_eq!(unloaded("platform:/levels/smpl/smpl_square.rpf"), "invalidated");
  assert_eq!(unloaded("platform:/levels/smpl/smpl_yard.rpf"), "invalidated");
  assert_eq!(unloaded("dlcsmplwinter:/levels/smpl/smpl_hut.rpf"), "not_declared");
  assert_eq!(
    activity["archives"]["dlcsmplwinter:/levels/smpl/smpl_square.rpf"]["stands_for"],
    "platform:/levels/smpl/smpl_square.rpf"
  );
  let index = workspace.json("vanilla-cache/generations.json");
  let inactive = index["inactive"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
  assert_eq!(
    inactive,
    ["smpl_pier.ymap", "smpl_square.ymap", "smpl_yard.ybn", "winter_smpl_hut.ymap"]
  );
  // The pier stands for nothing: no generation holds its items.
  let generations = workspace.json("vanilla-cache/ymap_generations.json").to_string();
  assert!(!generations.contains("smpl_pier.ymap"));
}

/// Example 11: Builds 1, 100 and 200 end at different stages.
#[test]
fn game_builds_end_at_their_stages() {
  let workspace = Workspace::new("builds_stages");
  let latest_version = |workspace: &Workspace| {
    workspace.json("vanilla-cache/cache_info.json")["latest_version"].as_str().unwrap().to_owned()
  };
  workspace.pipeline(None);
  assert_eq!(latest_version(&workspace), "0004-smpl_summer");
  let park = workspace.diff("smpl_custom_props", "smpl_block_02.ymap");

  // Build 100 ends before `smpl_summer`: the latest files go back to the stages before it.
  let build = workspace.pipeline(Some("100"));
  assert_eq!(latest_version(&workspace), "0003-smpl_patch_01");
  let vanilla = |path: &str| workspace.asset(&format!("vanilla/{path}"));
  assert_eq!(
    build.changes.vanilla_cache.added,
    [
      vanilla("0000-base/ybn/smpl_block_01.ybn"),
      vanilla("0002-smpl_winter/update_rpf/100/ymap/winter_smpl_rink.ymap"),
      vanilla("0003-smpl_patch_01/ymap/smpl_block_02.ymap"),
    ]
    .into()
  );
  assert_eq!(
    build.changes.vanilla_cache.removed,
    [
      vanilla("0002-smpl_winter/update_rpf/200/ymap/winter_smpl_hut.ymap"),
      vanilla("0002-smpl_winter/update_rpf/200/ymap/winter_smpl_rink.ymap"),
      vanilla("0004-smpl_summer/ybn/smpl_block_01.ybn"),
      vanilla("0004-smpl_summer/ybn/summer_smpl_block_01.ybn"),
      vanilla("0004-smpl_summer/ybn/summer_smpl_yard_0.ybn"),
      vanilla("0004-smpl_summer/ybn/summer_smpl_yard_1.ybn"),
      vanilla("0004-smpl_summer/ymap/smpl_block_02.ymap"),
      vanilla("0004-smpl_summer/ymap/summer_smpl_block_01.ymap"),
    ]
    .into()
  );
  assert!(!workspace.map("merged/ymap/summer_smpl_block_01.ymap").exists());
  // The custom park is diffed again, against the patch's park.
  assert_ne!(workspace.diff("smpl_custom_props", "smpl_block_02.ymap"), park);
  // The summer collision's bench carries over to the generation the build loads, as the planter
  // does without the overwrite written against the summer collision; the kiosk it takes out is
  // not there, so its removal is left behind.
  assert_eq!(workspace.merged_polygons("smpl_block_01.ybn"), 5);
  let left_behind = workspace.json("merged/generations/left_behind.json");
  let polygons =
    left_behind.as_array().unwrap().iter().filter(|change| change["array"] == "polygons");
  assert_eq!(
    polygons.map(|change| change["file_name"].as_str().unwrap()).collect::<Vec<_>>(),
    ["summer_smpl_block_01.ybn"]
  );

  // Build 1 stops at the title update.
  workspace.pipeline(Some("1"));
  assert_eq!(latest_version(&workspace), "0001-update");
}
