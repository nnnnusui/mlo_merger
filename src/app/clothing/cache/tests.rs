use super::*;
use crate::common::json::write_json as write_manifest;
use crate::core::format::gamefile::struct_reader::{FieldName, Scalar};
use crate::core::game_archive::{CacheVersion, VanillaCacheManifest};

/// An in-memory structure tree standing in for META/PSO containers.
#[derive(Default)]
struct Node {
  kind: &'static str,
  scalars: HashMap<u32, Scalar>,
  lists: HashMap<u32, Vec<Scalar>>,
  nodes: HashMap<u32, Vec<Node>>,
}

impl StructReader for &Node {
  fn type_name(&self) -> String {
    self.kind.to_string()
  }

  fn scalar(
    &self,
    name: FieldName,
  ) -> io::Result<Option<Scalar>> {
    Ok(self.scalars.get(&name.hash).cloned())
  }

  fn vector(
    &self,
    _name: FieldName,
  ) -> io::Result<Option<Vec<f32>>> {
    Ok(None)
  }

  fn child(
    &self,
    name: FieldName,
  ) -> io::Result<Option<Self>> {
    Ok(self.nodes.get(&name.hash).and_then(|nodes| nodes.first()))
  }

  fn children(
    &self,
    name: FieldName,
  ) -> io::Result<Vec<Self>> {
    Ok(self.nodes.get(&name.hash).map_or_else(Vec::new, |nodes| nodes.iter().collect()))
  }

  fn values(
    &self,
    name: FieldName,
  ) -> io::Result<Vec<Scalar>> {
    Ok(self.lists.get(&name.hash).cloned().unwrap_or_default())
  }
}

fn drawable(texture_count: usize) -> Node {
  Node {
    kind: "CPVDrawblData",
    nodes: HashMap::from([(
      jenk_hash("aTexData"),
      (0..texture_count).map(|_| Node::default()).collect(),
    )]),
    ..Default::default()
  }
}

fn component(drawables: Vec<Node>) -> Node {
  Node {
    kind: "CPVComponentData",
    nodes: HashMap::from([(jenk_hash("aDrawblData3"), drawables)]),
    ..Default::default()
  }
}

#[test]
fn counting_names_components_by_slot_and_props_by_anchor() {
  let mut available = vec![Scalar::Int(255); 12];
  available[2] = Scalar::Int(0); // HAIR
  available[3] = Scalar::Int(1); // UPPR
  let anchor = |name: &str, props: usize| Node {
    kind: "CAnchorProps",
    scalars: HashMap::from([(jenk_hash("anchor"), Scalar::Text(name.to_string()))]),
    lists: HashMap::from([(jenk_hash("props"), vec![Scalar::Int(1); props])]),
    ..Default::default()
  };
  let prop = |texture_count: usize| Node {
    kind: "CPedPropMetaData",
    nodes: HashMap::from([(
      jenk_hash("texData"),
      (0..texture_count).map(|_| Node::default()).collect(),
    )]),
    ..Default::default()
  };
  let info = Node {
    kind: "CPedPropInfo",
    nodes: HashMap::from([
      (jenk_hash("aPropMetaData"), vec![prop(2), prop(0)]),
      (jenk_hash("aAnchors"), vec![anchor("ANCHOR_HEAD", 2), anchor("ANCHOR_EYES", 1)]),
    ]),
    ..Default::default()
  };
  let root = Node {
    kind: "CPedVariationInfo",
    lists: HashMap::from([(jenk_hash("availComp"), available)]),
    nodes: HashMap::from([
      (
        jenk_hash("aComponentData3"),
        vec![component(vec![drawable(3), drawable(1)]), component(vec![drawable(0)])],
      ),
      (jenk_hash("propInfo"), vec![info]),
    ]),
    ..Default::default()
  };
  let counts = count_root(&&root).unwrap();
  assert_eq!(
    counts.components,
    BTreeMap::from([
      (
        "HAIR".to_string(),
        ComponentCounts {
          drawables: 2,
          textures: 4
        }
      ),
      (
        "UPPR".to_string(),
        ComponentCounts {
          drawables: 1,
          textures: 0
        }
      ),
    ])
  );
  assert_eq!(
    counts.props,
    BTreeMap::from([("ANCHOR_HEAD".to_string(), 2), ("ANCHOR_EYES".to_string(), 1)])
  );
  assert_eq!(
    counts.totals,
    Totals {
      drawables: 3,
      textures: 4,
      props: 2,
      prop_textures: 2
    }
  );
}

#[test]
fn counting_labels_unmapped_components_and_accepts_missing_props() {
  let root = Node {
    kind: "CPedVariationInfo",
    nodes: HashMap::from([(jenk_hash("aComponentData3"), vec![component(vec![drawable(1)])])]),
    ..Default::default()
  };
  let counts = count_root(&&root).unwrap();
  assert_eq!(counts.components.keys().collect::<Vec<_>>(), ["component_0"]);
  assert!(counts.props.is_empty());
  assert_eq!(
    counts.totals,
    Totals {
      drawables: 1,
      textures: 1,
      props: 0,
      prop_textures: 0
    }
  );
}

