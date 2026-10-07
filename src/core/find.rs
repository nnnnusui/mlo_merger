use crate::core::format::{
  gamefile::{
    meta_resource::MetaResource, meta_xml::ymap_to_model_with_entities, resource_file::Rsc7Resource,
  },
  ymap::{model::YmapEntity, xml::XmlYmapEntity},
};
use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  fs,
  io::BufReader,
  path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Condition selecting entities from merged and recorded pre-merge YMAPs.
#[derive(Debug, Clone, PartialEq)]
pub enum EntityQuery {
  /// Select every occurrence of this unsigned entity GUID.
  Guid(u32),
  /// Select entity positions within an inclusive three-dimensional radius.
  Position {
    /// Center coordinates, rounded to YMAP's f32 coordinate precision.
    position: [f64; 3],
    /// Finite nonnegative distance from the center.
    radius: f64,
  },
}

impl EntityQuery {
  fn validate(&self) -> Result<()> {
    if let Self::Position {
      position,
      radius,
    } = self
      && (!position.iter().all(|value| value.is_finite() && (*value as f32).is_finite())
        || !radius.is_finite()
        || *radius < 0.0)
    {
      return Err("Entity position and radius must be finite; --round must be nonnegative".into());
    }
    Ok(())
  }

  fn contains(
    &self,
    entity: &YmapEntity,
  ) -> bool {
    match self {
      Self::Guid(guid) => entity.guid == *guid,
      Self::Position {
        position,
        radius,
      } => {
        let center = position.map(|value| value as f32 as f64);
        let delta = [
          entity.position.x as f64 - center[0],
          entity.position.y as f64 - center[1],
          entity.position.z as f64 - center[2],
        ];
        delta[0].hypot(delta[1]).hypot(delta[2]) <= *radius
      }
    }
  }
}

/// Searches existing merged binaries by GUID or position and renders matching entities as XML.
#[derive(Debug, Clone)]
pub struct FindEntity {
  /// Validated GUID or three-dimensional position condition.
  pub query: EntityQuery,
  /// Directory containing merged YMAPs.
  pub merged_dir: PathBuf,
  /// Optional case-insensitive filename glob.
  pub filter: Option<String>,
  /// Include recorded vanilla and source inputs.
  pub diff_all: bool,
}

/// Searches existing merged binaries for an entity GUID and renders matching data as XML.
#[derive(Debug, Clone)]
pub struct FindEntityGuid {
  /// Entity GUID, including the unsigned range above i32::MAX.
  pub guid: u32,
  /// Directory containing grouped or legacy flat merged YMAPs.
  pub merged_dir: PathBuf,
  /// Optional case-insensitive filename glob filter.
  pub filter: Option<String>,
  /// Include recorded vanilla and source inputs without regenerating artifacts.
  pub diff_all: bool,
}

#[derive(Deserialize)]
struct MergeCache {
  format_version: u32,
  files: BTreeMap<String, Record>,
}
#[derive(Deserialize)]
struct Record {
  merge_sources: Vec<Input>,
  vanilla: Option<Input>,
}
#[derive(Deserialize)]
struct Input {
  path: PathBuf,
  resource: Option<String>,
  original_path: Option<PathBuf>,
  fingerprint: InputFingerprint,
}
#[derive(Deserialize)]
struct InputFingerprint {
  sha256: String,
  size: u64,
}

#[derive(Serialize)]
#[serde(rename = "EntityGuidSearch")]
struct SearchResult {
  #[serde(rename = "@guid", skip_serializing_if = "Option::is_none")]
  guid: Option<u32>,
  #[serde(rename = "@position", skip_serializing_if = "Option::is_none")]
  position: Option<String>,
  #[serde(rename = "@round", skip_serializing_if = "Option::is_none")]
  radius: Option<f64>,
  #[serde(rename = "File")]
  files: Vec<FileResult>,
}
#[derive(Serialize)]
struct FileResult {
  #[serde(rename = "@name")]
  name: String,
  #[serde(rename = "Data")]
  data: Vec<EntityData>,
}
#[derive(Serialize)]
struct EntityData {
  #[serde(rename = "@stage")]
  stage: &'static str,
  #[serde(rename = "@path", skip_serializing_if = "Option::is_none")]
  path: Option<String>,
  #[serde(rename = "@resource", skip_serializing_if = "Option::is_none")]
  resource: Option<String>,
  #[serde(rename = "@originalPath", skip_serializing_if = "Option::is_none")]
  original_path: Option<String>,
  #[serde(rename = "@found")]
  found: bool,
  #[serde(rename = "Item")]
  entities: Vec<XmlYmapEntity>,
}

