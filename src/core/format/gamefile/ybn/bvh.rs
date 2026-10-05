use std::io;

use super::*;

struct BvhItem {
  index: usize,
  minimum: [f32; 3],
  maximum: [f32; 3],
}

struct BvhBuildNode {
  items: Vec<BvhItem>,
  children: Vec<BvhBuildNode>,
  minimum: [f32; 3],
  maximum: [f32; 3],
  index: usize,
}

struct BvhNodeRecord {
  minimum: [i16; 3],
  maximum: [i16; 3],
  item_id: i16,
  item_count: i16,
}

struct BvhTreeRecord {
  minimum: [i16; 3],
  maximum: [i16; 3],
  first_node: i16,
  end_node: i16,
}

struct GeometryBvh {
  minimum: [f32; 3],
  maximum: [f32; 3],
  center: [f32; 3],
  quantum: [f32; 3],
  quantum_inverse: [f32; 3],
  nodes: Vec<BvhNodeRecord>,
  trees: Vec<BvhTreeRecord>,
}

/// Builds, orders, and serializes a geometry acceleration tree.
pub(super) fn append_geometry_bvh(
  data: &mut Vec<u8>,
  geometry: &mut Geometry,
  bound: &Bound,
) -> io::Result<u64> {
  let Some(bvh) = build_geometry_bvh(geometry, bound)? else {
    return Ok(0);
  };
  super::address(encode_geometry_bvh_data(data, &bvh)?)
}

/// Calculates an oriented geometry's conservative world-space AABB.
pub(super) fn geometry_polygon_bounds(
  geometry: &Geometry,
  bound: &Bound,
) -> io::Result<([f32; 3], [f32; 3])> {
  let mut minimum = [f32::INFINITY; 3];
  let mut maximum = [f32::NEG_INFINITY; 3];
  for polygon in &geometry.polygons {
    if matches!(polygon, Polygon::Unsupported { .. }) {
      continue;
    }
    let (polygon_minimum, polygon_maximum) = polygon_bounds(geometry, bound, polygon)?;
    for axis in 0..3 {
      minimum[axis] = minimum[axis].min(polygon_minimum[axis]);
      maximum[axis] = maximum[axis].max(polygon_maximum[axis]);
    }
  }
  if minimum.iter().any(|value| !value.is_finite())
    || maximum.iter().any(|value| !value.is_finite())
  {
    return Err(super::invalid("YBN geometry has no polygon bounds"));
  }
  Ok((minimum, maximum))
}

fn polygon_bounds(
  geometry: &Geometry,
  bound: &Bound,
  polygon: &Polygon,
) -> io::Result<([f32; 3], [f32; 3])> {
  let mut minimum = [f32::INFINITY; 3];
  let mut maximum = [f32::NEG_INFINITY; 3];
  let mut include_vertex = |index: usize, radius: f32| -> io::Result<()> {
    let vertex = geometry
      .vertices
      .get(index)
      .ok_or_else(|| super::invalid("YBN polygon vertex index is out of range"))?;
    let point = transform_point(
      [
        vertex[0] + geometry.center[0],
        vertex[1] + geometry.center[1],
        vertex[2] + geometry.center[2],
      ],
      bound.transform,
    );
    for axis in 0..3 {
      minimum[axis] = minimum[axis].min(point[axis] - radius);
      maximum[axis] = maximum[axis].max(point[axis] + radius);
    }
    Ok(())
  };
  match polygon {
    Polygon::Triangle(triangle) => {
      for vertex in triangle.vertices {
        include_vertex(vertex as usize, 0.0)?;
      }
    }
    Polygon::Box {
      vertices,
      ..
    } => {
      for vertex in vertices {
        include_vertex(*vertex as usize, 0.0)?;
      }
    }
    Polygon::Sphere {
      vertex,
      radius,
      ..
    } => include_vertex(*vertex as usize, *radius)?,
    Polygon::Capsule {
      vertex1,
      vertex2,
      radius,
      ..
    }
    | Polygon::Cylinder {
      vertex1,
      vertex2,
      radius,
      ..
    } => {
      include_vertex(*vertex1 as usize, *radius)?;
      include_vertex(*vertex2 as usize, *radius)?;
    }
    Polygon::Unsupported {
      ..
    } => return Err(super::invalid("unsupported polygon has no bounds")),
  }
  if minimum.iter().any(|value| !value.is_finite())
    || maximum.iter().any(|value| !value.is_finite())
  {
    return Err(super::invalid("YBN polygon has no finite bounds"));
  }
  Ok((minimum, maximum))
}

