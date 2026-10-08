use super::{FindEntity, Input, Result, read_input, search_inputs};
use crate::core::format::ybn::{
  model::{Bound, Geometry, Material, Polygon},
  read_ybn,
};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename = "YbnPositionSearch")]
struct SearchResult {
  #[serde(rename = "@position")]
  position: String,
  #[serde(rename = "@radius")]
  radius: f64,
  #[serde(rename = "@type", skip_serializing_if = "Option::is_none")]
  kind: Option<String>,
  #[serde(rename = "File")]
  files: Vec<FileResult>,
}

#[derive(Serialize)]
struct FileResult {
  #[serde(rename = "@name")]
  name: String,
  #[serde(rename = "Data")]
  data: Vec<CollisionData>,
}

#[derive(Serialize)]
struct CollisionData {
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
  items: Vec<Collision>,
}

#[derive(Serialize)]
struct Collision {
  #[serde(rename = "@type")]
  kind: String,
  #[serde(rename = "@boundPath")]
  bound_path: String,
  #[serde(rename = "@owner")]
  owner: String,
  #[serde(rename = "@index", skip_serializing_if = "Option::is_none")]
  index: Option<usize>,
  #[serde(rename = "Position")]
  position: Point,
  #[serde(rename = "Vertex")]
  vertices: Vec<Point>,
  #[serde(rename = "Material", skip_serializing_if = "Option::is_none")]
  material: Option<Material>,
  #[serde(rename = "Radius", skip_serializing_if = "Option::is_none")]
  radius: Option<f32>,
}

#[derive(Clone, Copy, Serialize)]
struct Point {
  #[serde(rename = "@x")]
  x: f64,
  #[serde(rename = "@y")]
  y: f64,
  #[serde(rename = "@z")]
  z: f64,
}

impl From<[f64; 3]> for Point {
  fn from(position: [f64; 3]) -> Self {
    Self {
      x: position[0],
      y: position[1],
      z: position[2],
    }
  }
}

pub(super) fn run(
  search: &FindEntity,
  position: [f64; 3],
  radius: f64,
  kind: Option<&str>,
) -> Result<String> {
  let (paths, cache, names) = search_inputs(search, "ybn")?;
  let mut files = Vec::new();
  for name in names {
    let mut data = vec![read_collisions(
      paths.get(&name).map(|path| path.as_path()),
      position,
      radius,
      kind,
      "merged",
      None,
    )?];
    if let Some(cache) = &cache {
      let record = cache
        .files
        .get(&name)
        .ok_or_else(|| format!("No pre-merge provenance recorded for {name}"))?;
      if let Some(vanilla) = &record.vanilla {
        data.push(read_collisions(
          Some(&vanilla.path),
          position,
          radius,
          kind,
          "vanilla",
          Some(vanilla),
        )?);
      }
      for source in &record.merge_sources {
        data.push(read_collisions(
          Some(&source.path),
          position,
          radius,
          kind,
          "source",
          Some(source),
        )?);
      }
    }
    if data.iter().any(|stage| stage.found) {
      files.push(FileResult {
        name,
        data,
      });
    }
  }
  let result = SearchResult {
    position: format!("{},{},{}", position[0], position[1], position[2]),
    radius,
    kind: kind.map(str::to_string),
    files,
  };
  let mut xml = String::new();
  let mut serializer = quick_xml::se::Serializer::new(&mut xml);
  serializer.indent(' ', 2);
  result.serialize(serializer)?;
  Ok(xml)
}

fn read_collisions(
  path: Option<&Path>,
  position: [f64; 3],
  radius: f64,
  kind: Option<&str>,
  stage: &'static str,
  input: Option<&Input>,
) -> Result<CollisionData> {
  let mut items = Vec::new();
  if let Some(path) = path {
    let root = read_ybn(&read_input(path, stage, input)?)?;
    collect(&root, &mut Vec::new(), &mut Vec::new(), root.transform, &mut items)?;
    let center = position.map(|value| value as f32 as f64);
    items.retain(|item| {
      kind.is_none_or(|kind| item.kind == kind)
        && (item.position.x - center[0])
          .hypot(item.position.y - center[1])
          .hypot(item.position.z - center[2])
          <= radius
    });
  }
  Ok(CollisionData {
    stage,
    path: path.map(|path| path.to_string_lossy().replace('\\', "/")),
    resource: input.and_then(|input| input.resource.clone()),
    original_path: input
      .and_then(|input| input.original_path.as_ref())
      .map(|path| path.to_string_lossy().replace('\\', "/")),
    found: !items.is_empty(),
    items,
  })
}

