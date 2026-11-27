//! Generations: the README's examples 1 to 4.

use super::*;

/// Example 1: The block's edits carry to its DLC generations, and the square, which `smpl_winter`
/// replaces, is not deployed: its source is omitted and the winter square takes its changes.
#[test]
fn edits_carry_to_the_loaded_generations() {
  let (workspace, _) = Workspace::latest("generations_carry");
  let relationships = workspace.json("vanilla-cache/ymap_relationships.json");
  assert_eq!(
    relationships["children_by_parent"]["smpl_block_01_lod.ymap"],
    serde_json::json!([
      "smpl_block_01.ymap",
      "summer_smpl_block_01.ymap",
      "winter_smpl_block_01.ymap"
    ])
  );
  for diff in [
    "smpl_street_tweaks/0000-base/winter_smpl_block_01.ymap",
    "smpl_street_tweaks/0000-base/summer_smpl_block_01.ymap",
    "smpl_block_rework/0000-base/winter_smpl_block_01.ymap",
    "smpl_left_behind/0000-base/winter_smpl_square.ymap",
  ] {
    assert!(workspace.map(&format!("merged/generations/{diff}.diff.json")).is_file(), "{diff}");
  }
  // The bin `smpl_block_rework` removes and the roof's removal reach the winter block; the wall
  // strips stay added to the block, which the game loads too.
  let winter = workspace.merged_ymap("winter_smpl_block_01.ymap");
  assert!(!guids(&winter).contains(&guid::BIN));
  assert!(guids(&winter).contains(&guid::XMAS_TREE));
  let triangles = winter.occlude_models.iter().map(|model| model.triangles.len());
  assert_eq!(triangles.collect::<Vec<_>>(), [2]);

  // The square is superseded by the winter square.
  let generations = workspace.json("vanilla-cache/ymap_generations.json");
  assert_eq!(
    generations["families"]["smpl_square"]["successors"],
    serde_json::json!({"smpl_square.ymap": "winter_smpl_square.ymap"})
  );
  assert!(workspace.omitted().contains(&"smpl_left_behind/stream/smpl_square.ymap".to_owned()));
  assert!(!workspace.map("merged/ymap/smpl_square.ymap").exists());
  let index = workspace.json("merged/report/index.json");
  assert_eq!(index["files"]["smpl_square.ymap"]["output"], "omitted");
  assert_eq!(index["files"]["smpl_square.ymap"]["superseded_by"], "winter_smpl_square.ymap");
  let merge = workspace.json("merged/cache_info.json");
  assert_eq!(merge["files"]["smpl_square.ymap"]["superseded_by"], "winter_smpl_square.ymap");
  // The statue `smpl_left_behind` moves is moved in the winter square.
  let square = workspace.merged_ymap("winter_smpl_square.ymap");
  assert_eq!(entity_of(&square, guid::STATUE).position.x, -7089.0);
}

/// Example 2: `summer_smpl_block_01.ymap` is first shipped after build 100. Under it, the resource
/// shipping only that file is diffed against it, and its changes carried to the winter block.
#[test]
fn a_ymap_first_shipped_after_the_build() {
  let workspace = Workspace::new("generations_newer");
  workspace.pipeline(Some("100"));
  let index = workspace.json("vanilla-cache/generations.json");
  assert!(
    index["newer_than_build"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!("summer_smpl_block_01.ymap"))
  );
  let cache = workspace.json("vanilla-cache/cache_info.json");
  assert_eq!(
    cache["newer_files"]["summer_smpl_block_01.ymap"]["object"],
    "0004-smpl_summer/ymap/summer_smpl_block_01.ymap"
  );
  let diff = workspace.diff("smpl_summer_only", "summer_smpl_block_01.ymap");
  assert_eq!(diff["base"]["path"], "0004-smpl_summer/ymap/summer_smpl_block_01.ymap");
  // Not deployed, its flag's turn is carried to the winter block, which the build loads.
  assert!(!workspace.map("merged/ymap/summer_smpl_block_01.ymap").exists());
  let winter = workspace.merged_ymap("winter_smpl_block_01.ymap");
  assert_eq!(heading_of(entity_of(&winter, guid::FLAG)), 30.0);
  // The parasol it moves is in no generation the build loads.
  let left_behind = workspace.json("merged/generations/left_behind.json");
  assert!(left_behind.as_array().unwrap().iter().any(|change| {
    change["resource"] == "smpl_summer_only" && change["reason"] == "no_loaded_counterpart"
  }));

  // For the last stage, the file is merged as it is loaded.
  workspace.pipeline(None);
  let summer = workspace.merged_ymap("summer_smpl_block_01.ymap");
  assert_eq!(heading_of(entity_of(&summer, guid::FLAG)), 30.0);
  assert_eq!(entity_of(&summer, guid::PARASOL).position.y, 7088.0);
}