fn transform_point(
  point: [f32; 3],
  transform: Option<[f32; 16]>,
) -> [f32; 3] {
  let Some(matrix) = transform else {
    return point;
  };
  [
    point[0] * matrix[0] + point[1] * matrix[4] + point[2] * matrix[8] + matrix[12],
    point[0] * matrix[1] + point[1] * matrix[5] + point[2] * matrix[9] + matrix[13],
    point[0] * matrix[2] + point[1] * matrix[6] + point[2] * matrix[10] + matrix[14],
  ]
}

fn subtract(
  first: [f32; 3],
  second: [f32; 3],
) -> [f32; 3] {
  std::array::from_fn(|axis| first[axis] - second[axis])
}

fn cross(
  first: [f32; 3],
  second: [f32; 3],
) -> [f32; 3] {
  [
    first[1] * second[2] - first[2] * second[1],
    first[2] * second[0] - first[0] * second[2],
    first[0] * second[1] - first[1] * second[0],
  ]
}

fn length(vector: [f32; 3]) -> f32 {
  vector.iter().map(|value| value * value).sum::<f32>().sqrt()
}

fn build_geometry_bvh(
  geometry: &mut Geometry,
  bound: &Bound,
) -> io::Result<Option<GeometryBvh>> {
  geometry.polygons.retain(|polygon| !matches!(polygon, Polygon::Unsupported { .. }));
  if geometry.polygons.is_empty() {
    return Ok(None);
  }
  if geometry.polygons.len() > i16::MAX as usize {
    return Err(super::invalid("GeometryBVH has too many polygons for BVH indices"));
  }
  let items = geometry
    .polygons
    .iter()
    .enumerate()
    .map(|(index, polygon)| {
      let (minimum, maximum) = polygon_bounds(geometry, bound, polygon)?;
      Ok(BvhItem {
        index,
        minimum,
        maximum,
      })
    })
    .collect::<io::Result<Vec<_>>>()?;
  let mut root = build_bvh_node(items, 4);
  let mut next_index = 0usize;
  assign_bvh_indices(&mut root, &mut next_index);
  if next_index > i16::MAX as usize {
    return Err(super::invalid("GeometryBVH has too many BVH nodes"));
  }

  let mut order = Vec::with_capacity(geometry.polygons.len());
  collect_bvh_item_order(&root, &mut order);
  if order.len() != geometry.polygons.len() {
    return Err(super::invalid("GeometryBVH builder lost polygon items"));
  }
  let mut old_to_new = vec![0usize; order.len()];
  for (new_index, old_index) in order.iter().copied().enumerate() {
    old_to_new[old_index] = new_index;
  }
  let original = geometry.polygons.clone();
  geometry.polygons = order.iter().map(|index| original[*index]).collect();
  for polygon in &mut geometry.polygons {
    if let Polygon::Triangle(triangle) = polygon {
      for edge in &mut triangle.edge_indices {
        *edge = if (*edge as usize) < old_to_new.len() {
          u16::try_from(old_to_new[*edge as usize])
            .map_err(|_| super::invalid("GeometryBVH edge index exceeds u16"))?
        } else {
          u16::MAX
        };
      }
    }
  }
  update_triangle_metadata(geometry, bound)?;

  let center = std::array::from_fn(|axis| (root.minimum[axis] + root.maximum[axis]) * 0.5);
  let quantum = std::array::from_fn(|axis| {
    let extent =
      (root.minimum[axis] - center[axis]).abs().max((root.maximum[axis] - center[axis]).abs());
    if extent <= f32::EPSILON { 1.0 / 32767.0 } else { extent / 32767.0 }
  });
  let quantum_inverse = quantum.map(|value| 1.0 / value);
  let mut nodes = Vec::with_capacity(next_index);
  collect_bvh_nodes(&root, &old_to_new, center, quantum_inverse, &mut nodes)?;
  let mut tree_nodes = Vec::new();
  collect_bvh_trees(&root, &mut tree_nodes);
  let trees = tree_nodes
    .into_iter()
    .map(|node| {
      let end = node.index + node.total_nodes();
      Ok(BvhTreeRecord {
        minimum: quantize_bvh_bounds(node.minimum, center, quantum_inverse)?,
        maximum: quantize_bvh_bounds(node.maximum, center, quantum_inverse)?,
        first_node: i16::try_from(node.index)
          .map_err(|_| super::invalid("BVH tree start index exceeds i16"))?,
        end_node: i16::try_from(end)
          .map_err(|_| super::invalid("BVH tree end index exceeds i16"))?,
      })
    })
    .collect::<io::Result<Vec<_>>>()?;
  if trees.len() > u16::MAX as usize {
    return Err(super::invalid("GeometryBVH has too many BVH trees"));
  }
  Ok(Some(GeometryBvh {
    minimum: root.minimum,
    maximum: root.maximum,
    center,
    quantum,
    quantum_inverse,
    nodes,
    trees,
  }))
}

