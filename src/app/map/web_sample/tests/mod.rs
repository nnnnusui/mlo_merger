//! The web sample's tests: that `web/sample` holds what the module builds, and one test per
//! example of its README, each running the pipeline on a workspace of its own.

mod baselines;
mod builds;
mod combining;
mod generations;
mod lights;
mod overwrites;
mod parents;
mod ybn;

use super::*;
use crate::app::map::{Pipeline, PipelineSummary};
use crate::core::config::gamebuild::GameBuilds;
use crate::core::format::ymap::model::YmapEntity;

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
  // Those of the vanilla archive, the resources and the earlier versions in the source cache's
  // history.
  assert_eq!(checked, 69);
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

  /// A workspace on which the pipeline ran for the last stage.
  fn latest(name: &str) -> (Self, PipelineSummary) {
    let workspace = Self::new(name);
    let summary = workspace.pipeline(None);
    (workspace, summary)
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
  ) -> PipelineSummary {
    let builds = GameBuilds::load(&self.asset("config.toml")).unwrap();
    Pipeline {
      source_dir: self.map("source"),
      game_dir: self.asset("missing-game"),
      magic: self.asset("magic.dat"),
      game_storage: None,
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

  /// `--read` of every file, the vanilla files as `gamebuild` loads them.
  fn read(
    &self,
    gamebuild: Option<&str>,
  ) -> crate::app::map::read::ReadResult {
    let builds = GameBuilds::load(&self.asset("config.toml")).unwrap();
    crate::app::map::read::Read {
      target: crate::app::map::read::ReadTarget::All,
      files: Vec::new(),
      source_dir: self.map("source"),
      source_cache_dir: self.map("source-cache"),
      vanilla_dir: self.asset("vanilla"),
      vanilla_cache_dir: self.map("vanilla-cache"),
      gamebuild: gamebuild.map(|build| builds.resolve(build).unwrap()),
      output_dir: self.asset("read-cache"),
      force: false,
      version: None,
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

  /// The source cache's diff document of a resource's stream file.
  fn diff(
    &self,
    resource: &str,
    stream_path: &str,
  ) -> serde_json::Value {
    self.json(&format!("source-cache/diff/{resource}/{stream_path}.diff.json"))
  }

  /// The merge report of the vanilla file `name`, which the index lists.
  fn report(
    &self,
    name: &str,
  ) -> serde_json::Value {
    let index = self.json("merged/report/index.json");
    let report = index["files"][name]["report"].as_str().unwrap_or_else(|| panic!("{name}"));
    self.json(&format!("merged/{report}"))
  }

  /// The source paths the merge omits.
  fn omitted(&self) -> Vec<String> {
    let text = fs::read_to_string(self.map("merged/_omit.txt")).unwrap();
    text.lines().map(str::to_owned).collect()
  }

  /// The deployed stream files, by their path below `stream/`.
  fn deployed(&self) -> BTreeSet<String> {
    let stream = self.map("merged_mlo/stream");
    fs::WalkDir::new(&stream)
      .into_iter()
      .filter_map(Result::ok)
      .filter(|entry| entry.file_type().is_file())
      .map(|entry| entry.path().strip_prefix(&stream).unwrap().to_string_lossy().replace('\\', "/"))
      .collect()
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

/// The entity of `map` with `guid`.
fn entity_of(
  map: &Ymap,
  guid: u32,
) -> &YmapEntity {
  &map.entities[find(map, guid)]
}

/// The heading of `entity` in degrees, rounded.
fn heading_of(entity: &YmapEntity) -> f32 {
  (2.0 * entity.rotation.z.atan2(entity.rotation.w).to_degrees()).round()
}

/// The resource and kind of the change applied to an entity or item, with the resource of each
/// change not applied and why.
type Competed = (String, String, Vec<(String, String)>);

/// The changes a report gives for each entity and item competed for.
fn competitions(report: &serde_json::Value) -> Vec<Competed> {
  let mut found = Vec::new();
  for competed in report["entities"].as_array().into_iter().chain(report["items"].as_array()) {
    for change in competed {
      let applied = &change["applied"];
      let ignored = change["ignored"]
        .as_array()
        .unwrap()
        .iter()
        .map(|ignored| {
          (
            ignored["change"]["resource"].as_str().unwrap().to_owned(),
            ignored["reason"].as_str().unwrap().to_owned(),
          )
        })
        .collect();
      found.push((
        applied["resource"].as_str().unwrap().to_owned(),
        applied["change"].as_str().unwrap().to_owned(),
        ignored,
      ));
    }
  }
  found.sort();
  found
}
