//! Diff-cache native fixtures and inference/reconstruction regression tests.

use super::{
  BuildDiffCache, ModelReader,
  comparison::{ModelState, best_candidate, distance},
  io::{Scratch, cache_path, content_hash},
  metadata::{history, load_manifest},
  resource::generate_resource,
  types::{CandidateScore, Variant},
  vanilla::NativeVariants,
};
use crate::core::{
  format::ymap::diff::YmapDiff,
  format::{gamefile::meta_resource::jenk_hash, ymap::model::Ymap},
  vanilla_cache::{CacheVersion, CachedFile, VanillaCacheManifest, read_ymap, write_json},
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::BufReader;
use std::path::Path;

struct NativeVersionFixture {
  root: Scratch,
  manifest: VanillaCacheManifest,
  expected: Vec<Ymap>,
}

fn write_native_model(
  path: &Path,
  model: &Ymap,
) -> Ymap {
  use crate::core::format::{
    gamefile::{
      resource_convert::{NativeResourceFormat, xml_to_resource},
      test_support::sample_ymap_catalog,
    },
    ymap::xml::XmlYmap,
  };
  let xml = quick_xml::se::to_string(&XmlYmap::from(model.clone())).unwrap();
  let bytes = xml_to_resource(NativeResourceFormat::Ymap, &xml, sample_ymap_catalog()).unwrap();
  assert!(bytes.starts_with(b"RSC7"));
  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::write(path, bytes).unwrap();
  read_ymap(path).unwrap()
}

impl NativeVersionFixture {
  fn new(label: &str) -> Self {
    use crate::core::{format::ymap::xml::XmlYmap, vanilla_cache::FileChange};
    let root =
      Scratch(std::env::temp_dir().join(format!("vanilla_version_{label}_{}", std::process::id())));
    let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
    )))
    .unwrap();
    let mut base: Ymap = xml.into();
    base.content_flags |= 1 << 30;
    let mut expected = Vec::new();
    let mut versions: Vec<CacheVersion> = Vec::new();
    let mut previous_sha256 = None;
    for (index, id) in ["0000-base", "0001-patch", "0002-patch"].into_iter().enumerate() {
      let mut model = expected.last().cloned().unwrap_or_else(|| base.clone());
      if index > 0 {
        model.parent = format!("parent_{index}");
        model.entity_map.values_mut().next().unwrap().position.x += index as f32;
        model.physics_dictionaries.push(format!("physics_{index}"));
      }
      if index == 1 {
        model.entity_map.shift_remove_index(1).unwrap();
        model.content_flags &= !(1 << 30);
        model.flags ^= 1;
        model.name = "renamed_map".into();
        model.streaming_extents_min.x += 4.0;
      }
      let object = format!("{id}/ymap/map.ymap");
      let path = root.0.join(&object);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(&path, serde_json::to_vec(&model).unwrap()).unwrap();
      let decoded = model.clone();
      let sha256 = content_hash(&path).unwrap();
      versions.push(CacheVersion {
        id: id.into(),
        parent: index.checked_sub(1).map(|index| versions[index].id.clone()),
        archives: vec![],
        unchanged: 0,
        changes: BTreeMap::from([(
          "map.ymap".into(),
          FileChange {
            previous_sha256: previous_sha256.clone(),
            file: CachedFile {
              sha256: sha256.clone(),
              object,
              source: "fixture.rpf/map.ymap".into(),
            },
          },
        )]),
      });
      previous_sha256 = Some(sha256);
      expected.push(decoded);
    }
    versions.push(CacheVersion {
      id: "0003-unchanged".into(),
      parent: Some("0002-patch".into()),
      archives: vec![],
      changes: BTreeMap::new(),
      unchanged: 1,
    });
    let manifest = VanillaCacheManifest {
      format_version: 1,
      game_dir: root.0.join("missing-game"),
      versions,
    };
    write_json(&root.0.join("cache_info.json"), &manifest).unwrap();
    Self {
      root,
      manifest,
      expected,
    }
  }

  fn provider(&self) -> NativeVariants<'_> {
    NativeVariants {
      cache: &self.root.0,
      reader: |path| Ok(serde_json::from_reader(fs::File::open(path)?)?),
      models: BTreeMap::new(),
      ybn_models: BTreeMap::new(),
    }
  }
}