impl FindEntityGuid {
  /// Returns one XML document containing every occurrence, including duplicate GUID entries.
  ///
  /// ```no_run
  /// let xml = mlo_merger::core::find::FindEntityGuid {
  ///   guid: 2443198849, merged_dir: "asset/merged".into(),
  ///   filter: Some("lr_cs4_10_strm_0.ymap".into()), diff_all: true,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<String> {
    FindEntity {
      query: EntityQuery::Guid(self.guid),
      merged_dir: self.merged_dir.clone(),
      filter: self.filter.clone(),
      diff_all: self.diff_all,
    }
    .run()
  }
}

impl FindEntity {
  /// Returns a read-only XML search result using the same condition for all stages.
  ///
  /// ```no_run
  /// use mlo_merger::core::find::{EntityQuery, FindEntity};
  /// let xml = FindEntity {
  ///   query: EntityQuery::Position { position: [3.5, 4.2, 0.0], radius: 1.0 },
  ///   merged_dir: "asset/merged".into(), filter: None, diff_all: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<String> {
    self.query.validate()?;
    let merged = self.merged_dir.canonicalize()?;
    let filter = filename_filter(self.filter.as_deref())?;
    let mut paths = BTreeMap::new();
    for entry in walkdir::WalkDir::new(&merged).follow_links(false) {
      let entry = entry?;
      if !entry.file_type().is_file() {
        continue;
      }
      let path = entry.path();
      if path.extension().is_none_or(|extension| !extension.eq_ignore_ascii_case("ymap")) {
        continue;
      }
      let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("YMAP filename is not UTF-8")?
        .to_ascii_lowercase();
      if !matches(&filter, &name) {
        continue;
      }
      if paths.insert(name.clone(), path.to_path_buf()).is_some() {
        return Err(format!("Duplicate merged YMAP filename: {name}").into());
      }
    }
    let cache = if self.diff_all {
      let cache: MergeCache = serde_json::from_reader(BufReader::new(fs::File::open(
        merged.join("merge_cache_info.json"),
      )?))?;
      if cache.format_version != 1 {
        return Err("Unsupported merge cache schema for --diff-all".into());
      }
      Some(cache)
    } else {
      None
    };
    let mut names = paths.keys().cloned().collect::<BTreeSet<_>>();
    if let Some(cache) = &cache {
      names.extend(
        cache
          .files
          .keys()
          .filter(|name| name.ends_with(".ymap") && matches(&filter, name))
          .cloned(),
      );
    }
    let mut files = Vec::new();
    for name in names {
      let mut data = Vec::new();
      if let Some(path) = paths.get(&name) {
        data.push(read_entities(path, &self.query, "merged", None)?);
      } else {
        data.push(EntityData {
          stage: "merged",
          path: None,
          resource: None,
          original_path: None,
          found: false,
          entities: Vec::new(),
        });
      }
      if let Some(cache) = &cache {
        let record = cache
          .files
          .get(&name)
          .ok_or_else(|| format!("No pre-merge provenance recorded for {name}"))?;
        if let Some(vanilla) = &record.vanilla {
          data.push(read_entities(&vanilla.path, &self.query, "vanilla", Some(vanilla))?);
        }
        for source in &record.merge_sources {
          data.push(read_entities(&source.path, &self.query, "source", Some(source))?);
        }
      }
      if data.iter().any(|data| data.found) {
        files.push(FileResult {
          name,
          data,
        });
      }
    }
    let result = search_result(&self.query, files);
    let mut xml = String::new();
    let root = match self.query {
      EntityQuery::Guid(_) => "EntityGuidSearch",
      EntityQuery::Position {
        ..
      } => "EntityPositionSearch",
    };
    let mut serializer = quick_xml::se::Serializer::with_root(&mut xml, Some(root))?;
    serializer.indent(' ', 2);
    result.serialize(serializer)?;
    Ok(xml)
  }
}

fn search_result(
  query: &EntityQuery,
  files: Vec<FileResult>,
) -> SearchResult {
  match query {
    EntityQuery::Guid(guid) => SearchResult {
      guid: Some(*guid),
      position: None,
      radius: None,
      files,
    },
    EntityQuery::Position {
      position,
      radius,
    } => SearchResult {
      guid: None,
      position: Some(format!("{},{},{}", position[0], position[1], position[2])),
      radius: Some(*radius),
      files,
    },
  }
}