/// Example 3: The changes `smpl_left_behind` makes to the square that reach nothing in the winter
/// square, one of each reason.
#[test]
fn left_behind_names_each_reason() {
  let (workspace, _) = Workspace::latest("generations_left_behind");
  let left_behind = workspace.json("merged/generations/left_behind.json");
  let changes = left_behind
    .as_array()
    .unwrap()
    .iter()
    .filter(|change| change["file_name"] == "smpl_square.ymap")
    .map(|change| (change["change"].as_str().unwrap(), change["reason"].as_str().unwrap()))
    .collect::<Vec<_>>();
  assert_eq!(
    changes,
    [
      ("Removed", "already_absent"),
      ("Modified", "no_loaded_counterpart"),
      ("Added", "addition_not_carried")
    ]
  );
  let report = workspace.report("smpl_square.ymap");
  assert_eq!(report["left_behind"].as_array().unwrap().len(), 3);
  // The bench and the bin are not in the winter square, and the sign is not added to it.
  let square = workspace.merged_ymap("winter_smpl_square.ymap");
  for guid in [guid::SQUARE_BENCH, guid::SQUARE_BIN, guid::SQUARE_SIGN] {
    assert!(!guids(&square).contains(&guid));
  }
}

/// Example 4: `smpl_summer` splits the yard's collision into two YBNs: the post's removal goes to
/// both, which hold it, and the crate to the western one, which covers it.
#[test]
fn ybn_changes_reach_their_candidates() {
  let (workspace, _) = Workspace::latest("generations_ybn_candidates");
  let candidates = workspace.json("vanilla-cache/ybn_generations.json")["candidates"].clone();
  let files = candidates["smpl_yard.ybn"]["files"].as_array().unwrap();
  let files = files.iter().map(|file| file["file"].as_str().unwrap()).collect::<Vec<_>>();
  assert_eq!(files, ["summer_smpl_yard_0.ybn", "summer_smpl_yard_1.ybn"]);
  for candidate in ["summer_smpl_yard_0.ybn", "summer_smpl_yard_1.ybn"] {
    let document =
      format!("merged/generations/smpl_yard_split/0001-update/smpl_yard.ybn/{candidate}.diff.json");
    assert!(workspace.map(&document).is_file(), "{document}");
  }
  // West: ground (2), shed, the title update's bench and crate, the post removed; east: ground
  // (2) and woodpile.
  assert_eq!(workspace.merged_polygons("summer_smpl_yard_0.ybn"), 5);
  assert_eq!(workspace.merged_polygons("summer_smpl_yard_1.ybn"), 3);
  // The source YBN, which the game does not load, is not deployed.
  assert!(workspace.omitted().contains(&"smpl_yard_split/stream/smpl_yard.ybn".to_owned()));
  assert!(!workspace.deployed().iter().any(|path| path.ends_with("/smpl_yard.ybn")));
  let index = workspace.json("merged/report/index.json");
  assert_eq!(index["files"]["smpl_yard.ybn"]["superseded_by"], "summer_smpl_yard_0.ybn");

  // Build 100 loads the yard as the title update left it. The resource's YBN, made from the base
  // game's, is merged from its differences rather than deployed as it is, so the bench stays:
  // ground (2), shed, bench, woodpile and crate.
  workspace.pipeline(Some("100"));
  let diff = workspace.diff("smpl_yard_split", "smpl_yard.ybn");
  assert_eq!(diff["baseline"]["path"], "0000-base/ybn/smpl_yard.ybn");
  assert!(workspace.deployed().contains("ybn/merged/smpl_yard.ybn"));
  assert_eq!(workspace.merged_polygons("smpl_yard.ybn"), 6);
}