#[test]
fn vanilla_version_state_from_raw_stage_files_matches_complete_models() {
  let fixture = NativeVersionFixture::new("complete_states");
  let mut provider = fixture.provider();
  for (index, version) in fixture.manifest.versions.iter().enumerate() {
    let files = fixture.manifest.resolve_version(&version.id).unwrap();
    let actual = provider.model(&files["map.ymap"]).unwrap();
    assert_eq!(actual.model, fixture.expected[index.min(2)], "vanilla state at {}", version.id);
  }
  assert_eq!(provider.models.len(), 3);
}

#[test]
fn vanilla_version_state_uses_raw_file_from_latest_changed_stage() {
  let fixture = NativeVersionFixture::new("raw_latest");
  for version in &fixture.manifest.versions {
    for change in version.changes.values() {
      assert!(change.file.object.ends_with(".ymap"));
      assert!(fixture.root.0.join(&change.file.object).is_file());
    }
  }
  let files = fixture.manifest.resolve_version("0003-unchanged").unwrap();
  let actual = fixture
    .provider()
    .model(&files["map.ymap"])
    .expect("every changed YMAP stage must have its raw file");
  assert_eq!(actual.model, fixture.expected[2]);
  assert_eq!(
    distance(&actual.comparison, &ModelState::new(fixture.expected[2].clone()).unwrap().comparison),
    0
  );
  assert!(structdiff::StructDiff::diff(&actual.model, &fixture.expected[2]).is_empty());
}

#[test]
fn vanilla_raw_reader_rejects_missing_and_corrupted_files() {
  let fixture = NativeVersionFixture::new("invalid_raw");
  let file = &fixture.manifest.versions[2].changes["map.ymap"].file;
  let path = fixture.root.0.join(&file.object);
  fs::write(&path, b"corrupted raw bytes").unwrap();
  let mut provider = fixture.provider();
  let error = provider.model(file).err().unwrap().to_string();
  assert!(error.contains("hash mismatch"));
  fs::remove_file(path).unwrap();
  let error = provider.model(file).err().unwrap().to_string();
  assert!(error.contains("Raw vanilla artifact is missing"));
}

#[test]
fn vanilla_diff_replay_cannot_restore_entity_deletion_or_cleared_content_flags() {
  let fixture = NativeVersionFixture::new("lossy_replay");
  let mut before = fixture.expected[0].clone();
  let mask = 1u32 << 30;
  before.content_flags |= mask;
  let removed_guid = *before.entity_map.keys().next().unwrap();
  let mut expected = before.clone();
  expected.entity_map.shift_remove(&removed_guid).unwrap();
  expected.content_flags &= !mask;
  let bytes = serde_json::to_vec(&YmapDiff::extract_from(&before, &expected)).unwrap();
  let diff: YmapDiff = serde_json::from_slice(&bytes).unwrap();
  let actual = diff.apply_to(&before, None);
  assert!(!expected.entity_map.contains_key(&removed_guid));
  assert!(actual.entity_map.contains_key(&removed_guid));
  assert_eq!(actual.entity_map.len(), expected.entity_map.len() + 1);
  assert_eq!(expected.content_flags & mask, 0);
  assert_eq!(actual.content_flags & mask, mask);
  assert_ne!(actual, expected, "merge-oriented YmapDiff is not a lossless version patch");
}

