//! YBN models as people and tools read them: the bound tree with named collision and material
//! flags, and geometries as vertex tables their polygons index.

use super::model::{Bound, Geometry, Material, Polygon};
use serde::{Deserialize, Serialize};

/// The bits of a composite child's flags, after CodeWalker's `EBoundCompositeFlags`.
const COMPOSITE_FLAGS: [&str; 32] = [
  "UNKNOWN",
  "MAP_WEAPON",
  "MAP_DYNAMIC",
  "MAP_ANIMAL",
  "MAP_COVER",
  "MAP_VEHICLE",
  "VEHICLE_NOT_BVH",
  "VEHICLE_BVH",
  "VEHICLE_BOX",
  "PED",
  "RAGDOLL",
  "ANIMAL",
  "ANIMAL_RAGDOLL",
  "OBJECT",
  "OBJECT_ENV_CLOTH",
  "PLANT",
  "PROJECTILE",
  "EXPLOSION",
  "PICKUP",
  "FOLIAGE",
  "FORKLIFT_FORKS",
  "TEST_WEAPON",
  "TEST_CAMERA",
  "TEST_AI",
  "TEST_SCRIPT",
  "TEST_VEHICLE_WHEEL",
  "GLASS",
  "MAP_RIVER",
  "SMOKE",
  "UNSMASHED",
  "MAP_STAIRS",
  "MAP_DEEP_SURFACE",
];

/// The bits of a material's flags, after CodeWalker's `EBoundMaterialFlags`.
const MATERIAL_FLAGS: [&str; 16] = [
  "STAIRS",
  "NOT_CLIMBABLE",
  "SEE_THROUGH",
  "SHOOT_THROUGH",
  "NOT_COVER",
  "WALKABLE_PATH",
  "NO_CAM_COLLISION",
  "SHOOT_THROUGH_FX",
  "NO_DECAL",
  "NO_NAVMESH",
  "NO_RAGDOLL",
  "VEHICLE_WHEEL",
  "NO_PTFX",
  "TOO_STEEP_FOR_PLAYER",
  "NO_NETWORK_SPAWN",
  "NO_CAM_COLLISION_ALLOW_CLIPPING",
];

/// A bit set with the names of its set bits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct NamedFlags {
  pub value: u32,
  /// The names of the bits set in `value`, lowest first.
  pub names: Vec<String>,
}

impl NamedFlags {
  fn of(
    value: u32,
    names: &[&str],
  ) -> Self {
    Self {
      value,
      names: names
        .iter()
        .enumerate()
        .filter(|(bit, _)| value & (1 << bit) != 0)
        .map(|(_, name)| (*name).to_string())
        .collect(),
    }
  }
}

/// A composite child's collision flags: what the child is, and what it collides with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct CollisionFlags {
  /// What the child's collision is.
  #[serde(rename = "type")]
  pub kind: NamedFlags,
  /// What the child's collision collides with.
  pub include: NamedFlags,
}

impl CollisionFlags {
  pub fn of(flags: [u32; 2]) -> Self {
    Self {
      kind: NamedFlags::of(flags[0], &COMPOSITE_FLAGS),
      include: NamedFlags::of(flags[1], &COMPOSITE_FLAGS),
    }
  }
}

/// A bound of a YBN.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnReadBound {
  /// Bounds type, such as `Composite` or `GeometryBVH`.
  pub kind: String,
  /// Corners of the bound's box, in its own coordinates.
  pub box_min: [f32; 3],
  pub box_max: [f32; 3],
  /// The children of a composite, in order, empty slots included.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub children: Vec<YbnReadChild>,
  /// The polygons of a geometry.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub geometry: Option<YbnReadGeometry>,
}

/// A slot of a composite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnReadChild {
  pub flags: CollisionFlags,
  /// Column-major transform from the child's coordinates to the composite's.
  pub transform: [f32; 16],
  /// The child, or `None` for an empty slot.
  pub bound: Option<YbnReadBound>,
}

/// The vertices, materials and polygons of a geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnReadGeometry {
  /// Vertex positions in the bound's coordinates, the geometry's centre added.
  pub vertices: Vec<[f32; 3]>,
  /// Colours parallel to `vertices`, when the geometry has them.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub vertex_colours: Vec<[u8; 4]>,
  pub materials: Vec<YbnReadMaterial>,
  pub polygons: Vec<YbnReadPolygon>,
}

/// A collision material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnReadMaterial {
  /// Surface type: the index of the game's `materials.dat` entry.
  pub kind: u8,
  /// The surface type's name in `materials.dat`, when the vanilla cache holds the names and the
  /// index is among them.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub surface: Option<String>,
  pub procedural_id: u8,
  pub room_id: u8,
  pub ped_density: u8,
  pub flags: NamedFlags,
  /// The material's colour, when the geometry has a colour table.
  pub colour: Option<[u8; 4]>,
  /// Bits of the material record without a known meaning.
  pub unknown: u16,
}