/// Polygon centers are means of reference vertices, matching the native Box center.
/// Ancestor transforms are applied inside-out; GeometryCenter is Bounds-local.
fn collect(
  bound: &Bound,
  path: &mut Vec<usize>,
  transforms: &mut Vec<[f32; 16]>,
  transform: Option<[f32; 16]>,
  items: &mut Vec<Collision>,
) -> Result<()> {
  if let Some(transform) = transform {
    transforms.push(transform);
  }
  if let Some(geometry) = &bound.geometry {
    for (index, polygon) in geometry.polygons.iter().enumerate() {
      let (kind, indices, radius) = match polygon {
        Polygon::Box {
          vertices,
          ..
        } => ("box", vertices.to_vec(), None),
        Polygon::Triangle(triangle) => ("triangle", triangle.vertices.to_vec(), None),
        Polygon::Sphere {
          vertex,
          radius,
          ..
        } => ("sphere", vec![*vertex], Some(*radius)),
        Polygon::Capsule {
          vertex1,
          vertex2,
          radius,
          ..
        } => ("capsule", vec![*vertex1, *vertex2], Some(*radius)),
        Polygon::Cylinder {
          vertex1,
          vertex2,
          radius,
          ..
        } => ("cylinder", vec![*vertex1, *vertex2], Some(*radius)),
        Polygon::Unsupported {
          ..
        } => continue,
      };
      let vertices = indices
        .into_iter()
        .map(|index| vertex(geometry, index, transforms))
        .collect::<Result<Vec<_>>>()?;
      let center = std::array::from_fn(|axis| {
        vertices.iter().map(|vertex| vertex[axis]).sum::<f64>() / vertices.len() as f64
      });
      items.push(Collision {
        kind: kind.into(),
        bound_path: path_text(path),
        owner: bound.kind.clone(),
        index: Some(index),
        position: center.into(),
        vertices: vertices.into_iter().map(Point::from).collect(),
        material: geometry.materials.get(polygon.material() as usize).copied(),
        radius,
      });
    }
  } else if matches!(bound.kind.as_str(), "Box" | "Sphere" | "Capsule" | "Cylinder") {
    let offset = if bound.kind == "Box" { 64 } else { 80 };
    let center = [
      common_float(bound, offset)?,
      common_float(bound, offset + 4)?,
      common_float(bound, offset + 8)?,
    ];
    items.push(Collision {
      kind: bound.kind.to_ascii_lowercase(),
      bound_path: path_text(path),
      owner: bound.kind.clone(),
      index: None,
      position: world(center.map(f64::from), transforms)?.into(),
      vertices: Vec::new(),
      material: None,
      radius: if bound.kind == "Box" { None } else { Some(common_float(bound, 20)?) },
    });
  }
  for (index, child) in bound.children.iter().enumerate() {
    path.push(index);
    collect(
      child,
      path,
      transforms,
      child.transform.or_else(|| bound.transforms.get(index).copied()),
      items,
    )?;
    path.pop();
  }
  if transform.is_some() {
    transforms.pop();
  }
  Ok(())
}

fn path_text(path: &[usize]) -> String {
  if path.is_empty() {
    "root".into()
  } else {
    path.iter().map(usize::to_string).collect::<Vec<_>>().join("/")
  }
}

fn common_float(
  bound: &Bound,
  offset: usize,
) -> Result<f32> {
  let bytes = bound.common.get(offset..offset + 4).ok_or("Truncated YBN primitive metadata")?;
  Ok(f32::from_le_bytes(bytes.try_into()?))
}

fn vertex(
  geometry: &Geometry,
  index: u16,
  transforms: &[[f32; 16]],
) -> Result<[f64; 3]> {
  let point =
    geometry.vertices.get(index as usize).ok_or("YBN polygon vertex index out of range")?;
  world(
    std::array::from_fn(|axis| f64::from(point[axis]) + f64::from(geometry.center[axis])),
    transforms,
  )
}