#[test]
fn diff_cache_identifies_middle_version_with_least_changes_and_diffs_against_it() {
  crate::core::vanilla_cache::init_test_version_logger();
  let fixture = NativeVersionFixture::new("closest");
  let resource = fixture.root.0.join("resource");
  let mut target = fixture.expected[1].clone();
  target.entity_map.values_mut().next().unwrap().position.y += 8.0;
  let target = write_native_model(&resource.join("stream/map.ymap"), &target);
  let histories = history(&fixture.manifest).unwrap();
  let output = fixture.root.0.join("output");
  fs::create_dir(&output).unwrap();
  let report = generate_resource(
    &resource,
    "resource",
    "2026-10-04T00:00:00Z",
    &output,
    &fixture.manifest,
    &histories,
    &mut fixture.provider(),
  )
  .unwrap();
  let expected_scores: Vec<_> = fixture
    .expected
    .iter()
    .map(|model| {
      distance(
        &ModelState::new(model.clone()).unwrap().comparison,
        &ModelState::new(target.clone()).unwrap().comparison,
      )
    })
    .collect();
  assert!(expected_scores[1] > 0);
  assert!(expected_scores[1] < expected_scores[0]);
  assert!(expected_scores[1] < expected_scores[2]);
  assert_eq!(
    report.files[0]
      .candidates
      .iter()
      .map(|candidate| candidate.difference_count)
      .collect::<Vec<_>>(),
    expected_scores
  );
  assert_eq!(report.vanilla_version.as_deref(), Some("0001-patch"));
  assert_eq!(report.files[0].best_version, "0001-patch");
  assert_eq!(report.files[0].baseline_content_version, "0001-patch");
  assert_eq!(report.files[0].difference_count, expected_scores[1]);
  assert_eq!(
    report.files[0].baseline_sha256,
    fixture.manifest.versions[1].changes["map.ymap"].file.sha256
  );
  let diff: YmapDiff = serde_json::from_reader(BufReader::new(
    fs::File::open(output.join("resource").join(&report.files[0].diff)).unwrap(),
  ))
  .unwrap();
  assert_eq!(diff.apply_to(&fixture.expected[1], None), target);
}

#[test]
fn diff_cache_distance_is_symmetric_and_counts_model_changes() {
  let before = serde_json::json!({"entity_map": {"1": {"x": 1, "y": 2}}, "parent": "map"});
  let after =
    serde_json::json!({"entity_map": {"1": {"x": 3, "y": 4}, "2": {"x": 7}}, "parent": "map"});
  assert_eq!(distance(&before, &after), 3);
  assert_eq!(distance(&after, &before), 3);
  assert_eq!(distance(&before, &before), 0);
  assert_eq!(
    distance(
      &Value::String("parent".into()),
      &Value::String(format!("hash_{:08X}", jenk_hash("parent")))
    ),
    0
  );
  assert!(cache_path(Path::new("cache"), "../escape").is_err());
}