/// A polygon of a geometry: indices into its vertices and materials.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YbnReadPolygon {
  /// Corners in winding order; a corner flag is set where `vertex_flags` lists `true`.
  Triangle {
    vertices: [u16; 3],
    #[serde(default, skip_serializing_if = "none_set")]
    vertex_flags: [bool; 3],
    material: u8,
  },
  /// Four reference corners of a box.
  Box {
    vertices: [u16; 4],
    material: u8,
  },
  Sphere {
    vertex: u16,
    radius: f32,
    material: u8,
  },
  /// Axis endpoints.
  Capsule {
    vertices: [u16; 2],
    radius: f32,
    material: u8,
  },
  /// Axis endpoints.
  Cylinder {
    vertices: [u16; 2],
    radius: f32,
    material: u8,
  },
  /// A record of an unknown type, as its bytes.
  Unknown {
    raw: Vec<u8>,
  },
}

fn none_set(flags: &[bool; 3]) -> bool {
  !flags.iter().any(|flag| *flag)
}

/// `bound` as it is read, its materials' surface types named by `surfaces`, the names of
/// `materials.dat` by index.
pub fn read_model(
  bound: &Bound,
  surfaces: &[String],
) -> YbnReadBound {
  let corner = |offset: usize| -> [f32; 3] {
    std::array::from_fn(|axis| {
      let at = offset + axis * 4;
      bound
        .common
        .get(at..at + 4)
        .map_or(0.0, |bytes| f32::from_le_bytes(bytes.try_into().expect("four bytes")))
    })
  };
  let children = bound
    .children
    .iter()
    .enumerate()
    .map(|(index, child)| YbnReadChild {
      flags: CollisionFlags::of(bound.flags.get(index).copied().unwrap_or_default()),
      transform: bound.transforms.get(index).copied().unwrap_or(IDENTITY),
      bound: if child.kind == "None" { None } else { Some(read_model(child, surfaces)) },
    })
    .collect();
  YbnReadBound {
    kind: bound.kind.clone(),
    box_min: corner(48),
    box_max: corner(32),
    children,
    geometry: bound.geometry.as_ref().map(|geometry| read_geometry(geometry, surfaces)),
  }
}

const IDENTITY: [f32; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

fn read_geometry(
  geometry: &Geometry,
  surfaces: &[String],
) -> YbnReadGeometry {
  let material = |material: &Material| YbnReadMaterial {
    kind: material.kind,
    surface: surfaces.get(usize::from(material.kind)).cloned(),
    procedural_id: material.procedural_id,
    room_id: material.room_id,
    ped_density: material.ped_density,
    flags: NamedFlags::of(u32::from(material.flags), &MATERIAL_FLAGS),
    colour: geometry.material_colours.get(usize::from(material.colour_index)).copied(),
    unknown: material.unknown,
  };
  let polygon = |polygon: &Polygon| match *polygon {
    Polygon::Triangle(triangle) => YbnReadPolygon::Triangle {
      vertices: triangle.vertices,
      vertex_flags: triangle.vertex_flags,
      material: triangle.material,
    },
    Polygon::Box {
      material,
      vertices,
    } => YbnReadPolygon::Box {
      vertices,
      material,
    },
    Polygon::Sphere {
      material,
      vertex,
      radius,
    } => YbnReadPolygon::Sphere {
      vertex,
      radius,
      material,
    },
    Polygon::Capsule {
      material,
      vertex1,
      vertex2,
      radius,
    } => YbnReadPolygon::Capsule {
      vertices: [vertex1, vertex2],
      radius,
      material,
    },
    Polygon::Cylinder {
      material,
      vertex1,
      vertex2,
      radius,
    } => YbnReadPolygon::Cylinder {
      vertices: [vertex1, vertex2],
      radius,
      material,
    },
    Polygon::Unsupported {
      raw,
    } => YbnReadPolygon::Unknown {
      raw: raw.to_vec(),
    },
  };
  YbnReadGeometry {
    vertices: geometry
      .vertices
      .iter()
      .map(|vertex| std::array::from_fn(|axis| vertex[axis] + geometry.center[axis]))
      .collect(),
    vertex_colours: geometry.vertex_colours.clone(),
    materials: geometry.materials.iter().map(material).collect(),
    polygons: geometry.polygons.iter().map(polygon).collect(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn flags_name_their_set_bits_and_keep_the_value() {
    let flags = CollisionFlags::of([0x3e, 1 << 9 | 1 << 16]);
    assert_eq!(
      flags.kind.names,
      ["MAP_WEAPON", "MAP_DYNAMIC", "MAP_ANIMAL", "MAP_COVER", "MAP_VEHICLE"]
    );
    assert_eq!(flags.kind.value, 0x3e);
    assert_eq!(flags.include.names, ["PED", "PROJECTILE"]);
    let json = serde_json::to_value(&flags).unwrap();
    assert_eq!(json["type"]["value"], 0x3e);
    assert_eq!(serde_json::from_value::<CollisionFlags>(json).unwrap(), flags);
    let material = NamedFlags::of(1 | 1 << 15, &MATERIAL_FLAGS);
    assert_eq!(material.names, ["STAIRS", "NO_CAM_COLLISION_ALLOW_CLIPPING"]);
  }
}
