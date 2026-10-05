//! XML numeric parsing, flag names and formatted tag helpers.

use super::super::*;

pub(in crate::core::format::ybn) fn child<'a>(
  node: &'a XmlElement,
  name: &str,
) -> io::Result<&'a XmlElement> {
  node
    .children
    .iter()
    .find(|child| child.name == name)
    .ok_or_else(|| invalid(&format!("YBN XML missing {name}")))
}
pub(in crate::core::format::ybn) fn text(node: &XmlElement) -> String {
  node.attributes.get("value").cloned().unwrap_or_else(|| node.text.trim().to_string())
}
pub(in crate::core::format::ybn) fn number<T: std::str::FromStr>(
  node: &XmlElement,
  name: &str,
) -> io::Result<T> {
  text(child(node, name)?).parse().map_err(|_| invalid(&format!("YBN XML {name} is invalid")))
}
pub(in crate::core::format::ybn) fn vec3(node: &XmlElement) -> io::Result<[f32; 3]> {
  Ok([float_attr(node, "x")?, float_attr(node, "y")?, float_attr(node, "z")?])
}
pub(in crate::core::format::ybn) fn float_attr(
  node: &XmlElement,
  name: &str,
) -> io::Result<f32> {
  node
    .attributes
    .get(name)
    .ok_or_else(|| invalid(&format!("YBN vector missing {name}")))?
    .parse()
    .map_err(|_| invalid("YBN vector coordinate is invalid"))
}
pub(in crate::core::format::ybn) fn read_float_array(
  node: &XmlElement,
  name: &str,
  count: usize,
) -> io::Result<[f32; 16]> {
  let values = numbers(&child(node, name)?.text)?;
  if values.len() != count {
    return Err(invalid(&format!("YBN {name} requires {count} values")));
  }
  values.try_into().map_err(|_| invalid("YBN matrix has the wrong number of values"))
}
pub(in crate::core::format::ybn) fn numbers(value: &str) -> io::Result<Vec<f32>> {
  value
    .split(|c: char| c.is_whitespace() || c == ',')
    .filter(|token| !token.is_empty())
    .map(|token| token.parse().map_err(|_| invalid("YBN numeric array contains an invalid value")))
    .collect()
}
pub(in crate::core::format::ybn) fn parse_vectors(value: &str) -> io::Result<Vec<[f32; 3]>> {
  value
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| {
      let values = numbers(line)?;
      if values.len() != 3 {
        return Err(invalid("YBN vertex must have three coordinates"));
      }
      Ok([values[0], values[1], values[2]])
    })
    .collect()
}
pub(in crate::core::format::ybn) fn parse_colors(value: &str) -> io::Result<Vec<[u8; 4]>> {
  value
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| {
      let values = line
        .split(',')
        .map(str::trim)
        .map(|item| item.parse::<u8>().map_err(|_| invalid("YBN color channel is invalid")))
        .collect::<io::Result<Vec<_>>>()?;
      if values.len() != 4 {
        return Err(invalid("YBN color must have four channels"));
      }
      Ok([values[0], values[1], values[2], values[3]])
    })
    .collect()
}
pub(in crate::core::format::ybn) fn polygon_attr<T: std::str::FromStr>(
  node: &XmlElement,
  name: &str,
) -> io::Result<T> {
  node
    .attributes
    .get(name)
    .ok_or_else(|| invalid(&format!("YBN polygon missing {name}")))?
    .parse()
    .map_err(|_| invalid("YBN polygon attribute is invalid"))
}
pub(in crate::core::format::ybn) fn parse_material_flags(text: &str) -> io::Result<u16> {
  parse_named_flags(
    text,
    &[
      "FLAG_STAIRS",
      "FLAG_NOT_CLIMBABLE",
      "FLAG_SEE_THROUGH",
      "FLAG_SHOOT_THROUGH",
      "FLAG_NOT_COVER",
      "FLAG_WALKABLE_PATH",
      "FLAG_NO_CAM_COLLISION",
      "FLAG_SHOOT_THROUGH_FX",
      "FLAG_NO_DECAL",
      "FLAG_NO_NAVMESH",
      "FLAG_NO_RAGDOLL",
      "FLAG_VEHICLE_WHEEL",
      "FLAG_NO_PTFX",
      "FLAG_TOO_STEEP_FOR_PLAYER",
      "FLAG_NO_NETWORK_SPAWN",
      "FLAG_NO_CAM_COLLISION_ALLOW_CLIPPING",
    ],
  )
}
pub(in crate::core::format::ybn) fn parse_composite_flags(text: &str) -> io::Result<u32> {
  parse_named_flags(
    text,
    &[
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
    ],
  )
}
pub(in crate::core::format::ybn) fn parse_named_flags<T: TryFrom<u64>>(
  text: &str,
  names: &[&str],
) -> io::Result<T> {
  let mut bits = 0u64;
  for name in text.split(',').map(str::trim).filter(|name| !name.is_empty() && *name != "NONE") {
    let index = names
      .iter()
      .position(|candidate| *candidate == name)
      .ok_or_else(|| invalid(&format!("unknown YBN flag {name}")))?;
    bits |= 1 << index;
  }
  T::try_from(bits).map_err(|_| invalid("YBN flags exceed supported width"))
}
pub(in crate::core::format::ybn) fn material_flag_text(flags: u16) -> String {
  named_flag_text(
    flags as u64,
    &[
      "FLAG_STAIRS",
      "FLAG_NOT_CLIMBABLE",
      "FLAG_SEE_THROUGH",
      "FLAG_SHOOT_THROUGH",
      "FLAG_NOT_COVER",
      "FLAG_WALKABLE_PATH",
      "FLAG_NO_CAM_COLLISION",
      "FLAG_SHOOT_THROUGH_FX",
      "FLAG_NO_DECAL",
      "FLAG_NO_NAVMESH",
      "FLAG_NO_RAGDOLL",
      "FLAG_VEHICLE_WHEEL",
      "FLAG_NO_PTFX",
      "FLAG_TOO_STEEP_FOR_PLAYER",
      "FLAG_NO_NETWORK_SPAWN",
      "FLAG_NO_CAM_COLLISION_ALLOW_CLIPPING",
    ],
  )
}
pub(in crate::core::format::ybn) fn composite_flag_text(flags: u32) -> String {
  named_flag_text(
    flags as u64,
    &[
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
    ],
  )
}
pub(in crate::core::format::ybn) fn named_flag_text(
  flags: u64,
  names: &[&str],
) -> String {
  let selected = names
    .iter()
    .enumerate()
    .filter(|(bit, _)| flags & (1 << bit) != 0)
    .map(|(_, name)| *name)
    .collect::<Vec<_>>();
  if selected.is_empty() { "NONE".into() } else { selected.join(", ") }
}
pub(in crate::core::format::ybn) fn indent(
  xml: &mut String,
  depth: usize,
) {
  xml.extend(std::iter::repeat_n(' ', depth));
}
pub(in crate::core::format::ybn) fn val_tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: impl std::fmt::Display,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name} value=\"{value}\" />\n"));
}
pub(in crate::core::format::ybn) fn vec_tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: [f32; 3],
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name} x=\"{}\" y=\"{}\" z=\"{}\" />\n", value[0], value[1], value[2]));
}
pub(in crate::core::format::ybn) fn text_tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name}>"));
  write_text_content(xml, depth, value);
  xml.push_str(&format!("</{name}>\n"));
}
pub(in crate::core::format::ybn) fn write_array_open(
  xml: &mut String,
  depth: usize,
  name: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name}>\n"));
}
pub(in crate::core::format::ybn) fn write_array_close(
  xml: &mut String,
  depth: usize,
  name: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("</{name}>\n"));
}
pub(in crate::core::format::ybn) fn format_vectors(values: &[[f32; 3]]) -> String {
  values.iter().map(|v| format!("{}, {}, {}", v[0], v[1], v[2])).collect::<Vec<_>>().join("\n")
}
pub(in crate::core::format::ybn) fn format_colors(values: &[[u8; 4]]) -> String {
  values
    .iter()
    .map(|c| format!("{}, {}, {}, {}", c[0], c[1], c[2], c[3]))
    .collect::<Vec<_>>()
    .join("\n")
}
