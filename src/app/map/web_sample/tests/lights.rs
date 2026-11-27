//! LOD light pairs: the README's examples 19 to 21.

use super::*;

/// The positions of the distant lights of `map`.
fn positions(map: &Ymap) -> Vec<&str> {
  map.distant_lod_lights.items.iter().map(|light| light.position.as_str()).collect()
}

/// Example 19: `smpl_street_tweaks` removes a light from both YMAPs of the pair; the removal
/// reaches the winter pair too.
#[test]
fn a_removed_light_pair_reaches_the_generations() {
  let (workspace, _) = Workspace::latest("lights_removed");
  for (file, array) in [
    (
      "lodlights_small001.ymap",
      serde_json::json!({"lod_lights": [{"Removed": {"base_index": 0}}]}),
    ),
    (
      "distlodlights_small001.ymap",
      serde_json::json!({"distant_lod_lights": {
        "items": [{"Removed": {"base_index": 0}}], "num_street_lights": 2
      }}),
    ),
  ] {
    let diff = workspace.diff("smpl_street_tweaks", file);
    assert_eq!(diff["diffs"], array, "{file}");
  }
  // The winter pair's five lights lose those the resources remove from the first three, and
  // keep the title update's and `smpl_winter`'s.
  let winter = workspace.merged_ymap("winter_distlodlights_small001.ymap");
  assert_eq!(positions(&winter), ["-7055 7090 25", "-7110 7096 25"]);
  assert_eq!(workspace.merged_ymap("winter_lodlights_small001.ymap").lod_lights.len(), 2);
}

/// Example 20: `smpl_distant_only` ships only `distlodlights_small001.ymap`: the light it removes
/// is removed from the vanilla `lodlights_small001.ymap` through its pair document.
#[test]
fn distant_only_removes_from_both() {
  let (workspace, _) = Workspace::latest("lights_distant_only");
  let implied =
    workspace.json("source-cache/implied/smpl_distant_only/distlodlights_small001.ymap.json");
  assert_eq!(implied["partner"]["file_name"], "lodlights_small001.ymap");
  assert_eq!(
    implied["partner"]["diffs"],
    serde_json::json!({"lod_lights": [{"Removed": {"base_index": 1, "implied": true}}]})
  );
  let lights = workspace.merged_ymap("lodlights_small001.ymap");
  assert!(!lights.lod_lights.iter().any(|light| light.hash == (0x534D_1000 + 1).to_string()));
}

/// Example 21: `smpl_one_side_light` adds a light to `lodlights_small001.ymap` only: it is left
/// out, as its pair document records.
#[test]
fn a_light_added_to_one_side_is_left_out() {
  let (workspace, _) = Workspace::latest("lights_one_side");
  let implied =
    workspace.json("source-cache/implied/smpl_one_side_light/lodlights_small001.ymap.json");
  assert_eq!(implied["skip"], serde_json::json!({"lod_lights": [{"Added": {"target_index": 4}}]}));
  let lights = workspace.merged_ymap("lodlights_small001.ymap");
  assert!(!lights.lod_lights.iter().any(|light| light.hash == 0x5A5A_1000u32.to_string()));
}