fn update_triangle_metadata(
  geometry: &mut Geometry,
  bound: &Bound,
) -> io::Result<()> {
  let mut edges = HashMap::<(u16, u16), Vec<(usize, usize)>>::new();
  for polygon_index in 0..geometry.polygons.len() {
    let Polygon::Triangle(mut triangle) = geometry.polygons[polygon_index] else {
      continue;
    };
    let mut points = [[0.0; 3]; 3];
    for (index, vertex_index) in triangle.vertices.iter().copied().enumerate() {
      let vertex = geometry
        .vertices
        .get(vertex_index as usize)
        .ok_or_else(|| super::invalid("triangle vertex index is out of range"))?;
      points[index] = transform_point(
        [
          vertex[0] + geometry.center[0],
          vertex[1] + geometry.center[1],
          vertex[2] + geometry.center[2],
        ],
        bound.transform,
      );
    }
    let first = subtract(points[1], points[0]);
    let second = subtract(points[2], points[0]);
    triangle.area = length(cross(first, second)) * 0.5;
    triangle.edge_indices = [u16::MAX; 3];
    for (edge_index, (first, second)) in [(0, 1), (1, 2), (2, 0)].into_iter().enumerate() {
      let first_vertex = triangle.vertices[first];
      let second_vertex = triangle.vertices[second];
      edges
        .entry((first_vertex.min(second_vertex), first_vertex.max(second_vertex)))
        .or_default()
        .push((polygon_index, edge_index));
    }
    geometry.polygons[polygon_index] = Polygon::Triangle(triangle);
  }
  for occurrences in edges.values() {
    if occurrences.len() < 2 {
      continue;
    }
    let (first_polygon, first_edge) = occurrences[0];
    let (second_polygon, second_edge) = occurrences[1];
    set_triangle_edge(&mut geometry.polygons[first_polygon], first_edge, second_polygon)?;
    set_triangle_edge(&mut geometry.polygons[second_polygon], second_edge, first_polygon)?;
    for (polygon, edge) in occurrences.iter().skip(2) {
      set_triangle_edge(&mut geometry.polygons[*polygon], *edge, first_polygon)?;
    }
  }
  Ok(())
}