#[test]
fn diff_cache_selects_per_resource_latest_and_compares_its_cumulative_baseline() {
  use crate::core::format::ymap::xml::XmlYmap;
  use crate::core::vanilla_cache::{FileChange, init_test_version_logger};
  init_test_version_logger();
  let root = std::env::temp_dir().join(format!("diff_cache_run_{}", std::process::id()));
  let cache = root.join("cache");
  let resources = root.join("resources");
  let first = resources.join("[group]/first");
  let second = resources.join("second");
  fs::create_dir_all(first.join("stream")).unwrap();
  fs::create_dir_all(second.join("streams")).unwrap();
  let _cleanup = Scratch(root.clone());
  fs::write(first.join("fxmanifest.lua"), []).unwrap();
  fs::write(second.join("__resource.lua"), []).unwrap();
  let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
  )))
  .unwrap();
  let mut old: Ymap = xml.into();
  old.parent = "old_parent".into();
  let mut new = old.clone();
  new.parent = "new_parent".into();
  let reader: ModelReader =
    |path| Ok(serde_json::from_reader(BufReader::new(fs::File::open(path)?))?);
  let snapshot = |id: &str, name: &str, model: &Ymap| {
    let object = format!("{id}/ymap/{name}");
    let path = cache.join(&object);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, model).unwrap();
    CachedFile {
      sha256: content_hash(&path).unwrap(),
      object,
      source: format!("fixture.rpf/{name}"),
    }
  };
  let a0 = snapshot("0000-base", "a.ymap", &old);
  let b0 = snapshot("0000-base", "b.ymap", &old);
  let a1 = snapshot("0001-patch", "a.ymap", &new);
  let b2 = snapshot("0002-patch", "b.ymap", &new);
  let version =
    |id: &str, parent: Option<&str>, changes: Vec<(&str, Option<String>, CachedFile)>| {
      CacheVersion {
        id: id.into(),
        parent: parent.map(str::to_string),
        archives: vec![],
        unchanged: 0,
        changes: changes
          .into_iter()
          .map(|(name, previous_sha256, file)| {
            (
              name.into(),
              FileChange {
                previous_sha256,
                file,
              },
            )
          })
          .collect(),
      }
    };
  let manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: root.join("missing-game"),
    versions: vec![
      version(
        "0000-base",
        None,
        vec![
          ("a.ymap", None, a0.clone()),
          ("b.ymap", None, b0.clone()),
          (
            "collision.ybn",
            None,
            CachedFile {
              sha256: "3".repeat(64),
              object: "0000-base/ybn/collision.ybn".into(),
              source: "fixture.rpf/collision.ybn".into(),
            },
          ),
        ],
      ),
      version("0001-patch", Some("0000-base"), vec![("a.ymap", Some(a0.sha256.clone()), a1)]),
      version("0002-patch", Some("0001-patch"), vec![("b.ymap", Some(b0.sha256), b2)]),
      version("0003-empty", Some("0002-patch"), vec![]),
    ],
  };
  for version in &manifest.versions {
    fs::create_dir_all(cache.join(&version.id)).unwrap();
    write_json(&cache.join(&version.id).join("version_info.json"), version).unwrap();
  }
  write_json(&cache.join("cache_info.json"), &manifest).unwrap();
  write_json(&first.join("stream/a.ymap"), &old).unwrap();
  write_json(&first.join("stream/b.ymap"), &new).unwrap();
  write_json(&second.join("streams/a.ymap"), &new).unwrap();
  fs::write(first.join("stream/unmatched.ytyp"), b"unmatched").unwrap();
  fs::write(first.join("stream/collision.ybn"), b"unsupported collision data").unwrap();
  fs::write(first.join("outside.ymap"), b"not in stream").unwrap();
  let command = BuildDiffCache {
    input_dir: resources,
    output_dir: root.join("output"),
    vanilla_cache_dir: cache.clone(),
  };
  command.run_with_reader(reader).unwrap();
  let report: Value = serde_json::from_reader(BufReader::new(
    fs::File::open(command.output_dir.join("diff_cache_info.json")).unwrap(),
  ))
  .unwrap();
  assert_eq!(report["completed"], true);
  assert!(chrono::DateTime::parse_from_rfc3339(report["generated_at"].as_str().unwrap()).is_ok());
  assert_eq!(report["resources"][0]["vanilla_version"], "0002-patch");
  assert_eq!(report["resources"][1]["vanilla_version"], "0001-patch");
  assert_eq!(report["resources"][0]["scanned_files"], 4);
  assert_eq!(report["resources"][0]["unmatched_files"].as_array().unwrap().len(), 1);
  assert_eq!(
    report["resources"][0]["unsupported_files"],
    serde_json::json!(["stream/collision.ybn"])
  );
  let first_report = &report["resources"][0];
  assert_eq!(first_report["files"][0]["best_version"], "0000-base");
  assert_eq!(first_report["files"][0]["baseline_content_version"], "0001-patch");
  assert_eq!(first_report["files"][0]["difference_count"], 1);
  let diff: YmapDiff = serde_json::from_reader(BufReader::new(
    fs::File::open(command.output_dir.join("[group]/first/ymap/stream/a.ymap.diff.json")).unwrap(),
  ))
  .unwrap();
  assert_eq!(diff.apply_to(&new, None).parent, old.parent);
  assert!(
    fs::read_to_string(command.output_dir.join("create_cache.log"))
      .unwrap()
      .contains("Selected vanilla version for [group]/first: 0002-patch")
  );
  assert!(command.run_with_reader(reader).is_err());
  fs::remove_file(cache.join("cache_info.json")).unwrap();
  assert!(load_manifest(&cache).is_err());
  write_json(&cache.join("cache_info.json"), &manifest).unwrap();
  let single = BuildDiffCache {
    input_dir: first,
    output_dir: root.join("single"),
    vanilla_cache_dir: cache.clone(),
  };
  single.run_with_reader(reader).unwrap();
  assert!(single.output_dir.join("first/resource_info.json").is_file());
  let tied: Vec<_> = manifest
    .versions
    .iter()
    .take(2)
    .enumerate()
    .map(|(index, version)| Variant {
      index,
      version: version.id.clone(),
      file: a0.clone(),
    })
    .collect();
  let scores: Vec<_> = tied
    .iter()
    .map(|variant| CandidateScore {
      version: variant.version.clone(),
      sha256: variant.file.sha256.clone(),
      difference_count: 0,
    })
    .collect();
  assert_eq!(best_candidate(&scores, &tied).unwrap(), 1);
  fs::write(cache.join(&a0.object), b"corrupt").unwrap();
  let failed = BuildDiffCache {
    input_dir: single.input_dir,
    output_dir: root.join("failed"),
    vanilla_cache_dir: cache,
  };
  assert!(failed.run_with_reader(reader).is_err());
  let failure: Value = serde_json::from_reader(BufReader::new(
    fs::File::open(failed.output_dir.join("diff_cache_info.json")).unwrap(),
  ))
  .unwrap();
  assert_eq!(failure["completed"], false);
  assert!(failure["error"].as_str().unwrap().contains("hash mismatch"));
}