fn filename_filter(pattern: Option<&str>) -> Result<Option<GlobMatcher>> {
  pattern
    .map(|pattern| {
      if pattern.contains(['/', '\\'])
        || Path::new(pattern).components().count() != 1
        || !matches!(Path::new(pattern).components().next(), Some(Component::Normal(_)))
      {
        return Err("--filter requires a filename glob, not a path".into());
      }
      Ok(
        GlobBuilder::new(pattern)
          .case_insensitive(true)
          .literal_separator(true)
          .build()?
          .compile_matcher(),
      )
    })
    .transpose()
}

fn matches(
  filter: &Option<GlobMatcher>,
  name: &str,
) -> bool {
  filter.as_ref().is_none_or(|filter| filter.is_match(name))
}

fn read_entities(
  path: &Path,
  query: &EntityQuery,
  stage: &'static str,
  input: Option<&Input>,
) -> Result<EntityData> {
  let bytes = fs::read(path).map_err(|error| {
    std::io::Error::new(
      error.kind(),
      format!("Cannot read {stage} YMAP {}: {error}", path.display()),
    )
  })?;
  if let Some(input) = input
    && (input.fingerprint.size != bytes.len() as u64
      || input.fingerprint.sha256 != format!("{:x}", Sha256::digest(&bytes)))
  {
    return Err(
      format!(
        "Pre-merge input changed since merge: {}; rerun merge before using --diff-all",
        path.display()
      )
      .into(),
    );
  }
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes)?)?;
  let names: HashMap<_, _> = meta.hash_names();
  let (_, entities) = ymap_to_model_with_entities(&bytes, &names)?;
  Ok(entity_data(path, query, stage, input, entities))
}