fn set_triangle_edge(
  polygon: &mut Polygon,
  edge: usize,
  target: usize,
) -> io::Result<()> {
  let target =
    u16::try_from(target).map_err(|_| super::invalid("triangle edge reference exceeds u16"))?;
  if let Polygon::Triangle(triangle) = polygon {
    triangle.edge_indices[edge] = target;
  }
  Ok(())
}

fn build_bvh_node(
  mut items: Vec<BvhItem>,
  threshold: usize,
) -> BvhBuildNode {
  items.sort_by(|first, second| {
    first
      .minimum
      .iter()
      .chain(&first.maximum)
      .zip(second.minimum.iter().chain(&second.maximum))
      .map(|(first, second)| first.total_cmp(second))
      .find(|order| !order.is_eq())
      .unwrap_or_else(|| first.index.cmp(&second.index))
  });
  let mut node = BvhBuildNode {
    items,
    children: Vec::new(),
    minimum: [f32::INFINITY; 3],
    maximum: [f32::NEG_INFINITY; 3],
    index: 0,
  };
  node.update_bounds();
  if node.items.len() <= threshold {
    return node;
  }
  let mut average_sum = [0.0; 3];
  let mut counts = [0usize; 3];
  for item in &node.items {
    for (axis, sum) in average_sum.iter_mut().enumerate() {
      *sum += item.minimum[axis] + item.maximum[axis];
    }
  }
  let count = node.items.len() as f32;
  let average = average_sum.map(|sum| sum * (0.5 / count));
  for item in &node.items {
    let center = item_center(item);
    for axis in 0..3 {
      if center[axis] < average[axis] {
        counts[axis] += 1;
      }
    }
  }
  let target = count * 0.5;
  let differences = counts.map(|value| (target - value as f32).abs());
  let axis = if differences[0] <= differences[1] && differences[0] <= differences[2] {
    0
  } else if differences[1] <= differences[2] {
    1
  } else {
    2
  };
  let mut first = Vec::new();
  let mut second = Vec::new();
  for item in std::mem::take(&mut node.items) {
    if item_center(&item)[axis] > average[axis] {
      first.push(item);
    } else {
      second.push(item);
    }
  }
  if first.is_empty() || second.is_empty() {
    let mut sorted = first;
    sorted.extend(second);
    sorted.sort_by(compare_bvh_items);
    let midpoint = sorted.len() / 2;
    second = sorted.split_off(midpoint);
    first = sorted;
  }
  node.children = vec![build_bvh_node(first, threshold), build_bvh_node(second, threshold)];
  node.children.sort_by_key(|child| std::cmp::Reverse(child.total_items()));
  node.update_bounds();
  node
}

fn item_center(item: &BvhItem) -> [f32; 3] {
  std::array::from_fn(|axis| (item.minimum[axis] + item.maximum[axis]) * 0.5)
}

fn compare_bvh_items(
  first: &BvhItem,
  second: &BvhItem,
) -> std::cmp::Ordering {
  for axis in 0..3 {
    let order = first.minimum[axis].total_cmp(&second.minimum[axis]);
    if !order.is_eq() {
      return order;
    }
  }
  for axis in 0..3 {
    let order = first.maximum[axis].total_cmp(&second.maximum[axis]);
    if !order.is_eq() {
      return order;
    }
  }
  std::cmp::Ordering::Equal
}

fn assign_bvh_indices(
  node: &mut BvhBuildNode,
  next: &mut usize,
) {
  node.index = *next;
  *next += 1;
  for child in &mut node.children {
    assign_bvh_indices(child, next);
  }
}

fn collect_bvh_item_order(
  node: &BvhBuildNode,
  order: &mut Vec<usize>,
) {
  if node.children.is_empty() {
    order.extend(node.items.iter().map(|item| item.index));
  } else {
    for child in &node.children {
      collect_bvh_item_order(child, order);
    }
  }
}

