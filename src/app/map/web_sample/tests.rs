use super::*;
use crate::app::map::Pipeline;
use crate::core::config::gamebuild::GameBuilds;

#[test]
fn web_sample_is_what_the_module_builds() {
  let root = sample_root();
  let files = sample_files();
  if std::env::var_os("MLO_WRITE_SAMPLES").is_some() {
    for stale in existing_files(&root) {
      fs::remove_file(root.join(stale)).unwrap();
    }
    for file in &files {
      let path = root.join(&file.path);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, &file.bytes).unwrap();
    }
    return;
  }
  let built = files.iter().map(|file| file.path.clone()).collect::<BTreeSet<_>>();
  assert_eq!(existing_files(&root), built, "rerun with MLO_WRITE_SAMPLES=1");
  for file in &files {
    assert!(
      fs::read(root.join(&file.path)).unwrap() == file.bytes,
      "{} is stale; rerun with MLO_WRITE_SAMPLES=1",
      file.path
    );
  }
}

/// A workspace holding the sample, as the page seeds it.
struct Workspace(PathBuf);

impl Workspace {
  fn new(name: &str) -> Self {
    let root = std::env::temp_dir().join(format!("mlo_web_sample_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    // Distinct modification times, as the page gives its files: fingerprints skip hashing a
    // file whose size and time are unchanged, and two stages of a file can share a size.
    for (index, file) in sample_files().into_iter().enumerate() {
      let path = root.join(&file.path);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(&path, &file.bytes).unwrap();
      let time =
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_767_225_600 + index as u64);
      fs::File::options().write(true).open(&path).unwrap().set_modified(time).unwrap();
    }
    Self(root)
  }

  fn asset(
    &self,
    path: &str,
  ) -> PathBuf {
    self.0.join("asset").join(path)
  }

  fn map(
    &self,
    path: &str,
  ) -> PathBuf {
    self.asset("map").join(path)
  }

  fn pipeline(
    &self,
    gamebuild: Option<&str>,
  ) -> crate::app::map::PipelineSummary {
    let builds = GameBuilds::load(&self.asset("config.toml")).unwrap();
    Pipeline {
      source_dir: self.map("source"),
      game_dir: self.asset("missing-game"),
      magic: self.asset("magic.dat"),
      vanilla_dir: self.asset("vanilla"),
      vanilla_cache_dir: self.map("vanilla-cache"),
      source_cache_dir: self.map("source-cache"),
      exclusions: None,
      merged_dir: self.map("merged"),
      overwrite_dir: Some(self.map("overwrite")),
      overwrite_cache_dir: self.map("overwrite-cache"),
      output_dir: self.map("merged_mlo"),
      gamebuild: gamebuild.map(|build| builds.resolve(build).unwrap()),
      force: false,
    }
    .run()
    .unwrap()
  }

  fn json(
    &self,
    path: &str,
  ) -> serde_json::Value {
    serde_json::from_slice(&fs::read(self.map(path)).unwrap()).unwrap()
  }

  fn merged_ymap(
    &self,
    name: &str,
  ) -> Ymap {
    let names = hash_names().into_iter().collect();
    ymap_to_model(&fs::read(self.map(&format!("merged/ymap/{name}"))).unwrap(), &names).unwrap()
  }

  /// The polygons of every geometry child of a merged collision.
  fn merged_polygons(
    &self,
    name: &str,
  ) -> usize {
    let bytes = fs::read(self.map(&format!("merged/ybn/{name}"))).unwrap();
    let bound = crate::core::format::ybn::read_ybn(&bytes).unwrap();
    bound
      .children
      .iter()
      .filter_map(|child| child.geometry.as_ref())
      .map(|g| g.polygons.len())
      .sum()
  }
}

impl Drop for Workspace {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

fn guids(map: &Ymap) -> Vec<u32> {
  map.entities.iter().map(|entity| entity.guid).collect()
}

fn parent_indices(map: &Ymap) -> Vec<i32> {
  [guid::HOUSE_A, guid::HOUSE_B].map(|house| map.entities[find(map, house)].parent_index).into()
}

#[test]
fn pipeline_runs_on_the_web_sample() {
  let workspace = Workspace::new("pipeline");
  let latest = workspace.pipeline(None);
  // Nine stream files merged or cloned, and the custom maps, which the source cache collects too.
  assert_eq!(latest.deploy.copied, 12);

  // Both versions of the custom map merge, the later first: the shared sign once, and the crate
  // once, as placed again under another GUID; the label's parent index follows it.
  let custom = workspace.merged_ymap("smpl_custom_01.ymap");
  assert_eq!(guids(&custom), [guid::BARREL, guid::SIGN, guid::CRATE_V2]);
  let label = workspace.merged_ymap("smpl_custom_02.ymap");
  assert_eq!(label.entities[0].parent_index, 2);

  // Relationships: the block and its generations are children of the LOD map, and the LOD lights
  // of their distant partner.
  let relationships = workspace.json("vanilla-cache/ymap_relationships.json");
  assert_eq!(
    relationships["children_by_parent"]["smpl_block_01_lod.ymap"],
    serde_json::json!([
      "smpl_block_01.ymap",
      "summer_smpl_block_01.ymap",
      "winter_smpl_block_01.ymap"
    ])
  );
  assert_eq!(
    relationships["children_by_parent"]["distlodlights_small001.ymap"],
    serde_json::json!(["lodlights_small001.ymap"])
  );

  // Generations: the prefixed copies pair with the base files, and edits carry over to them.
  let generations = workspace.json("vanilla-cache/generations.json").to_string();
  for file in
    ["winter_smpl_block_01.ymap", "summer_smpl_block_01.ymap", "winter_distlodlights_small001.ymap"]
  {
    assert!(generations.contains(file), "{file} is not a generation");
  }
  for diff in [
    "smpl_street_tweaks/0000-base/winter_smpl_block_01.ymap",
    "smpl_street_tweaks/0000-base/summer_smpl_block_01.ymap",
    "smpl_street_tweaks/0000-base/winter_lodlights_small001.ymap",
    "smpl_block_rework/0000-base/winter_smpl_block_01.ymap",
  ] {
    assert!(workspace.map(&format!("merged/generations/{diff}.diff.json")).is_file(), "{diff}");
  }

  // The overwrite of `smpl_street_tweaks`, written against its earlier version, is rewritten
  // against the current one: it still reverts the planter, and no longer names the cone. That of
  // `smpl_block_rework` reverted the lamp's colour, which the current version changes otherwise,
  // so it is left to be written again, and not applied.
  let update =
    |resource: &str, stream_path: &str| -> crate::app::map::overwrite_cache::SourceUpdate {
      let directory =
        workspace.map(&format!("overwrite-cache/source_updates/{resource}/{stream_path}"));
      let record = fs::read_dir(directory).unwrap().next().unwrap().unwrap().path();
      serde_json::from_slice(&fs::read(record).unwrap()).unwrap()
    };
  let tweaks = update("smpl_street_tweaks", "smpl_block_01.ymap");
  assert_eq!(tweaks.outcome, crate::app::map::overwrite_cache::SourceUpdateOutcome::Rewritten);
  let rewritten: serde_json::Value = serde_json::from_slice(
    &fs::read(workspace.map("overwrite/resources/smpl_street_tweaks/smpl_block_01.ymap.diff.json"))
      .unwrap(),
  )
  .unwrap();
  let current: serde_json::Value = serde_json::from_slice(
    &fs::read(workspace.map("source-cache/diff/smpl_street_tweaks/smpl_block_01.ymap.diff.json"))
      .unwrap(),
  )
  .unwrap();
  let planter = super::earlier::added_index(&current, guid::PLANTER);
  assert_eq!(
    rewritten["skip"],
    serde_json::json!({"entities": [{"Added": {"target_index": planter}}]})
  );
  let rework = update("smpl_block_rework", "maps/smpl_block_01.ymap");
  assert_eq!(rework.outcome, crate::app::map::overwrite_cache::SourceUpdateOutcome::Manual);
  // That of `smpl_park_tweaks` reverted its removal of an oak and its bin; the current version
  // removes the other oak and places the bin elsewhere, so it is left too, its record listing
  // the removal and the addition it reverted.
  let park = update("smpl_park_tweaks", "smpl_block_02.ymap");
  assert_eq!(park.outcome, crate::app::map::overwrite_cache::SourceUpdateOutcome::Manual);
  let reverted = serde_json::to_value(&park.reverted).unwrap()["entities"].clone();
  let variants = reverted
    .as_array()
    .unwrap()
    .iter()
    .map(|item| item.as_object().unwrap().keys().next().unwrap().clone())
    .collect::<Vec<_>>();
  assert_eq!(variants, ["Removed", "Added"]);
  // One overwrite left out for each reason.
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
      (Some("smpl_street_tweaks"), "smpl_block_01.ybn", UnsupportedFormat),
      (None, "0000-base/ymap/smpl_block_99.ymap", MissingVanilla),
      (None, "0001-update/ymap/smpl_block_02.ymap", VanillaChanged),
    ]
  );