fn entity_data(
  path: &Path,
  query: &EntityQuery,
  stage: &'static str,
  input: Option<&Input>,
  entities: Vec<YmapEntity>,
) -> EntityData {
  let entities = entities
    .into_iter()
    .filter(|entity| query.contains(entity))
    .map(XmlYmapEntity::from)
    .collect::<Vec<_>>();
  EntityData {
    stage,
    path: Some(path.to_string_lossy().replace('\\', "/")),
    resource: input.and_then(|input| input.resource.clone()),
    original_path: input
      .and_then(|input| input.original_path.as_ref())
      .map(|path| path.to_string_lossy().replace('\\', "/")),
    found: !entities.is_empty(),
    entities,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn entity_position_search_includes_the_3d_boundary_and_all_occurrences() {
    use crate::core::common::position::Position;
    let xml: crate::core::format::ymap::xml::XmlYmap = quick_xml::de::from_str(include_str!(
      concat!(env!("CARGO_MANIFEST_DIR"), "/docs/sample/parent_refs/vanilla_parent.ymap.xml")
    ))
    .unwrap();
    let model: crate::core::format::ymap::model::Ymap = xml.into();
    let sample = model.entity_map.values().next().unwrap();
    let positions = [
      [3.5, 4.2, 0.0],
      [4.5, 4.2, 0.0],
      [3.5, 4.2, 1.0],
      [3.5, 4.2, -1.0],
      [4.5001, 4.2, 0.0],
      [3.5, 4.2, 1.001],
      [4.5, 5.2, 0.0],
    ];
    let entities = positions
      .into_iter()
      .enumerate()
      .map(|(index, position)| {
        let mut entity = sample.clone();
        entity.guid = index as u32;
        entity.position = Position {
          x: position[0],
          y: position[1],
          z: position[2],
        };
        entity
      })
      .collect::<Vec<_>>();
    let query = EntityQuery::Position {
      position: [3.5, 4.2, 0.0],
      radius: 1.0,
    };
    query.validate().unwrap();
    let data = entity_data(Path::new("merged/map.ymap"), &query, "merged", None, entities.clone());
    assert_eq!(
      data.entities.iter().map(|entity| entity.guid.value).collect::<Vec<_>>(),
      [0, 1, 2, 3]
    );
    let exact = EntityQuery::Position {
      position: [3.5, 4.2, 0.0],
      radius: 0.0,
    };
    assert!(exact.contains(&entities[0]));
    assert!(!exact.contains(&entities[1]));
    for query in [
      EntityQuery::Position {
        position: [f64::NAN, 0.0, 0.0],
        radius: 1.0,
      },
      EntityQuery::Position {
        position: [f64::MAX, 0.0, 0.0],
        radius: 1.0,
      },
      EntityQuery::Position {
        position: [0.0; 3],
        radius: -1.0,
      },
      EntityQuery::Position {
        position: [0.0; 3],
        radius: f64::INFINITY,
      },
    ] {
      assert!(query.validate().is_err());
    }
  }

  #[test]
  fn filename_globs_match_prefixed_ymaps_and_preserve_exact_filters() {
    let filter = filename_filter(Some("*cs4_10_strm_0.ymap")).unwrap();
    for name in [
      "hei_cs4_10_strm_0.ymap",
      "lr_cs4_10_strm_0.ymap",
      "cs4_10_strm_0.ymap",
      "LR_CS4_10_STRM_0.YMAP",
    ] {
      assert!(matches(&filter, name), "{name}");
    }
    assert!(!matches(&filter, "lr_cs4_10_strm_1.ymap"));
    assert!(!matches(&filter, "lr_cs4_10_strm_0.ybn"));
    let exact = filename_filter(Some("LR_CS4_10_STRM_0.YMAP")).unwrap();
    assert!(matches(&exact, "lr_cs4_10_strm_0.ymap"));
    assert!(!matches(&exact, "hei_cs4_10_strm_0.ymap"));
    assert!(matches(
      &filename_filter(Some("[hl]?_cs4_10_strm_0.ymap")).unwrap(),
      "lr_cs4_10_strm_0.ymap"
    ));
    assert!(filename_filter(Some("[invalid")).is_err());
    assert!(filename_filter(Some("../*.ymap")).is_err());
    assert!(filename_filter(Some("folder\\*.ymap")).is_err());
    assert!(matches(&filename_filter(None).unwrap(), "any.ymap"));
  }

  #[test]
  fn entity_guid_xml_keeps_unsigned_guids_duplicates_and_stage_context() {
    let xml: crate::core::format::ymap::xml::XmlYmap = quick_xml::de::from_str(include_str!(
      concat!(env!("CARGO_MANIFEST_DIR"), "/docs/sample/parent_refs/vanilla_parent.ymap.xml")
    ))
    .unwrap();
    let model: crate::core::format::ymap::model::Ymap = xml.into();
    let mut entity = model.entity_map.values().next().unwrap().clone();
    entity.guid = 2443198849;
    let data = entity_data(
      Path::new("merged/ymap/lr_cs4_10_strm_0.ymap"),
      &EntityQuery::Guid(entity.guid),
      "merged",
      None,
      vec![entity.clone(), entity],
    );
    assert!(data.found);
    assert_eq!(data.entities.len(), 2);
    let result = search_result(
      &EntityQuery::Guid(2443198849),
      vec![FileResult {
        name: "lr_cs4_10_strm_0.ymap".into(),
        data: vec![data],
      }],
    );
    let xml = quick_xml::se::to_string(&result).unwrap();
    assert!(xml.contains("stage=\"merged\""));
    assert_eq!(xml.matches("<guid value=\"2443198849\"").count(), 2);
    assert_eq!(xml.matches("<Item type=").count(), 2);
  }

  #[test]
  #[ignore = "requires local vanilla YMAP schemas"]
  fn find_searches_native_merged_and_recorded_before_entities_without_writes() {
    use crate::core::format::gamefile::meta_resource::MetaSchemaCatalog;
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
    let mut paths =
      fs::read_dir(base).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    paths.sort();
    let bytes = fs::read(
      paths
        .iter()
        .find(|path| {
          ymap_to_model_with_entities(&fs::read(path).unwrap(), &HashMap::new())
            .is_ok_and(|(_, entities)| !entities.is_empty())
        })
        .unwrap(),
    )
    .unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&meta);
    let (model, mut entities) = ymap_to_model_with_entities(&bytes, &catalog.hash_names).unwrap();
    entities[0].guid = 2443198849;
    let root = std::env::temp_dir().join(format!("find_entity_guid_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let merged = root.join("merged");
    fs::create_dir_all(merged.join("ymap")).unwrap();
    let vanilla = root.join("vanilla.ymap");
    let source = root.join("source.ymap");
    let merged_path = merged.join("ymap/lr_cs4_10_strm_0.ymap");
    let before =
      crate::core::format::ymap::binary::write_ymap(&model, &entities, &catalog).unwrap();
    fs::write(&vanilla, &before).unwrap();
    fs::write(&source, &before).unwrap();
    entities[0].position.x += 1.0;
    fs::write(
      &merged_path,
      crate::core::format::ymap::binary::write_ymap(&model, &entities, &catalog).unwrap(),
    )
    .unwrap();
    let input = |path: &Path, resource: Option<&str>| {
      serde_json::json!({
        "path": path, "resource": resource, "original_path": "resources/stream/lr_cs4_10_strm_0.ymap",
        "fingerprint": {"sha256": format!("{:x}", Sha256::digest(&before)), "size": before.len()},
      })
    };
    let record = serde_json::json!({"format_version": 1, "files": {
      "lr_cs4_10_strm_0.ymap": {"vanilla": input(&vanilla, None), "merge_sources": [input(&source, Some("resource"))]},
    }});
    crate::core::vanilla::write_json(&merged.join("merge_cache_info.json"), &record).unwrap();
    let query = FindEntityGuid {
      guid: 2443198849,
      merged_dir: merged.clone(),
      filter: Some("LR_CS4_10_STRM_0.YMAP".into()),
      diff_all: false,
    };
    let modified = fs::metadata(&merged_path).unwrap().modified().unwrap();
    let xml = query.run().unwrap();
    assert!(xml.contains("stage=\"merged\""));
    assert!(!xml.contains("stage=\"source\""));
    let position = &entities[0].position;
    let nearby = FindEntity {
      query: EntityQuery::Position {
        position: [position.x as f64, position.y as f64, position.z as f64],
        radius: 1.0,
      },
      merged_dir: merged.clone(),
      filter: query.filter.clone(),
      diff_all: true,
    };
    let nearby_xml = nearby.run().unwrap();
    assert!(nearby_xml.starts_with("<EntityPositionSearch"));
    assert!(nearby_xml.contains("position=\""));
    assert!(nearby_xml.contains("round=\"1"));
    assert!(!nearby_xml.contains(" guid=\""));
    assert!(nearby_xml.contains("stage=\"vanilla\""));
    assert!(nearby_xml.contains("stage=\"source\""));
    assert_eq!(nearby_xml.matches("<guid value=\"2443198849\"").count(), 3);
    let mut all = query.clone();
    all.diff_all = true;
    let xml = all.run().unwrap();
    assert!(xml.contains("stage=\"vanilla\""));
    assert!(xml.contains("stage=\"source\""));
    assert!(xml.contains("resource=\"resource\""));
    assert_eq!(xml.matches("<guid value=\"2443198849\"").count(), 3);
    assert_eq!(fs::metadata(&merged_path).unwrap().modified().unwrap(), modified);
    let mut expanded = record.clone();
    for name in ["hei_cs4_10_strm_0.ymap", "cs4_10_strm_0.ymap"] {
      fs::write(merged.join("ymap").join(name), fs::read(&merged_path).unwrap()).unwrap();
      expanded["files"]
        .as_object_mut()
        .unwrap()
        .insert(name.into(), record["files"]["lr_cs4_10_strm_0.ymap"].clone());
    }
    crate::core::vanilla::write_json(&merged.join("merge_cache_info.json"), &expanded).unwrap();
    let mut wildcard = query.clone();
    wildcard.filter = Some("*cs4_10_strm_0.ymap".into());
    assert_eq!(wildcard.run().unwrap().matches("<File ").count(), 3);
    wildcard.diff_all = true;
    let xml = wildcard.run().unwrap();
    assert_eq!(xml.matches("<File ").count(), 3);
    assert_eq!(xml.matches("stage=\"vanilla\"").count(), 3);
    assert_eq!(xml.matches("stage=\"source\"").count(), 3);
    let mut missing = query.clone();
    missing.guid = u32::MAX;
    assert!(!missing.run().unwrap().contains("<File"));
    missing.filter = Some("missing.ymap".into());
    assert!(!missing.run().unwrap().contains("<File"));
    missing.filter = Some("../outside.ymap".into());
    assert!(missing.run().is_err());
    fs::remove_file(&merged_path).unwrap();
    let xml = all.run().unwrap();
    assert!(xml.contains("stage=\"merged\" found=\"false\""));
    assert_eq!(xml.matches("<guid value=\"2443198849\"").count(), 2);
    fs::write(&source, b"changed source").unwrap();
    assert!(all.run().unwrap_err().to_string().contains("Pre-merge input changed"));
    fs::remove_dir_all(root).unwrap();
  }
}