fn collect_bvh_nodes(
  node: &BvhBuildNode,
  old_to_new: &[usize],
  center: [f32; 3],
  inverse: [f32; 3],
  records: &mut Vec<BvhNodeRecord>,
) -> io::Result<()> {
  let total_nodes = node.total_nodes();
  let (item_id, item_count) = if total_nodes <= 1 {
    let first = node.items.first().ok_or_else(|| super::invalid("BVH leaf has no items"))?;
    (old_to_new[first.index], node.total_items())
  } else {
    (total_nodes, 0)
  };
  records.push(BvhNodeRecord {
    minimum: quantize_bvh_bounds(node.minimum, center, inverse)?,
    maximum: quantize_bvh_bounds(node.maximum, center, inverse)?,
    item_id: i16::try_from(item_id).map_err(|_| super::invalid("BVH item id exceeds i16"))?,
    item_count: i16::try_from(item_count)
      .map_err(|_| super::invalid("BVH item count exceeds i16"))?,
  });
  for child in &node.children {
    collect_bvh_nodes(child, old_to_new, center, inverse, records)?;
  }
  Ok(())
}

fn collect_bvh_trees<'a>(
  node: &'a BvhBuildNode,
  trees: &mut Vec<&'a BvhBuildNode>,
) {
  if node.total_nodes() > 127 && !node.children.is_empty() {
    for child in &node.children {
      collect_bvh_trees(child, trees);
    }
  } else {
    trees.push(node);
  }
}

fn quantize_bvh_bounds(
  bounds: [f32; 3],
  center: [f32; 3],
  inverse: [f32; 3],
) -> io::Result<[i16; 3]> {
  let mut result = [0; 3];
  for axis in 0..3 {
    let value = (bounds[axis] - center[axis]) * inverse[axis];
    if !value.is_finite() {
      return Err(super::invalid("GeometryBVH quantized bound is not finite"));
    }
    result[axis] = value.clamp(-32767.0, 32767.0) as i16;
  }
  Ok(result)
}

fn encode_geometry_bvh_data(
  data: &mut Vec<u8>,
  bvh: &GeometryBvh,
) -> io::Result<usize> {
  if bvh.nodes.len() > u32::MAX as usize || bvh.trees.len() > u16::MAX as usize {
    return Err(super::invalid("GeometryBVH node/tree count exceeds format limits"));
  }
  let header = super::reserve(data, 128)?;
  let mut nodes = Vec::with_capacity(bvh.nodes.len() * 16);
  for node in &bvh.nodes {
    for value in node.minimum.into_iter().chain(node.maximum) {
      nodes.extend_from_slice(&value.to_le_bytes());
    }
    nodes.extend_from_slice(&node.item_id.to_le_bytes());
    nodes.extend_from_slice(&node.item_count.to_le_bytes());
  }
  let node_data = super::append(data, &nodes)?;
  let mut trees = Vec::with_capacity(bvh.trees.len() * 16);
  for tree in &bvh.trees {
    for value in tree.minimum.into_iter().chain(tree.maximum) {
      trees.extend_from_slice(&value.to_le_bytes());
    }
    trees.extend_from_slice(&tree.first_node.to_le_bytes());
    trees.extend_from_slice(&tree.end_node.to_le_bytes());
  }
  let tree_data = super::append(data, &trees)?;
  super::put_u64(data, header, super::pointer_if_nonempty(node_data, nodes.len())?)?;
  super::put_u32(data, header + 8, bvh.nodes.len() as u32)?;
  super::put_u32(data, header + 12, bvh.nodes.len() as u32)?;
  for (offset, vector) in [
    (32, bvh.minimum),
    (48, bvh.maximum),
    (64, bvh.center),
    (80, bvh.quantum_inverse),
    (96, bvh.quantum),
  ] {
    super::write_vec3(data, header + offset, vector)?;
    super::write_f32(data, header + offset + 12, f32::NAN)?;
  }
  super::put_u64(data, header + 112, super::pointer_if_nonempty(tree_data, trees.len())?)?;
  super::put_u16(data, header + 120, bvh.trees.len() as u16)?;
  super::put_u16(data, header + 122, bvh.trees.len() as u16)?;
  super::put_u32(data, header + 124, 0)?;
  Ok(header)
}

