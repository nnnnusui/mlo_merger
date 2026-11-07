use super::*;
use crate::core::config::gamebuild::GameBuilds;
use crate::core::pipeline::Pipeline;

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

  fn pipeline(
    &self,
    gamebuild: Option<&str>,
  ) -> crate::core::pipeline::PipelineSummary {
    let builds = GameBuilds::load(&self.asset("config.toml")).unwrap();
    Pipeline {
      source_dir: self.asset("source"),
      game_dir: self.asset("missing-game"),
      magic: self.asset("magic.dat"),
      vanilla_dir: self.asset("vanilla"),
      vanilla_cache_dir: self.asset("vanilla-cache"),
      source_cache_dir: self.asset("source-cache"),
      exclusions: None,
      merged_dir: self.asset("merged"),
      overwrite_dir: None,
      output_dir: self.asset("merged_mlo"),
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
    serde_json::from_slice(&fs::read(self.asset(path)).unwrap()).unwrap()
  }

  fn merged_ymap(
    &self,
    name: &str,
  ) -> Ymap {
    let names = hash_names().into_iter().collect();
    ymap_to_model(&fs::read(self.asset(&format!("merged/ymap/{name}"))).unwrap(), &names).unwrap()
  }

  /// The polygons of every geometry child of a merged collision.
  fn merged_polygons(
    &self,
    name: &str,
  ) -> usize {
    let bytes = fs::read(self.asset(&format!("merged/ybn/{name}"))).unwrap();
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
  assert_eq!(latest.deploy.copied, 10);

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
    "smpl_street_tweaks/winter_smpl_block_01.ymap",
    "smpl_street_tweaks/summer_smpl_block_01.ymap",
    "smpl_street_tweaks/winter_lodlights_small001.ymap",
    "smpl_block_rework/winter_smpl_block_01.ymap",
  ] {
    assert!(
      workspace.asset(&format!("source-cache/propagated/{diff}.diff.json")).is_file(),
      "{diff}"
    );
  }

  // Both resources' edits merge, and the houses follow their parents' removed SLOD.
  let block = workspace.merged_ymap("smpl_block_01.ymap");
  assert!(guids(&block).contains(&guid::PLANTER));
  assert!(!guids(&block).contains(&guid::BIN));
  assert!(block.car_generators.is_empty());
  assert_eq!(block.time_cycle_modifiers[0].percentage, 80.0);
  assert_eq!(parent_indices(&block), [0, 1]);
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
    fs::read(workspace.asset("source-cache/diff/smpl_custom_props/smpl_block_02.ymap.diff.json"))
      .unwrap();

  // Build 100 ends before `smpl_summer`: an older park and collision, no parasol generation.
  let build = workspace.pipeline(Some("100"));
  assert_eq!(
    workspace.json("vanilla-cache/cache_info.json")["latest_version"],
    "0003-smpl_patch_01"
  );
  let vanilla_cache = |path: &str| workspace.asset(&format!("vanilla-cache/latest/{path}"));
  assert_eq!(
    build.changes.vanilla_cache.modified,
    [vanilla_cache("ybn/smpl_block_01.ybn"), vanilla_cache("ymap/smpl_block_02.ymap")].into()
  );
  assert_eq!(
    build.changes.vanilla_cache.removed,
    [vanilla_cache("ymap/summer_smpl_block_01.ymap")].into()
  );
  assert!(!workspace.asset("merged/ymap/summer_smpl_block_01.ymap").exists());
  assert_eq!(workspace.merged_polygons("smpl_block_01.ybn"), 4);
  assert_ne!(
    fs::read(workspace.asset("source-cache/diff/smpl_custom_props/smpl_block_02.ymap.diff.json"))
      .unwrap(),
    park_diff
  );

  // `--read` writes each vanilla file under the stage holding it as of the build.
  let read = crate::core::read::Read {
    target: crate::core::read::ReadTarget::All,
    files: Vec::new(),
    source_dir: workspace.asset("source"),
    source_cache_dir: workspace.asset("source-cache"),
    vanilla_dir: workspace.asset("vanilla"),
    vanilla_cache_dir: workspace.asset("vanilla-cache"),
    gamebuild: None,
    output_dir: workspace.asset("read-cache"),
    force: false,
  }
  .run()
  .unwrap();
  assert!(
    workspace.asset("read-cache/vanilla/0003-smpl_patch_01/ymap/smpl_block_02.ymap.json").is_file()
  );
  // Seven resource YMAPs, the custom map included, and the eight vanilla YMAPs of build 100.
  assert_eq!(read.files.len(), 15);
}
