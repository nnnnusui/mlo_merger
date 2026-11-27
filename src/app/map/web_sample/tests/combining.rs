//! Combining sources: the README's examples 16 to 18.

use super::*;

/// Example 16: The merge report gives a change not applied for each reason.
#[test]
fn the_merge_report_names_each_reason() {
  let (workspace, _) = Workspace::latest("combining_report");
  let owned = |(resource, change, ignored): (&str, &str, &[(&str, &str)])| {
    let ignored = ignored.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect();
    (resource.to_owned(), change.to_owned(), ignored)
  };
  // The park: both bench resources move the bench alike, turn the fountain each its own way,
  // and the second turns the oak `smpl_park_tweaks` removes.
  assert_eq!(
    competitions(&workspace.report("smpl_block_02.ymap")),
    [
      ("smpl_bench_moved", "Modified", &[("smpl_bench_moved_too", "earlier_source_wins")][..]),
      ("smpl_bench_moved", "Modified", &[("smpl_bench_moved_too", "identical_duplicate")]),
      ("smpl_park_tweaks", "Removed", &[("smpl_bench_moved_too", "removal_wins")]),
    ]
    .map(owned)
  );
  // The winter square: `smpl_square_both` turns the birch in both generations, the winter one's
  // turn winning, and plants the flower bed in both, once.
  assert_eq!(
    competitions(&workspace.report("winter_smpl_square.ymap")),
    [
      ("smpl_square_both", "Added", &[("smpl_square_both", "same_addition_newer")][..]),
      ("smpl_square_both", "Modified", &[("smpl_square_both", "newer_generation_wins")]),
    ]
    .map(owned)
  );
  let square = workspace.merged_ymap("winter_smpl_square.ymap");
  assert_eq!(heading_of(entity_of(&square, guid::SQUARE_TREE)), 60.0);
  assert_eq!(guids(&square).iter().filter(|guid| **guid == guid::FLOWER_BED).count(), 1);
  let park = workspace.merged_ymap("smpl_block_02.ymap");
  assert_eq!(heading_of(entity_of(&park, guid::FOUNTAIN)), 10.0);
  assert_eq!(entity_of(&park, guid::PARK_BENCH).position.x, -7099.0);
  assert!(!guids(&park).contains(&guid::OAK_3));
}

/// Example 17: Two versions of `smpl_custom_props` ship the custom map, which the merge combines,
/// the later first: the shared sign once, and the crate once, as placed again under another GUID;
/// the label's parent index follows it.
#[test]
fn added_map_versions_combine() {
  let (workspace, _) = Workspace::latest("combining_added");
  let custom = workspace.merged_ymap("smpl_custom_01.ymap");
  assert_eq!(guids(&custom), [guid::BARREL, guid::SIGN, guid::CRATE_V2]);
  let label = workspace.merged_ymap("smpl_custom_02.ymap");
  assert_eq!(label.entities[0].parent_index, 2);
}

/// Example 18: `smpl_rounded_guids` stores the park's GUIDs rounded through a float: its entities
/// pair with vanilla's, and only its turn of the kiosk is merged, under the vanilla GUID.
#[test]
fn rounded_guids_pair_with_vanilla() {
  let (workspace, _) = Workspace::latest("combining_rounded");
  let diff = workspace.diff("smpl_rounded_guids", "smpl_block_02.ymap");
  let entities = diff["diffs"]["entities"].as_array().unwrap();
  assert_eq!(entities.len(), 1, "{entities:?}");
  let park = workspace.merged_ymap("smpl_block_02.ymap");
  assert_eq!(heading_of(entity_of(&park, guid::KIOSK)), 200.0);
  assert_eq!(
    guids(&park),
    [guid::OAK_1, guid::FOUNTAIN, guid::PARK_BIN, guid::PARK_BENCH, guid::KIOSK, guid::BIRCH]
  );
}