impl BvhBuildNode {
  fn update_bounds(&mut self) {
    self.minimum = [f32::INFINITY; 3];
    self.maximum = [f32::NEG_INFINITY; 3];
    for item in &self.items {
      for axis in 0..3 {
        self.minimum[axis] = self.minimum[axis].min(item.minimum[axis]);
        self.maximum[axis] = self.maximum[axis].max(item.maximum[axis]);
      }
    }
    for child in &self.children {
      for axis in 0..3 {
        self.minimum[axis] = self.minimum[axis].min(child.minimum[axis]);
        self.maximum[axis] = self.maximum[axis].max(child.maximum[axis]);
      }
    }
  }

  fn total_nodes(&self) -> usize {
    1 + self.children.iter().map(BvhBuildNode::total_nodes).sum::<usize>()
  }

  fn total_items(&self) -> usize {
    self.items.len() + self.children.iter().map(BvhBuildNode::total_items).sum::<usize>()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::Path;

  #[test]
  fn builder_splits_polygons_and_repairs_triangle_edges() {
    let path =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/ybn_conflicts/geometry_bvh.ybn.xml");
    let xml = std::fs::read_to_string(path).unwrap();
    let root = super::super::parse_xml(&xml).unwrap();
    let bounds =
      super::super::read_bound_xml(super::super::child(&root, "Bounds").unwrap(), None).unwrap();
    let mut geometry_bound = bounds.children[0].clone();
    let mut geometry = geometry_bound.geometry.take().unwrap();
    let bvh = build_geometry_bvh(&mut geometry, &geometry_bound).unwrap().unwrap();
    assert_eq!(geometry.polygons.len(), 6);
    assert!(bvh.nodes.len() > 1);
    assert_eq!(bvh.trees.len(), 1);
    assert!(
      geometry
        .polygons
        .iter()
        .all(|polygon| { matches!(polygon, Polygon::Triangle(triangle) if triangle.area > 0.0) })
    );
    assert!(geometry.polygons.iter().any(|polygon| {
      matches!(polygon, Polygon::Triangle(triangle) if triangle.edge_indices.iter().any(|edge| *edge != u16::MAX))
    }));
  }

  #[test]
  fn native_geometry_bvh_writes_pointer_nodes_and_tree_records() {
    let path =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/ybn_conflicts/geometry_bvh.ybn.xml");
    let xml = std::fs::read_to_string(path).unwrap();
    let bytes = super::super::xml_to_ybn(&xml).unwrap();
    let resource = super::super::Rsc7Resource::decode(&bytes).unwrap();
    let root = resource.read_address(super::super::BASE, super::super::COMPOSITE_SIZE).unwrap();
    let children_pointer = super::super::read_u64(root, 112).unwrap();
    let child_pointer =
      super::super::read_u64(resource.read_address(children_pointer, 8).unwrap(), 0).unwrap();
    let geometry = resource.read_address(child_pointer, super::super::GEOMETRY_BVH_SIZE).unwrap();
    let bvh_pointer = super::super::read_u64(geometry, 304).unwrap();
    assert!(bvh_pointer >= super::super::BASE);
    let bvh = resource.read_address(bvh_pointer, 128).unwrap();
    let nodes_pointer = super::super::read_u64(bvh, 0).unwrap();
    let nodes_count = super::super::read_u32(bvh, 8).unwrap();
    let trees_pointer = super::super::read_u64(bvh, 112).unwrap();
    let trees_count = super::super::read_u16(bvh, 120).unwrap();
    assert!(nodes_count > 1);
    assert!(trees_count > 0);
    assert_eq!(
      resource.read_address(nodes_pointer, nodes_count as usize * 16).unwrap().len(),
      nodes_count as usize * 16
    );
    assert_eq!(
      resource.read_address(trees_pointer, trees_count as usize * 16).unwrap().len(),
      trees_count as usize * 16
    );
  }
}