  // Every store is consistent with its metadata. An overwrite changed since, and an earlier
  // version gone from the history, show in the overwrite cache's check.
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
  let rework_overwrite =
    workspace.map("overwrite/resources/smpl_block_rework/maps/smpl_block_01.ymap.diff.json");
  let original = fs::read(&rework_overwrite).unwrap();
  fs::write(&rework_overwrite, [&original[..], b"\n"].concat()).unwrap();
  let earlier = workspace.map("source-cache").join(rework.before.source.as_ref().unwrap());
  let earlier_bytes = fs::read(&earlier).unwrap();
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
  fs::write(&rework_overwrite, original).unwrap();
  fs::write(&earlier, earlier_bytes).unwrap();

  // Both resources' edits merge, but for the planter the overwrite reverts, and the houses follow
  // their parents' removed SLOD.
  let block = workspace.merged_ymap("smpl_block_01.ymap");
  assert!(!guids(&block).contains(&guid::PLANTER));
  assert!(!guids(&block).contains(&guid::BIN));
  assert!(block.car_generators.is_empty());
  assert_eq!(block.time_cycle_modifiers[0].percentage, 80.0);
  assert_eq!(parent_indices(&block), [0, 1]);
  // The wall without its roof and both resources' wall strips, split where the next triangle
  // would pass 255 vertices.
  let triangles = block.occlude_models.iter().map(|model| model.triangles.len());
  assert_eq!(triangles.collect::<Vec<_>>(), [250, 32]);
  assert_eq!(250 + 32, 2 + 4 * WALL_PANELS);
  assert_eq!(workspace.merged_ymap("smpl_block_01_lod.ymap").entities.len(), 2);
  let winter = workspace.merged_ymap("winter_smpl_block_01.ymap");
  assert!(!guids(&winter).contains(&guid::BIN));
  assert!(guids(&winter).contains(&guid::XMAS_TREE));
  assert_eq!(parent_indices(&winter), [0, 1]);
  assert_eq!(workspace.merged_ymap("lodlights_small001.ymap").lod_lights.len(), 2);
  assert_eq!(workspace.merged_ymap("winter_lodlights_small001.ymap").lod_lights.len(), 3);
  // Ground (2), house A and the planter: the bollard is removed, and so is the kiosk `smpl_summer`
  // added, as both resources were made from the collision before it.
  assert_eq!(workspace.merged_polygons("smpl_block_01.ybn"), 4);
  let park_diff =
    fs::read(workspace.map("source-cache/diff/smpl_custom_props/smpl_block_02.ymap.diff.json"))
      .unwrap();

