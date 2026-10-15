use crate::core::format::gamefile::meta::{
  MetaSchemaCatalog, MetaStructureEntry, MetaStructureInfo, jenk_hash,
};
use crate::core::format::ymap::{
  model::{Ymap, YmapDistantLodLight, YmapLodLight},
  xml::XmlYmap,
};
use crate::core::lod_light::{io::Fingerprint, snapshot::LoadedMap};
use std::collections::BTreeMap;

pub(super) fn fixture() -> (BTreeMap<String, LoadedMap>, BTreeMap<String, Fingerprint>) {
  let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
  )))
  .unwrap();
  let mut parent: Ymap = xml.into();
  parent.name = "parent".into();
  parent.parent = String::new();
  parent.distant_lod_lights.items = (0..3)
    .map(|index| YmapDistantLodLight {
      position: format!("{index} 0 0"),
      rgbi: "4294967295".into(),
    })
    .collect();
  let mut child = parent.clone();
  child.name = "child".into();
  child.parent = "parent".into();
  child.distant_lod_lights.items.clear();
  child.lod_lights = (0..3)
    .map(|_| YmapLodLight {
      direction: crate::common::position::Position {
        x: 0.0,
        y: 0.0,
        z: -1.0,
      },
      falloff: 10.0,
      time_and_state_flags: ((2u32 << 26) | (1 << 24)).to_string(),
      hash: "42".into(),
      cone_outer_angle_or_cap_ext: 100,
      ..Default::default()
    })
    .collect();
  let maps: BTreeMap<String, LoadedMap> = BTreeMap::from([
    (
      "parent.ymap".into(),
      LoadedMap {
        model: parent,
        entities: Vec::new(),
      },
    ),
    (
      "child.ymap".into(),
      LoadedMap {
        model: child,
        entities: Vec::new(),
      },
    ),
  ]);
  let fingerprints = maps
    .keys()
    .map(|path| {
      (
        path.clone(),
        Fingerprint {
          sha256: "fixture".into(),
          size: 1,
          modified_seconds: 0,
          modified_nanos: 0,
        },
      )
    })
    .collect();
  (maps, fingerprints)
}

pub(super) fn native_catalog() -> MetaSchemaCatalog {
  let entry = |name: &str, offset, kind, key| MetaStructureEntry {
    name_hash: if name == "ARRAYINFO" { 0x100 } else { jenk_hash(name) },
    data_offset: offset,
    data_type: kind,
    reference_key: key,
    reference_type_index: 0,
    unknown: 0,
  };
  let mut catalog = MetaSchemaCatalog::default();
  let mut add = |name: &str, size, entries| {
    let hash = jenk_hash(name);
    catalog.structures.insert(
      hash,
      MetaStructureInfo {
        name_hash: hash,
        structure_size: size,
        structure_key: 0,
        unknown_8: 0,
        unknown_12: 0,
        unknown_28: 0,
        entries,
      },
    );
  };
  add("FloatXYZ", 12, vec![entry("x", 0, 0x21, 0), entry("y", 4, 0x21, 0), entry("z", 8, 0x21, 0)]);
  let mut lod = Vec::new();
  for (index, (field, kind, key)) in [
    ("direction", 0x05, jenk_hash("FloatXYZ")),
    ("falloff", 0x21, 0),
    ("falloffExponent", 0x21, 0),
    ("timeAndStateFlags", 0x15, 0),
    ("hash", 0x15, 0),
    ("coneInnerAngle", 0x11, 0),
    ("coneOuterAngleOrCapExt", 0x11, 0),
    ("coronaIntensity", 0x11, 0),
  ]
  .into_iter()
  .enumerate()
  {
    lod.push(entry("ARRAYINFO", 0, kind, key));
    lod.push(entry(field, 8 + index as u32 * 16, 0x52, 0));
  }
  add("CLODLight", 136, lod);
  add(
    "CDistantLODLight",
    48,
    vec![
      entry("ARRAYINFO", 0, 0x05, jenk_hash("FloatXYZ")),
      entry("position", 8, 0x52, 0),
      entry("ARRAYINFO", 0, 0x15, 0),
      entry("RGBI", 24, 0x52, 0),
      entry("numStreetLights", 40, 0x13, 0),
      entry("category", 42, 0x13, 0),
    ],
  );
  add(
    "CBlockDesc",
    104,
    vec![
      entry("version", 0, 0x15, 0),
      entry("flags", 4, 0x15, 0),
      entry("name", 8, 0x40, 24),
      entry("exportedBy", 32, 0x40, 24),
      entry("owner", 56, 0x40, 24),
      entry("time", 80, 0x40, 24),
    ],
  );
  add(
    "CMapData",
    368,
    vec![
      entry("name", 0, 0x4a, 0),
      entry("parent", 4, 0x4a, 0),
      entry("flags", 8, 0x15, 0),
      entry("contentFlags", 12, 0x15, 0),
      entry("streamingExtentsMin", 16, 0x33, 0),
      entry("streamingExtentsMax", 32, 0x33, 0),
      entry("entitiesExtentsMin", 48, 0x33, 0),
      entry("entitiesExtentsMax", 64, 0x33, 0),
      entry("LODLightsSOA", 80, 0x05, jenk_hash("CLODLight")),
      entry("DistantLODLightsSOA", 216, 0x05, jenk_hash("CDistantLODLight")),
      entry("block", 264, 0x05, jenk_hash("CBlockDesc")),
    ],
  );
  catalog
}

pub(super) fn native_resources(
  root: &std::path::Path,
  resources: &[&str],
) -> std::path::PathBuf {
  use std::fs;
  let source = root.join("source");
  let (maps, _) = fixture();
  let catalog = native_catalog();
  for resource in resources {
    let directory = source.join(resource);
    fs::create_dir_all(directory.join("stream/nested")).unwrap();
    fs::write(directory.join("fxmanifest.lua"), []).unwrap();
    for (path, map) in &maps {
      let mut model = map.model.clone();
      model.entities.clear();
      model.block.name.clear();
      model.block.exported_by.clear();
      model.block.owner.clear();
      model.block.time.clear();
      fs::write(
        directory.join("stream/nested").join(path),
        crate::core::format::ymap::binary::write_ymap(&model, &[], &catalog).unwrap(),
      )
      .unwrap();
    }
  }
  source
}