fn world(
  mut point: [f64; 3],
  transforms: &[[f32; 16]],
) -> Result<[f64; 3]> {
  for matrix in transforms.iter().rev() {
    point = std::array::from_fn(|axis| {
      point[0] * f64::from(matrix[axis])
        + point[1] * f64::from(matrix[4 + axis])
        + point[2] * f64::from(matrix[8 + axis])
        + f64::from(matrix[12 + axis])
    });
  }
  if point.iter().any(|value| !value.is_finite()) {
    return Err("Non-finite YBN collision position".into());
  }
  Ok(point)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    find::EntityQuery,
    format::ybn::{write_ybn, xml::read_xml},
  };
  use sha2::{Digest, Sha256};
  use std::fs;

  fn fixture() -> Bound {
    let mut root = read_xml(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
    )))
    .unwrap();
    let geometry = root.children[0].geometry.as_mut().unwrap();
    geometry.vertices =
      vec![[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [-1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [0.0; 3]];
    geometry.polygons = vec![Polygon::Box {
      material: 0,
      vertices: [0, 1, 2, 3],
    }];
    root
  }

  fn translation(position: [f32; 3]) -> [f32; 16] {
    [
      1.0,
      0.0,
      0.0,
      0.0,
      0.0,
      1.0,
      0.0,
      0.0,
      0.0,
      0.0,
      1.0,
      0.0,
      position[0],
      position[1],
      position[2],
      1.0,
    ]
  }

  #[test]
  fn ybn_positions_apply_geometry_center_and_nested_parent_transforms() {
    let mut root = fixture();
    let mut geometry = root.children.remove(0);
    geometry.geometry.as_mut().unwrap().center = [2.0, 0.0, 0.0];
    geometry.transform = None;
    let mut wrapper = root.clone();
    wrapper.children = vec![geometry];
    wrapper.transforms = vec![translation([0.0; 3])];
    wrapper.transform = Some(translation([1.0, 2.0, 3.0]));
    root.children = vec![wrapper];
    root.transform =
      Some([0.0, 2.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 10.0, 20.0, 30.0, 1.0]);
    let mut items = Vec::new();
    collect(&root, &mut Vec::new(), &mut Vec::new(), root.transform, &mut items).unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!([item.position.x, item.position.y, item.position.z], [4.0, 26.0, 42.0]);
    assert_eq!(item.bound_path, "0/0");
    assert_eq!(item.vertices.len(), 4);
    assert_eq!(item.index, Some(0));
    root.children[0].children[0].geometry.as_mut().unwrap().polygons = vec![Polygon::Box {
      material: 0,
      vertices: [0, 1, 2, 99],
    }];
    assert!(
      collect(&root, &mut Vec::new(), &mut Vec::new(), root.transform, &mut Vec::new()).is_err()
    );
    assert!(world([0.0; 3], &[translation([f32::NAN, 0.0, 0.0])]).is_err());
  }

  #[test]
  fn ybn_search_preserves_duplicates_types_and_inclusive_3d_boundary() {
    let directory = std::env::temp_dir().join(format!("find_ybn_boundary_{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let mut root = fixture();
    let geometry = root.children[0].geometry.as_mut().unwrap();
    geometry.polygons.extend([
      geometry.polygons[0],
      Polygon::Sphere {
        material: 0,
        vertex: 4,
        radius: 2.0,
      },
      Polygon::Capsule {
        material: 0,
        vertex1: 0,
        vertex2: 3,
        radius: 0.5,
      },
      Polygon::Cylinder {
        material: 0,
        vertex1: 1,
        vertex2: 2,
        radius: 0.5,
      },
      Polygon::Triangle(crate::core::format::ybn::model::Triangle {
        vertices: [0, 1, 2],
        ..Default::default()
      }),
    ]);
    let original = root.children[0].clone();
    for position in
      [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [1.0, 1.0, 0.0], [0.0, 0.0, 1.01]]
    {
      let mut child = original.clone();
      child.transform = Some(translation(position));
      root.transforms.push(child.transform.unwrap());
      root.flags.push([0; 2]);
      root.children.push(child);
    }
    let path = directory.join("collision.ybn");
    fs::write(&path, write_ybn(&root).unwrap()).unwrap();
    let data = read_collisions(Some(&path), [0.0; 3], 1.0, Some("box"), "merged", None).unwrap();
    assert_eq!(data.items.len(), 8);
    assert!(data.items.iter().all(|item| item.kind == "box"));
    assert_eq!(
      read_collisions(Some(&path), [0.0; 3], 0.0, Some("box"), "merged", None).unwrap().items.len(),
      2
    );
    for kind in ["triangle", "sphere", "capsule", "cylinder"] {
      assert!(
        read_collisions(Some(&path), [0.0; 3], 1.0, Some(kind), "merged", None).unwrap().found,
        "{kind}"
      );
    }
    assert!(
      read_collisions(Some(&path), [0.0; 3], 1.0, None, "merged", None).unwrap().items.len()
        > data.items.len()
    );
    let mut primitive = original;
    primitive.kind = "Box".into();
    primitive.geometry = None;
    for axis in 0..3 {
      primitive.common[64 + axis * 4..68 + axis * 4].copy_from_slice(&(axis as f32).to_le_bytes());
    }
    let mut items = Vec::new();
    collect(&primitive, &mut Vec::new(), &mut Vec::new(), Some(translation([10.0; 3])), &mut items)
      .unwrap();
    assert_eq!([items[0].position.x, items[0].position.y, items[0].position.z], [10.0, 11.0, 12.0]);
    assert!(items[0].index.is_none());
    fs::remove_dir_all(directory).unwrap();
  }

  #[test]
  fn ybn_find_reads_before_inputs_globs_missing_output_and_rejects_changed_inputs() {
    let directory =
      std::env::temp_dir().join(format!("find_ybn_provenance_{}", std::process::id()));
    let merged = directory.join("merged");
    fs::create_dir_all(merged.join("ybn")).unwrap();
    let bytes = write_ybn(&fixture()).unwrap();
    let vanilla = directory.join("vanilla.ybn");
    let source = directory.join("source.ybn");
    let missing = directory.join("missing.ybn");
    fs::write(&vanilla, &bytes).unwrap();
    fs::write(&source, &bytes).unwrap();
    let mut absent = fixture();
    absent.children[0].transform = Some(translation([10.0, 0.0, 0.0]));
    absent.transforms[0] = absent.children[0].transform.unwrap();
    fs::write(&missing, write_ybn(&absent).unwrap()).unwrap();
    let output = merged.join("ybn/collision.ybn");
    fs::write(&output, &bytes).unwrap();
    fs::write(merged.join("ybn/hei_collision.ybn"), &bytes).unwrap();
    let input = |path: &Path, resource: Option<&str>| {
      let bytes = fs::read(path).unwrap();
      serde_json::json!({"path": path, "resource": resource, "original_path": "resource/stream/collision.ybn", "fingerprint": {"size": bytes.len(), "sha256": format!("{:x}", Sha256::digest(bytes))}})
    };
    let record = serde_json::json!({"vanilla": input(&vanilla, None), "merge_sources": [input(&source, Some("retains")), input(&missing, Some("removes"))]});
    let cache = serde_json::json!({"format_version": 1, "files": {"collision.ybn": record.clone(), "hei_collision.ybn": record}});
    crate::core::vanilla::write_json(&merged.join("merge_cache_info.json"), &cache).unwrap();
    let mut query = FindEntity {
      query: EntityQuery::YbnPosition {
        position: [0.0; 3],
        radius: 1.0,
        kind: Some("box".into()),
      },
      merged_dir: merged.clone(),
      filter: Some("COLLISION.YBN".into()),
      diff_all: true,
    };
    let modified = fs::metadata(&output).unwrap().modified().unwrap();
    let xml = query.run().unwrap();
    assert!(xml.starts_with("<YbnPositionSearch position=\"0,0,0\" radius=\"1\" type=\"box\""));
    assert_eq!(xml.matches("<Item ").count(), 3);
    assert!(xml.contains("boundPath=\"0\""));
    assert!(xml.contains("resource=\"removes\"") && xml.contains("found=\"false\""));
    assert!(xml.contains("<Vertex ") && xml.contains("<Material>"));
    assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), modified);
    query.filter = Some("*collision.ybn".into());
    assert_eq!(query.run().unwrap().matches("<File ").count(), 2);
    query.diff_all = false;
    assert!(!query.run().unwrap().contains("stage=\"source\""));
    query.diff_all = true;
    query.filter = Some("collision.ybn".into());
    fs::remove_file(&output).unwrap();
    let xml = query.run().unwrap();
    assert!(xml.contains("stage=\"merged\" found=\"false\""));
    assert_eq!(xml.matches("<Item ").count(), 2);
    query.filter = Some("../*.ybn".into());
    assert!(query.run().is_err());
    query.filter = Some("collision.ybn".into());
    fs::write(source, b"changed source").unwrap();
    assert!(query.run().unwrap_err().to_string().contains("Pre-merge input changed"));
    query.query = EntityQuery::YbnPosition {
      position: [0.0; 3],
      radius: -1.0,
      kind: None,
    };
    assert!(query.run().is_err());
    query.query = EntityQuery::YbnPosition {
      position: [0.0; 3],
      radius: 1.0,
      kind: Some("unknown".into()),
    };
    assert!(query.run().is_err());
    fs::remove_dir_all(directory).unwrap();
  }
}
