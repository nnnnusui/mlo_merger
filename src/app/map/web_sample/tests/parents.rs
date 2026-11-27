//! Entity parents: the README's examples 12 to 15.

use super::*;

/// Example 12: `smpl_block_rework` removes the empty SLOD from the block's parent, so the houses of
/// the block and of its generations point one entity lower.
#[test]
fn parent_indices_follow_a_removed_lod() {
  let (workspace, _) = Workspace::latest("parents_removed_lod");
  let parent_indices =
    |map: &Ymap| [guid::HOUSE_A, guid::HOUSE_B].map(|house| entity_of(map, house).parent_index);
  assert_eq!(workspace.merged_ymap("smpl_block_01_lod.ymap").entities.len(), 2);
  for name in ["smpl_block_01.ymap", "winter_smpl_block_01.ymap", "summer_smpl_block_01.ymap"] {
    assert_eq!(parent_indices(&workspace.merged_ymap(name)), [0, 1], "{name}");
  }
}

/// Example 13: `smpl_houses_gone` removes the two houses below the row's SLOD: their LODs lose all
/// their children and are removed, and the SLOD in turn.
#[test]
fn houses_gone_take_their_lods() {
  let (workspace, _) = Workspace::latest("parents_houses_gone");
  let row = workspace.merged_ymap("smpl_row.ymap");
  assert_eq!(guids(&row), [guid::HOUSE_E, guid::HOUSE_F]);
  // `smpl_orphaned_house` removes the third house's LOD; the fourth's stays.
  let row_lod = workspace.merged_ymap("smpl_row_lod.ymap");
  assert_eq!(guids(&row_lod), [guid::HOUSE_F_LOD]);
  assert_eq!(entity_of(&row, guid::HOUSE_F).parent_index, 0);
}

/// Example 14: The quay's crane points at the second of two LOD entities sharing a GUID: the
/// reference is kept by index.
#[test]
fn a_shared_parent_guid_is_kept_by_index() {
  let (workspace, _) = Workspace::latest("parents_shared_guid");
  let quay = workspace.merged_ymap("smpl_quay.ymap");
  let crane = entity_of(&quay, guid::CRANE);
  assert_eq!(crane.parent_index, 1);
  assert_eq!(heading_of(crane), 20.0);
}

/// Example 15: `smpl_orphaned_house` removes the third house's LOD: the house is detached.
#[test]
fn an_orphaned_house_is_detached() {
  let (workspace, _) = Workspace::latest("parents_orphaned");
  let row = workspace.merged_ymap("smpl_row.ymap");
  let house = entity_of(&row, guid::HOUSE_E);
  assert_eq!(house.parent_index, -1);
  assert_eq!(house.lod_level, "LODTYPES_DEPTH_ORPHANHD");
}