  // Build 100 ends before `smpl_summer`: an older park and collision, no parasol generation.
  let build = workspace.pipeline(Some("100"));
  assert_eq!(
    workspace.json("vanilla-cache/cache_info.json")["latest_version"],
    "0003-smpl_patch_01"
  );
  // The latest files go back to the stages before `smpl_summer`.
  let vanilla = |path: &str| workspace.asset(&format!("vanilla/{path}"));
  assert_eq!(
    build.changes.vanilla_cache.added,
    [
      vanilla("0000-base/ybn/smpl_block_01.ybn"),
      vanilla("0003-smpl_patch_01/ymap/smpl_block_02.ymap")
    ]
    .into()
  );
  assert_eq!(
    build.changes.vanilla_cache.removed,
    [
      vanilla("0004-smpl_summer/ybn/smpl_block_01.ybn"),
      vanilla("0004-smpl_summer/ymap/smpl_block_02.ymap"),
      vanilla("0004-smpl_summer/ymap/summer_smpl_block_01.ymap"),
    ]
    .into()
  );
  assert!(build.changes.vanilla_cache.modified.is_empty());
  assert!(!workspace.map("merged/ymap/summer_smpl_block_01.ymap").exists());
  assert_eq!(workspace.merged_polygons("smpl_block_01.ybn"), 4);
  assert_ne!(
    fs::read(workspace.map("source-cache/diff/smpl_custom_props/smpl_block_02.ymap.diff.json"))
      .unwrap(),
    park_diff
  );

  // `--read` writes each vanilla file under the stage holding it as of the build.
  let read = crate::app::map::read::Read {
    target: crate::app::map::read::ReadTarget::All,
    files: Vec::new(),
    source_dir: workspace.map("source"),
    source_cache_dir: workspace.map("source-cache"),
    vanilla_dir: workspace.asset("vanilla"),
    vanilla_cache_dir: workspace.map("vanilla-cache"),
    gamebuild: None,
    output_dir: workspace.asset("read-cache"),
    force: false,
    version: None,
  }
  .run()
  .unwrap();
  assert!(
    workspace.asset("read-cache/vanilla/0003-smpl_patch_01/ymap/smpl_block_02.ymap.json").is_file()
  );
  // Ten resource YMAPs, the custom maps included, and the eight vanilla YMAPs of build 100.
  assert_eq!(read.files.len(), 18);
}

#[test]
fn every_stream_file_covers_the_shared_position() {
  let [x, y, z] = EVERY_FILE_POSITION.map(f64::from);
  let names = hash_names().into_iter().collect();
  let mut checked = 0;
  for file in sample_files() {
    let bounds = if file.path.ends_with(".ymap") {
      let map = ymap_to_model(&file.bytes, &names).unwrap();
      let (min, max) = (&map.entities_extents_min, &map.entities_extents_max);
      ([min.x, min.y, min.z].map(f64::from), [max.x, max.y, max.z].map(f64::from))
    } else if file.path.ends_with(".ybn") {
      let root = crate::core::format::ybn::read_ybn_bound_tree(&file.bytes).unwrap();
      crate::app::map::find::ybn::world_bounds(&root).unwrap().unwrap()
    } else {
      continue;
    };
    let (min, max) = bounds;
    assert!(
      (min[0]..=max[0]).contains(&x)
        && (min[1]..=max[1]).contains(&y)
        && (min[2]..=max[2]).contains(&z),
      "{} spans {min:?} to {max:?}",
      file.path
    );
    checked += 1;
  }
  // The earlier versions kept in the source cache's history included.
  assert_eq!(checked, 30);
}
