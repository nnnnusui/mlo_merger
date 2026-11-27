//! Older versions of a vanilla file (source baselines): the README's examples 5 to 8.

use super::*;

/// Example 5: `smpl_old_park` was made from the title update's park: its diff keeps its move of the
/// first oak and leaves out the bench and kiosk later stages added, and the oak they swapped.
#[test]
fn old_park_keeps_what_later_stages_changed() {
  let (workspace, _) = Workspace::latest("baselines_old_park");
  let diff = workspace.diff("smpl_old_park", "smpl_block_02.ymap");
  assert_eq!(diff["baseline"]["path"], "0001-update/ymap/smpl_block_02.ymap");
  assert_eq!(
    diff["diffs"]["entities"],
    serde_json::json!([{"Modified": {
      "base_index": 0, "target_index": 0, "changes": {"position": {"y": 7083.0}}
    }}])
  );
  let park = workspace.merged_ymap("smpl_block_02.ymap");
  assert_eq!(entity_of(&park, guid::OAK_1).position.y, 7083.0);
  assert!(guids(&park).contains(&guid::PARK_BENCH) && guids(&park).contains(&guid::KIOSK));
  assert!(!guids(&park).contains(&guid::OAK_2));
}

/// Example 6: The resources' collisions of the block were made before `smpl_summer` added the
/// kiosk's, which the merged collision keeps.
#[test]
fn collisions_made_before_the_kiosk_keep_it() {
  let (workspace, _) = Workspace::latest("baselines_collisions");
  for (resource, stream_path) in
    [("smpl_street_tweaks", "smpl_block_01.ybn"), ("smpl_block_rework", "maps/smpl_block_01.ybn")]
  {
    let diff = workspace.diff(resource, stream_path);
    assert_eq!(diff["baseline"]["path"], "0000-base/ybn/smpl_block_01.ybn", "{resource}");
  }
  // Ground (2), house A and the kiosk: the bollard is removed, and the overwrite leaves the
  // planter out.
  assert_eq!(workspace.merged_polygons("smpl_block_01.ybn"), 4);
}

/// Example 7: `smpl_old_lights` was made from the LOD light pair before the title update's fourth
/// light, which both YMAPs keep; the light it removes from one YMAP only is removed from both.
#[test]
fn old_lights_keep_the_later_light() {
  let (workspace, _) = Workspace::latest("baselines_old_lights");
  for file in ["lodlights_small001.ymap", "distlodlights_small001.ymap"] {
    let diff = workspace.diff("smpl_old_lights", file);
    assert_eq!(diff["baseline"]["path"], format!("0000-base/ymap/{file}"));
  }
  let distant = workspace.diff("smpl_old_lights", "distlodlights_small001.ymap");
  assert_eq!(
    distant["diffs"]["distant_lod_lights"]["items"],
    serde_json::json!([{"Removed": {"base_index": 2, "implied": true}}])
  );
  // Of the four lights, `smpl_street_tweaks`, `smpl_distant_only` and this resource remove the
  // first three; the title update's stays in both YMAPs.
  let lights = workspace.merged_ymap("lodlights_small001.ymap");
  let distant = workspace.merged_ymap("distlodlights_small001.ymap");
  assert_eq!(lights.lod_lights.len(), 1);
  assert_eq!(lights.lod_lights[0].hash, (0x534D_1000 + 3).to_string());
  assert_eq!(distant.distant_lod_lights.items.len(), 1);
  assert_eq!(distant.distant_lod_lights.items[0].position, "-7055 7090 25");
}

/// Example 8: `smpl_vanilla_rink` is `smpl_winter`'s rink before the title update's patches: it
/// changes nothing, so neither a merged rink nor the resource's is deployed, which would undo the
/// patch.
#[test]
fn vanilla_rink_is_not_deployed() {
  let (workspace, _) = Workspace::latest("baselines_vanilla_rink");
  let diff = workspace.diff("smpl_vanilla_rink", "winter_smpl_rink.ymap");
  assert_eq!(diff["baseline"]["path"], "0002-smpl_winter/ymap/winter_smpl_rink.ymap");
  // Only the extents differ, which merges leave out.
  assert!(diff["diffs"].get("entities").is_none());
  assert!(!workspace.map("merged/ymap/winter_smpl_rink.ymap").exists());
  assert!(
    workspace.omitted().contains(&"smpl_vanilla_rink/stream/winter_smpl_rink.ymap".to_owned())
  );
  assert!(!workspace.deployed().iter().any(|path| path.ends_with("/winter_smpl_rink.ymap")));
}