#[test]
fn diff_cache_generates_ybn_reports_from_raw_vanilla_history() {
  use super::types::YbnDiffReport;
  use crate::core::format::ybn::{diff::YbnDiff, read_ybn, xml::xml_to_ybn};
  use crate::core::vanilla_cache::{FileChange, init_test_version_logger};
  use sha2::{Digest, Sha256};

  init_test_version_logger();
  let root = std::env::temp_dir().join(format!("diff_cache_ybn_{}", std::process::id()));
  let _cleanup = Scratch(root.clone());
  let resource = root.join("resources").join("[group]").join("collision-resource");
  let cache = root.join("vanilla-cache");
  fs::create_dir_all(resource.join("stream")).unwrap();
  fs::create_dir_all(cache.join("0000-base/ybn")).unwrap();
  fs::create_dir_all(cache.join("0001-patch/ybn")).unwrap();
  fs::write(resource.join("fxmanifest.lua"), "fx_version 'cerulean'").unwrap();

  let source_xml = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
  ));
  let patch_xml = source_xml.replacen("<Margin value=\"0\" />", "<Margin value=\"0.1\" />", 1);
  let mod_xml = source_xml.replacen("<Margin value=\"0\" />", "<Margin value=\"0.2\" />", 1);
  let base_binary = xml_to_ybn(source_xml).unwrap();
  let patch_binary = xml_to_ybn(&patch_xml).unwrap();
  let mod_binary = xml_to_ybn(&mod_xml).unwrap();
  let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
  let base_sha = digest(&base_binary);
  let patch_sha = digest(&patch_binary);
  fs::write(cache.join("0000-base/ybn/collision.ybn"), &base_binary).unwrap();
  fs::write(cache.join("0001-patch/ybn/collision.ybn"), &patch_binary).unwrap();
  fs::write(resource.join("stream/collision.ybn"), &mod_binary).unwrap();

  let base_file = CachedFile {
    sha256: base_sha.clone(),
    object: "0000-base/ybn/collision.ybn".into(),
    source: "base.rpf/collision.ybn".into(),
  };
  let manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: root.join("missing-game"),
    versions: vec![
      CacheVersion {
        id: "0000-base".into(),
        parent: None,
        archives: vec!["base.rpf".into()],
        changes: BTreeMap::from([(
          "collision.ybn".into(),
          FileChange {
            previous_sha256: None,
            file: base_file,
          },
        )]),
        unchanged: 0,
      },
      CacheVersion {
        id: "0001-patch".into(),
        parent: Some("0000-base".into()),
        archives: vec!["patch.rpf".into()],
        changes: BTreeMap::from([(
          "collision.ybn".into(),
          FileChange {
            previous_sha256: Some(base_sha),
            file: CachedFile {
              sha256: patch_sha,
              object: "0001-patch/ybn/collision.ybn".into(),
              source: "patch.rpf/collision.ybn".into(),
            },
          },
        )]),
        unchanged: 0,
      },
    ],
  };
  for version in &manifest.versions {
    write_json(&cache.join(&version.id).join("version_info.json"), version).unwrap();
  }
  write_json(&cache.join("cache_info.json"), &manifest).unwrap();

  let command = BuildDiffCache {
    input_dir: root.join("resources"),
    output_dir: root.join("source-cache"),
    vanilla_cache_dir: cache,
  };
  command.run().unwrap();

  let report: Value = serde_json::from_reader(BufReader::new(
    fs::File::open(command.output_dir.join("diff_cache_info.json")).unwrap(),
  ))
  .unwrap();
  let resource_report = &report["resources"][0];
  assert_eq!(resource_report["vanilla_version"], "0001-patch");
  assert!(resource_report["unsupported_files"].as_array().is_none_or(Vec::is_empty));
  assert_eq!(resource_report["files"].as_array().unwrap().len(), 1);
  assert_eq!(resource_report["files"][0]["best_version"], "0001-patch");
  assert_eq!(resource_report["files"][0]["baseline_content_version"], "0001-patch");
  let diff_path =
    command.output_dir.join("[group]/collision-resource/ybn/stream/collision.ybn.diff.json");
  let changes: YbnDiffReport =
    serde_json::from_reader(BufReader::new(fs::File::open(diff_path).unwrap())).unwrap();
  let YbnDiffReport::Semantic {
    diff,
  } = changes
  else {
    panic!("expected semantic YBN diff");
  };
  assert_eq!(diff.bound_diffs.len(), 1);
  assert!(diff.polygon_diffs.is_empty());
  let baseline = read_ybn(&patch_binary).unwrap();
  let modified = read_ybn(&mod_binary).unwrap();
  assert!(!YbnDiff::extract_from(&baseline, &modified).unwrap().is_empty());
}

