//! The web sample's collision files, written as YBN XML and converted as `--from-xml` does.

/// A polygon of a [`Geometry`], its vertices indexing [`Geometry::vertices`].
pub(super) enum Polygon {
  Triangle([usize; 3]),
  Box([usize; 4]),
  Sphere(usize, f32),
}

/// A child bound with its own vertices and polygons.
pub(super) struct Geometry {
  pub(super) vertices: Vec<[f32; 3]>,
  pub(super) polygons: Vec<Polygon>,
}

/// A child of the composite bound.
pub(super) enum Child {
  Geometry(Geometry),
  /// An axis-aligned box from the first corner to the second.
  Box([f32; 3], [f32; 3]),
}

impl Geometry {
  /// A flat quad from `min` to `max` at height `z`, as two triangles.
  pub(super) fn ground(
    min: [f32; 2],
    max: [f32; 2],
    z: f32,
  ) -> Self {
    Self {
      vertices: vec![
        [min[0], min[1], z],
        [max[0], min[1], z],
        [max[0], max[1], z],
        [min[0], max[1], z],
      ],
      polygons: vec![Polygon::Triangle([0, 1, 2]), Polygon::Triangle([0, 2, 3])],
    }
  }

  /// Adds a box polygon spanning `min` to `max` by its four diagonal corners.
  pub(super) fn add_box(
    &mut self,
    min: [f32; 3],
    max: [f32; 3],
  ) {
    let first = self.vertices.len();
    self.vertices.extend([
      [min[0], min[1], min[2]],
      [max[0], max[1], min[2]],
      [min[0], max[1], max[2]],
      [max[0], min[1], max[2]],
    ]);
    self.polygons.push(Polygon::Box([first, first + 1, first + 2, first + 3]));
  }

  pub(super) fn add_sphere(
    &mut self,
    centre: [f32; 3],
    radius: f32,
  ) {
    self.vertices.push(centre);
    self.polygons.push(Polygon::Sphere(self.vertices.len() - 1, radius));
  }

  fn bounds(&self) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for (index, vertex) in self.vertices.iter().enumerate() {
      let radius = self
        .polygons
        .iter()
        .find_map(|polygon| match polygon {
          Polygon::Sphere(centre, radius) if *centre == index => Some(*radius),
          _ => None,
        })
        .unwrap_or(0.0);
      for axis in 0..3 {
        min[axis] = min[axis].min(vertex[axis] - radius);
        max[axis] = max[axis].max(vertex[axis] + radius);
      }
    }
    (min, max)
  }
}

fn vector(
  name: &str,
  value: [f32; 3],
) -> String {
  format!("<{name} x=\"{}\" y=\"{}\" z=\"{}\" />", value[0], value[1], value[2])
}

/// The fields every bound has, for the box from `min` to `max`.
fn common(
  min: [f32; 3],
  max: [f32; 3],
) -> String {
  let centre = std::array::from_fn::<f32, 3, _>(|axis| (min[axis] + max[axis]) / 2.0);
  let radius = (0..3).map(|axis| ((max[axis] - min[axis]) / 2.0).powi(2)).sum::<f32>().sqrt();
  [
    vector("BoxMin", min),
    vector("BoxMax", max),
    vector("BoxCenter", centre),
    vector("SphereCenter", centre),
    format!("<SphereRadius value=\"{radius}\" />"),
    "<Margin value=\"0.04\" />".into(),
    "<Volume value=\"1\" />".into(),
    "<Inertia x=\"1\" y=\"1\" z=\"1\" />".into(),
    "<MaterialIndex value=\"0\" />".into(),
    "<MaterialColourIndex value=\"0\" />".into(),
    "<ProceduralID value=\"0\" />".into(),
    "<RoomID value=\"0\" />".into(),
    "<PedDensity value=\"0\" />".into(),
    "<UnkFlags value=\"0\" />".into(),
    "<PolyFlags value=\"0\" />".into(),
    "<UnkType value=\"1\" />".into(),
  ]
  .join("\n")
}

const CHILD: &str = "<CompositeTransform>1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1</CompositeTransform>\n\
  <CompositeFlags1>NONE</CompositeFlags1>\n<CompositeFlags2>NONE</CompositeFlags2>";

const MATERIAL: &str = "<Materials><Item><Type value=\"1\" /><ProceduralID value=\"0\" />\
  <RoomID value=\"0\" /><PedDensity value=\"0\" /><Flags>NONE</Flags>\
  <MaterialColourIndex value=\"0\" /><Unk value=\"0\" /></Item></Materials>";

impl Child {
  fn bounds(&self) -> ([f32; 3], [f32; 3]) {
    match self {
      Self::Geometry(geometry) => geometry.bounds(),
      Self::Box(min, max) => (*min, *max),
    }
  }

  fn xml(&self) -> String {
    let (min, max) = self.bounds();
    match self {
      Self::Box(..) => format!("<Item type=\"Box\">\n{}\n{CHILD}\n</Item>", common(min, max)),
      Self::Geometry(geometry) => {
        let centre = std::array::from_fn::<f32, 3, _>(|axis| (min[axis] + max[axis]) / 2.0);
        let vertices = geometry
          .vertices
          .iter()
          .map(|vertex| format!("{}, {}, {}", vertex[0], vertex[1], vertex[2]))
          .collect::<Vec<_>>()
          .join("\n");
        let polygons = geometry
          .polygons
          .iter()
          .map(|polygon| match polygon {
            Polygon::Triangle([a, b, c]) => {
              format!(
                "<Triangle m=\"0\" v1=\"{a}\" v2=\"{b}\" v3=\"{c}\" f1=\"0\" f2=\"0\" f3=\"0\" />"
              )
            }
            Polygon::Box([a, b, c, d]) => {
              format!("<Box m=\"0\" v1=\"{a}\" v2=\"{b}\" v3=\"{c}\" v4=\"{d}\" />")
            }
            Polygon::Sphere(vertex, radius) => {
              format!("<Sphere m=\"0\" v=\"{vertex}\" radius=\"{radius}\" />")
            }
          })
          .collect::<Vec<_>>()
          .join("\n");
        format!(
          "<Item type=\"GeometryBVH\">\n{}\n{CHILD}\n{}\n<UnkFloat1 value=\"0\" />\n\
           <UnkFloat2 value=\"0\" />\n{MATERIAL}\n<Vertices>{vertices}</Vertices>\n\
           <Polygons>\n{polygons}\n</Polygons>\n</Item>",
          common(min, max),
          vector("GeometryCenter", centre),
        )
      }
    }
  }
}

/// The YBN XML of a composite bound holding `children`.
pub(super) fn xml(children: &[Child]) -> String {
  let mut min = [f32::MAX; 3];
  let mut max = [f32::MIN; 3];
  for child in children {
    let (child_min, child_max) = child.bounds();
    for axis in 0..3 {
      min[axis] = min[axis].min(child_min[axis]);
      max[axis] = max[axis].max(child_max[axis]);
    }
  }
  format!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BoundsFile>\n<Bounds type=\"Composite\">\n{}\n\
     <Children>\n{}\n</Children>\n</Bounds>\n</BoundsFile>\n",
    common(min, max),
    children.iter().map(Child::xml).collect::<Vec<_>>().join("\n"),
  )
}