#[test]
fn cache_reuses_matching_archives_and_rebuilds_changed_or_forced_ones() {
  let root = std::env::temp_dir().join(format!("clothing_cache_{}", std::process::id()));
  let _cleanup = Staging(root.clone());
  let archive = root.join("clothing");
  fs::create_dir_all(&archive).unwrap();
  let manifest = |label: &str| VanillaCacheManifest {
    format_version: 1,
    game_dir: root.join("game"),
    versions: vec![CacheVersion {
      id: format!("0000-{label}"),
      parent: None,
      archives: vec![],
      changes: BTreeMap::new(),
      file_kinds: Default::default(),
      unchanged: 0,
    }],
  };
  write_manifest(&archive.join("cache_info.json"), &manifest("base")).unwrap();
  let build = BuildClothingCache {
    clothing_dir: archive.clone(),
    output_dir: root.join("cache"),
    force: false,
  };
  build.run().unwrap();
  let info: ClothingCacheInfo = read_json(root.join("cache/cache_info.json")).unwrap();
  assert_eq!(info.format_version, CACHE_SCHEMA);
  assert_eq!(info.latest_version, "0000-base");
  let summary: serde_json::Value =
    serde_json::from_slice(&fs::read(root.join("cache/summary.json")).unwrap()).unwrap();
  assert_eq!(
    summary,
    serde_json::json!([{
      "version": "0000-base", "files": 0,
      "drawables": 0, "textures": 0, "props": 0, "prop_textures": 0,
    }])
  );
  // An empty stage writes no counts file; a current cache is kept, force and changes rebuild.
  assert_eq!(fs::read_dir(root.join("cache/counts")).unwrap().count(), 0);
  fs::write(root.join("cache/canary"), b"kept").unwrap();
  build.run().unwrap();
  assert!(root.join("cache/canary").is_file());
  BuildClothingCache {
    force: true,
    ..build.clone()
  }
  .run()
  .unwrap();
  assert!(!root.join("cache/canary").exists());
  fs::write(root.join("cache/canary"), b"kept").unwrap();
  write_manifest(&archive.join("cache_info.json"), &manifest("update")).unwrap();
  build.run().unwrap();
  assert!(!root.join("cache/canary").exists());
  let info: ClothingCacheInfo = read_json(root.join("cache/cache_info.json")).unwrap();
  assert_eq!(info.latest_version, "0000-update");
}

#[test]
fn cache_skips_creature_metadata_when_counting_variation_ymts() {
  let root = std::env::temp_dir().join(format!("creature_metadata_cache_{}", std::process::id()));
  let _cleanup = Staging(root.clone());
  let archive = root.join("clothing");
  let object = "0000-base/ymt/mp_creaturemetadata_m_test.ymt";
  fs::create_dir_all(archive.join("0000-base/ymt")).unwrap();
  fs::write(archive.join(object), b"not a CPedVariationInfo resource").unwrap();
  write_manifest(
    &archive.join("cache_info.json"),
    &serde_json::json!({
      "format_version": 1,
      "game_dir": root.join("game"),
      "versions": [{
        "id": "0000-base",
        "parent": null,
        "archives": [],
        "changes": {
          "mp_creaturemetadata_m_test.ymt": {
            "previous_sha256": null,
            "file": {
              "sha256": "00",
              "object": object,
              "source": "x.rpf/mp_creaturemetadata_m_test.ymt"
            }
          }
        },
        "file_kinds": {
          "mp_creaturemetadata_m_test.ymt": "creature_metadata"
        },
        "unchanged": 0
      }]
    }),
  )
  .unwrap();

  BuildClothingCache {
    clothing_dir: archive,
    output_dir: root.join("cache"),
    force: false,
  }
  .run()
  .unwrap();
  let summary: serde_json::Value =
    serde_json::from_slice(&fs::read(root.join("cache/summary.json")).unwrap()).unwrap();
  assert_eq!(
    summary,
    serde_json::json!([{
      "version": "0000-base",
      "files": 0,
      "drawables": 0,
      "textures": 0,
      "props": 0,
      "prop_textures": 0
    }])
  );
  assert_eq!(fs::read_dir(root.join("cache/counts")).unwrap().count(), 0);
}

#[test]
fn cache_fails_on_unreadable_definitions_and_keeps_missing_output_absent() {
  let root = std::env::temp_dir().join(format!("clothing_cache_bad_{}", std::process::id()));
  let _cleanup = Staging(root.clone());
  let archive = root.join("clothing");
  fs::create_dir_all(archive.join("0000-base/ymt")).unwrap();
  fs::write(archive.join("0000-base/ymt/bad.ymt"), b"not a resource").unwrap();
  let manifest = serde_json::json!({
    "format_version": 1,
    "game_dir": root.join("game"),
    "versions": [{
      "id": "0000-base", "parent": null, "archives": [], "unchanged": 0,
      "changes": {"bad.ymt": {"previous_sha256": null, "file": {
        "sha256": "00", "object": "0000-base/ymt/bad.ymt", "source": "x.rpf/bad.ymt",
      }}},
    }],
  });
  write_manifest(&archive.join("cache_info.json"), &manifest).unwrap();
  let error = BuildClothingCache {
    clothing_dir: archive,
    output_dir: root.join("cache"),
    force: false,
  }
  .run()
  .unwrap_err()
  .to_string();
  assert!(error.contains("0000-base/ymt/bad.ymt"), "{error}");
  assert!(!root.join("cache").exists());
}