#[test]
#[ignore = "requires generated asset/diff-cache for brofx_mansion_06"]
fn diff_cache_real_brofx_output_is_readable_and_records_selected_version() {
  let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/diff-cache");
  let report: Value = serde_json::from_reader(BufReader::new(
    fs::File::open(output.join("diff_cache_info.json")).unwrap(),
  ))
  .unwrap();
  assert_eq!(report["completed"], true);
  assert!(chrono::DateTime::parse_from_rfc3339(report["generated_at"].as_str().unwrap()).is_ok());
  let resource = &report["resources"][0];
  assert_eq!(resource["resource"], "brofx_mansion_06");
  assert_eq!(resource["vanilla_version"], "0029-mpapartment");
  let files = resource["files"].as_array().unwrap();
  assert_eq!(files.len(), 2);
  for file in files {
    let candidates = file["candidates"].as_array().unwrap();
    let minimum = candidates
      .iter()
      .map(|candidate| candidate["difference_count"].as_u64().unwrap())
      .min()
      .unwrap();
    assert_eq!(file["best_difference_count"].as_u64(), Some(minimum));
    let path = output.join("brofx_mansion_06").join(file["diff"].as_str().unwrap());
    let _: YmapDiff =
      serde_json::from_reader(BufReader::new(fs::File::open(path).unwrap())).unwrap();
    assert_eq!(file["input_sha256"].as_str().unwrap().len(), 64);
  }
  let log = fs::read_to_string(output.join("create_cache.log")).unwrap();
  assert!(log.contains("Selected vanilla version for brofx_mansion_06: 0029-mpapartment"));
  assert!(log.contains("Candidate stream/apa_ch2_06_strm_2.ymap"));
  assert!(log.contains("Generating diff"));
  assert!(!output.join(".working").exists());
}
