//! YBN: the README's examples 22 and 23.

use super::*;

/// Example 22: The park's collision holds a box twice: the merge deploys it once, though no
/// resource changes it. The overwrite of `smpl_street_tweaks`'s collision leaves the planter's out.
#[test]
fn repeats_and_overwritten_polygons_are_left_out() {
  let (workspace, _) = Workspace::latest("ybn_repeats");
  assert_eq!(workspace.merged_polygons("smpl_block_02.ybn"), 3);
  assert_eq!(
    workspace.json("merged/report/ybn_repeats.json")["files"]["smpl_block_02.ybn"],
    serde_json::json!(1)
  );
  // Ground (2), house A and the kiosk; no planter.
  assert_eq!(workspace.merged_polygons("smpl_block_01.ybn"), 4);
}

/// Example 23: The vanilla archive's `materials.dat` names the sample's surface types, which
/// `--read` writes for each YBN material.
#[test]
fn read_names_surface_types() {
  let (workspace, _) = Workspace::latest("ybn_surfaces");
  let materials = workspace.json("vanilla-cache/materials.json");
  assert_eq!(
    materials["names"],
    serde_json::json!(["SMPL_DEFAULT", "SMPL_PAVING", "SMPL_LAWN", "SMPL_PLANKS"])
  );
  workspace.read(None);
  let document =
    fs::read_to_string(workspace.asset("read-cache/vanilla/0000-base/ybn/smpl_block_02.ybn.json"))
      .unwrap();
  let document: serde_json::Value = serde_json::from_str(&document).unwrap();
  assert!(document.to_string().contains(r#""surface":"SMPL_PAVING""#), "{document}");
}
