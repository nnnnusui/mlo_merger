use serde::{Deserialize, Serialize, Serializer};

use crate::core::{
  common::{position::Position, triangle::Triangle},
  format::{
    xml::{
      XmlValueAttr, deserialize_trim_and_minify::deserialize_trim_and_minify,
      position::XmlPositionAttr,
    },
    ymap::model::YmapOccludeModel,
  },
};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlYmapOccludeModel {
  pub bmin: XmlPositionAttr,
  pub bmax: XmlPositionAttr,
  pub data_size: XmlValueAttr<u32>,
  pub verts: XmlYmapVertsAttr,
  pub num_verts_in_bytes: XmlValueAttr<u32>,
  pub num_tris: XmlValueAttr<u32>,
  pub flags: XmlValueAttr<u32>,
}
impl From<XmlYmapOccludeModel> for YmapOccludeModel {
  fn from(v: XmlYmapOccludeModel) -> Self {
    // Parse hex bytes from the verts value
    let verts = parse_hex_bytes(&v.verts.value);
    let indices_offset = v.num_verts_in_bytes.value;
    let vertices = parse_vertices(&verts[..indices_offset as usize]);
    let indices = &verts[indices_offset as usize..];
    let triangles = parse_tirangles(indices, vertices);

    Self {
      bmin: v.bmin.into(),
      bmax: v.bmax.into(),
      triangles,
      flags: v.flags.value,
    }
  }
}

impl From<YmapOccludeModel> for XmlYmapOccludeModel {
  fn from(v: YmapOccludeModel) -> Self {
    // Collect unique vertices and build index mapping
    let mut index_cache: Vec<Position> = Vec::new();
    let mut indices: Vec<u8> = Vec::new();

    for triangle in &v.triangles {
      for vert in [&triangle.corner_1, &triangle.corner_2, &triangle.corner_3] {
        if let Some(index) = index_cache.iter().position(|it| it == vert) {
          indices.push(index as u8);
        } else {
          indices.push(index_cache.len() as u8);
          index_cache.push(vert.clone());
        }
      }
    }

    let mut verts: Vec<u8> =
      index_cache.iter().flat_map(|it| [it.x, it.y, it.z]).flat_map(|f| f.to_le_bytes()).collect();
    verts.extend(indices.iter().copied());

    let verts_hex_str = format_hex_bytes(&verts);

    Self {
      bmax: v.bmax.into(),
      bmin: v.bmin.into(),
      data_size: XmlValueAttr {
        value: verts.len() as u32,
      },
      verts: XmlYmapVertsAttr {
        value: verts_hex_str,
      },
      num_verts_in_bytes: XmlValueAttr {
        value: (index_cache.len() * 12) as u32,
      },
      num_tris: XmlValueAttr {
        value: (indices.len() / 3 + 32768) as u32,
      },
      flags: XmlValueAttr {
        value: v.flags,
      },
    }
  }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct XmlYmapVertsAttr {
  #[serde(
    rename = "$value",
    default,
    deserialize_with = "deserialize_trim_and_minify",
    serialize_with = "XmlYmapVertsAttr::serialize_with"
  )]
  pub value: String,
}

impl XmlYmapVertsAttr {
  pub fn serialize_with<S>(
    x: &str,
    s: S,
  ) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let items: Vec<&str> = x.split_whitespace().collect();
    let lines: Vec<String> = items.chunks(32).map(|chunk| chunk.join(" ")).collect();
    let indent = "\n          ";
    let result = indent.to_string() + &lines.join(indent) + "\n        ";
    s.serialize_str(&result)
  }
}

fn format_hex_bytes(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
}

/// Parse hex string like "0xE0 0xF2 0x9A ..." into Vec<u8>
fn parse_hex_bytes(s: &str) -> Vec<u8> {
  s.split_whitespace()
    .filter_map(|hex| u8::from_str_radix(hex.strip_prefix("0x").unwrap_or(hex), 16).ok())
    .collect()
}

fn parse_vertices(verts: &[u8]) -> Vec<Position> {
  verts
    .chunks_exact(12)
    .map(|chunk| Position {
      x: f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
      y: f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
      z: f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
    })
    .collect()
}

fn parse_tirangles(
  indices: &[u8],
  vertices: Vec<Position>,
) -> Vec<Triangle> {
  indices
    .chunks_exact(3)
    .map(|chunk| Triangle {
      corner_1: vertices[chunk[0] as usize].clone(),
      corner_2: vertices[chunk[1] as usize].clone(),
      corner_3: vertices[chunk[2] as usize].clone(),
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use quick_xml::de::from_str;
  use regex::Regex;

  use super::*;

  #[test]
  fn test_parse_hex_bytes_with_prefix() -> Result<(), Box<dyn std::error::Error>> {
    let xml = r#"
      <Item>
        <bmin x="-369.255859" y="-818.564453" z="26.72461" />
        <bmax x="-257.018555" y="-677.0254" z="109.213867" />
        <dataSize value="3045" />
        <verts>
          80 CE B1 C3 E0 F4 30 C4 00 3B 16 42 70 66 A9 C3 90 F4 30 C4 80 3E 16 42 B0 D7 AA C3 C8 82 34 C4
          80 3F 16 42 40 D2 A9 C3 B0 19 33 C4 00 3C 16 42 E0 96 9E C3 C8 24 35 C4 00 B9 0C 42 C0 3F AD C3
          F8 3F 36 C4 80 D0 16 42 20 9C 9F C3 C8 8D 36 C4 80 06 20 42 C0 0B 9C C3 48 63 33 C4 00 BA 0C 42
          C0 27 9F C3 58 D0 38 C4 80 F2 20 42 00 22 B4 C3 A0 05 3B C4 80 22 50 42 40 4E A6 C3 38 98 37 C4
          80 CB 50 42 20 CF A3 C3 E0 46 34 C4 00 04 50 42 20 4D A6 C3 70 99 37 C4 80 F0 50 42 80 7C 98 C3
          28 09 34 C4 80 B8 0C 42 10 E0 A9 C3 40 2D 33 C4 00 5C 50 42 70 CF B1 C3 60 C3 35 C4 80 1E 50 42
          80 56 9C C3 B0 53 39 C4 80 06 20 42 50 70 9B C3 78 17 38 C4 80 B8 0C 42 C0 27 9F C3 58 D0 38 C4
          80 B8 0C 42 80 56 9C C3 B0 53 39 C4 80 B8 0C 42 C0 0B 9C C3 48 63 33 C4 00 93 20 42 00 68 9B C3
          18 0C 38 C4 80 06 20 42 80 7C 98 C3 28 09 34 C4 80 06 20 42 E0 96 9E C3 C8 24 35 C4 00 92 20 42
          10 41 9C C3 A0 71 39 C4 00 24 50 42 D0 A4 9F C3 90 30 41 C4 80 23 50 42 20 9C 9F C3 C8 8D 36 C4
          00 B9 0C 42 70 66 A9 C3 90 F4 30 C4 00 15 2A 42 40 D2 A9 C3 B0 19 33 C4 00 15 2A 42 B0 D7 AA C3
          C0 82 34 C4 80 EF 29 42 80 CE B1 C3 E0 F4 30 C4 80 89 29 42 50 3F AD C3 30 40 36 C4 00 C5 03 42
          C0 D7 AA C3 C0 82 34 C4 80 F8 02 42 C0 3F AD C3 F8 3F 36 C4 00 EF 29 42 20 9C 9F C3 C8 8D 36 C4
          00 4D 33 42 30 3A 91 C3 F8 30 2A C4 80 85 00 42 80 49 96 C3 E0 13 2A C4 00 DE FE 41 C0 71 9C C3
          50 AD 32 C4 00 CC D5 41 C0 27 9F C3 58 D0 38 C4 00 4D 33 42 50 1E 98 C3 80 6E 33 C4 00 ED D5 41
          80 CE B1 C3 E0 F4 30 C4 00 D0 3C 42 70 66 A9 C3 90 F4 30 C4 80 1E 3D 42 40 D2 A9 C3 B0 19 33 C4
          80 1E 3D 42 70 66 A9 C3 90 F4 30 C4 00 24 50 42 80 CE B1 C3 D0 51 33 C4 00 23 50 42 B0 D7 AA C3
          C0 82 34 C4 00 62 3D 42 20 9C 9F C3 C8 8D 36 C4 00 29 47 42 C0 27 9F C3 58 D0 38 C4 00 29 47 42
          10 D2 A9 C3 B8 19 33 C4 00 24 50 42 80 CE B1 C3 F0 58 35 C4 80 FA 3C 42 E0 96 9E C3 C8 24 35 C4
          80 9B 33 42 80 7C 98 C3 28 09 34 C4 00 4D 33 42 80 56 9C C3 B0 53 39 C4 00 4D 33 42 D0 E7 8D C3
          E0 BA 41 C4 00 D0 3C 42 E0 40 9C C3 78 71 39 C4 00 CE 3C 42 E0 4E 9B C3 F0 1F 38 C4 80 CF 3C 42
          70 B8 88 C3 F8 84 3B C4 00 D0 3C 42 E0 A1 9F C3 D0 2C 41 C4 80 93 29 42 00 22 B4 C3 F0 58 35 C4
          00 90 29 42 00 22 B4 C3 A0 64 38 C4 80 93 29 42 E0 96 9E C3 C8 24 35 C4 00 9E 46 42 C0 0B 9C C3
          48 63 33 C4 00 9F 46 42 80 7C 98 C3 28 09 34 C4 00 9E 46 42 C0 27 9F C3 58 D0 38 C4 00 33 47 42
          80 56 9C C3 B0 53 39 C4 80 CA 46 42 20 9C 9F C3 C8 8D 36 C4 80 57 33 42 B0 D7 AA C3 C0 82 34 C4
          80 F9 29 42 C0 3F AD C3 F8 3F 36 C4 80 F9 29 42 C0 27 9F C3 58 D0 38 C4 80 57 33 42 F0 F1 B2 C3
          F0 58 35 C4 00 EA 29 42 E0 40 9C C3 78 71 39 C4 00 92 29 42 10 4E AD C3 28 53 36 C4 80 20 2A 42
          70 66 A9 C3 90 F4 30 C4 00 1F 2A 42 80 CE B1 C3 E0 F4 30 C4 80 93 29 42 A0 E6 8D C3 90 BA 41 C4
          80 89 29 42 E0 40 9C C3 78 71 39 C4 80 87 29 42 D0 4E 9B C3 F0 1F 38 C4 00 89 29 42 E0 A1 9F C3
          D0 2C 41 C4 80 89 29 42 F0 F1 B2 C3 F0 58 35 C4 80 DF 29 42 10 4E AD C3 28 53 36 C4 00 16 2A 42
          70 B8 88 C3 F8 84 3B C4 80 89 29 42 00 22 B4 C3 F0 58 35 C4 00 86 29 42 40 D2 A9 C3 B0 19 33 C4
          80 1F 2A 42 E0 96 9E C3 C8 24 35 C4 80 9C 20 42 00 22 B4 C3 A0 64 38 C4 80 89 29 42 E0 96 9E C3
          C8 24 35 C4 00 A6 33 42 80 56 9C C3 B0 53 39 C4 00 57 33 42 80 7C 98 C3 28 09 34 C4 00 57 33 42
          20 9C 9F C3 C8 8D 36 C4 80 33 47 42 C0 0B 9C C3 48 63 33 C4 80 9D 20 42 C0 0B 9C C3 48 63 33 C4
          00 A7 33 42 70 66 A9 C3 90 F4 30 C4 00 29 3D 42 40 D2 A9 C3 B0 19 33 C4 00 29 3D 42 80 CE B1 C3
          E0 F4 30 C4 00 DA 3C 42 20 9C 9F C3 C8 8D 36 C4 00 11 20 42 B0 D7 AA C3 C0 82 34 C4 00 6C 3D 42
          80 CE B1 C3 F0 58 35 C4 00 05 3D 42 C0 3F AD C3 F8 3F 36 C4 00 1F 3D 42 00 68 9B C3 18 0C 38 C4
          80 10 20 42 80 7C 98 C3 28 09 34 C4 80 10 20 42 80 56 9C C3 B0 53 39 C4 80 10 20 42 C0 27 9F C3
          58 D0 38 C4 00 FD 20 42 C0 3F AD C3 F8 3F 36 C4 00 DB 16 42 A0 E6 8D C3 98 BA 41 C4 00 43 16 42
          E0 40 9C C3 78 71 39 C4 00 8D 16 42 D0 4E 9B C3 F0 1F 38 C4 80 90 16 42 B0 D7 AA C3 C8 82 34 C4
          80 49 16 42 C0 F8 84 C3 A0 33 3C C4 80 3B 16 42 10 4E AD C3 28 53 36 C4 00 D0 3C 42 E0 A1 9F C3
          D0 2C 41 C4 00 D0 3C 42 10 4E AD C3 28 53 36 C4 00 DA 3C 42 E0 A1 9F C3 D0 2C 41 C4 00 DA 3C 42
          E0 40 9C C3 78 71 39 C4 80 D8 3C 42 00 22 B4 C3 F0 58 35 C4 80 D6 3C 42 00 22 B4 C3 C8 A3 39 C4
          00 DA 3C 42 70 66 A9 C3 90 F4 30 C4 80 48 16 42 80 CE B1 C3 E0 F4 30 C4 00 45 16 42 90 C6 AA C3
          08 55 34 C4 00 75 F4 41 C0 D0 AA C3 48 56 34 C4 00 A8 10 42 30 EC A9 C3 A8 FE 31 C4 00 A8 10 42
          00 E2 A9 C3 68 FD 31 C4 00 75 F4 41 90 B0 8E C3 20 97 41 C4 80 E4 50 42 60 52 9B C3 38 22 38 C4
          80 19 50 42 70 B8 88 C3 F8 84 3B C4 80 19 50 42 40 D2 A9 C3 B0 19 33 C4 00 46 16 42 E0 96 9E C3
          C8 24 35 C4 00 C3 0C 42 C0 0B 9C C3 48 63 33 C4 80 C4 0C 42 10 41 9C C3 A0 71 39 C4 80 19 50 42
          D0 A4 9F C3 90 30 41 C4 00 19 50 42 00 22 B4 C3 A0 05 3B C4 00 18 50 42 20 4D A6 C3 70 99 37 C4
          80 E6 50 42 80 7C 98 C3 28 09 34 C4 00 C3 0C 42 50 70 9B C3 78 17 38 C4 00 C3 0C 42 C0 27 9F C3
          58 D0 38 C4 00 C3 0C 42 40 4E A6 C3 38 98 37 C4 80 C1 50 42 20 CF A3 C3 E0 46 34 C4 80 F9 4F 42
          80 56 9C C3 B0 53 39 C4 00 C3 0C 42 20 9C 9F C3 C8 8D 36 C4 00 C3 0C 42 70 CF B1 C3 60 C3 35 C4
          00 14 50 42 10 E0 A9 C3 40 2D 33 C4 80 51 50 42 50 3F AD C3 30 40 36 C4 80 CF 03 42 80 CE B1 C3
          D0 51 33 C4 80 18 50 42 70 66 A9 C3 90 F4 30 C4 80 19 50 42 C0 D7 AA C3 C0 82 34 C4 00 03 03 42
          10 D2 A9 C3 B8 19 33 C4 80 19 50 42 E0 96 9E C3 C8 24 35 C4 80 93 46 42 C0 0B 9C C3 48 63 33 C4
          00 95 46 42 80 7C 98 C3 28 09 34 C4 80 93 46 42 80 56 9C C3 B0 53 39 C4 80 C0 46 42 E0 03 AA C3
          A8 33 33 C4 00 76 4E 42 D0 12 AA C3 60 34 33 C4 00 54 F4 41 F0 CF AA C3 88 63 33 C4 00 77 F4 41
          D0 E0 AA C3 78 59 34 C4 00 75 F4 41 20 DE AA C3 70 5B 34 C4 00 31 4C 42 90 18 9F C3 C0 7E 36 C4
          00 76 F4 41 00 28 9F C3 B0 8C 36 C4 80 37 55 42 20 41 9E C3 F0 59 35 C4 00 75 F4 41 C0 42 9E C3
          40 59 35 C4 00 87 52 42 C0 0B 9C C3 48 63 33 C4 00 9D 33 42 20 21 99 C3 78 11 35 C4 00 10 0C 42
          60 2A 99 C3 98 14 35 C4 00 75 F4 41 90 B0 8E C3 20 97 41 C4 80 EE 50 42 70 B8 88 C3 F8 84 3B C4
          00 24 50 42 60 52 9B C3 38 22 38 C4 00 24 50 42 F0 FB 97 C3 F0 7A 33 C4 00 6B 01 42 00 EB A7 C3
          20 A4 4C C4 00 3A F3 41 60 82 80 C3 48 FC 33 C4 80 9B 00 42 80 62 9C C3 00 98 32 C4 00 3D FE 41
          A0 42 A6 C3 48 98 37 C4 00 0B 03 42 F0 32 A4 C3 B8 D1 34 C4 80 91 02 42 20 3C AB C3 F0 97 33 C4
          00 76 01 42 F0 3D 8D C3 A8 DF 2B C4 80 F4 36 42 90 A8 91 C3 E8 02 32 C4 80 F4 36 42 A0 AF 91 C3
          A0 01 32 C4 40 67 DA 42 20 7C B6 C3 00 BE 33 C4 00 92 F8 41 10 D7 AF C3 A0 19 3F C4 00 22 F6 41
          D0 E7 8D C3 E0 BA 41 C4 00 DA 3C 42 E0 4E 9B C3 F0 1F 38 C4 00 DA 3C 42 E0 8D AD C3 80 BF 2D C4
          00 A3 F7 41 70 B8 88 C3 F8 84 3B C4 00 DA 3C 42 00 22 B4 C3 C8 A3 39 C4 00 D0 3C 42 00 22 B4 C3
          F0 58 35 C4 80 CC 3C 42 A0 19 AA C3 F0 7C 33 C4 00 FC 02 42 70 CE B1 C3 78 55 33 C4 00 FC 02 42
          D0 50 9B C3 A0 08 38 C4 00 89 10 42 90 5C 9B C3 F0 09 38 C4 00 77 F4 41 C0 43 8D C3 E8 DC 2B C4
          80 6D DA 42 10 6F A9 C3 80 E1 30 C4 00 7A FB 41 20 D9 B1 C3 60 DD 30 C4 00 50 F9 41 E0 A1 9F C3
          D0 2C 41 C4 80 3B 16 42 00 22 B4 C3 28 19 37 C4 80 3B 16 42 C0 A0 B8 C3 A0 41 29 C4 00 AD EF 41
          10 4E AD C3 28 53 36 C4 00 3F 16 42 70 7F A8 C3 20 33 37 C4 00 3F 16 42 00 22 B4 C3 00 C7 35 C4
          80 06 03 42 10 A2 9F C3 88 2E 41 C4 80 06 03 42 10 4E AD C3 28 53 36 C4 00 03 03 42 C0 3F AD C3
          F8 3F 36 C4 80 14 3D 42 10 27 A4 C3 90 FD 37 C4 00 03 03 42 A0 E6 8D C3 90 BA 41 C4 80 93 29 42
          70 B8 88 C3 F8 84 3B C4 80 93 29 42 D0 4E 9B C3 F0 1F 38 C4 80 93 29 42 C0 F8 84 C3 A0 33 3C C4
          80 06 03 42 D0 4E 9B C3 F8 1F 38 C4 00 03 03 42 E0 40 9C C3 78 71 39 C4 00 97 16 42 00 22 B4 C3
          28 19 37 C4 00 46 16 42 E0 A1 9F C3 D0 2C 41 C4 00 46 16 42 10 4E AD C3 28 53 36 C4 80 49 16 42
          70 7F A8 C3 20 33 37 C4 80 49 16 42 D0 4E 9B C3 F0 1F 38 C4 80 9A 16 42 A0 E6 8D C3 98 BA 41 C4
          00 4D 16 42 C0 F8 84 C3 A0 33 3C C4 00 46 16 42 00 01 02 01 03 02 04 03 01 05 02 06 01 07 04 06
          08 05 09 0A 0B 0C 0A 09 04 07 0D 0B 0E 0F 06 10 08 04 0D 11 04 11 12 11 13 12 06 14 10 14 15 10
          14 16 15 06 17 14 18 0C 09 18 09 19 1A 04 12 1B 14 17 17 1C 1B 1B 1C 1D 1E 1B 1D 1A 12 1F 1F 20
          1A 21 1D 22 23 24 25 22 26 21 25 27 23 28 29 2A 0E 2B 2C 2D 2E 2F 0E 30 2B 31 2A 2D 22 32 26 32
          33 26 33 34 26 35 36 37 35 37 38 39 3A 3B 2B 30 3C 3C 3D 2B 3E 3D 3C 3F 3E 3C 3F 40 3E 41 42 43
          43 44 41 39 45 3A 39 46 47 39 47 45 42 48 49 4A 4B 4C 4D 4E 4F 4A 4C 50 4D 4F 4B 4D 51 4E 42 52
          48 48 52 53 4D 54 51 41 44 55 44 56 57 44 57 55 58 3F 3C 53 59 48 57 5A 55 55 5A 5B 5B 5C 55 5C
          5B 5D 5E 59 53 3F 58 5F 5F 5C 60 5F 61 3F 62 63 59 64 62 59 5E 64 59 5E 65 64 66 65 5E 67 68 69
          5E 6A 66 67 69 6B 6C 36 6D 6E 6F 70 6E 71 6F 71 72 6F 6A 73 74 75 76 77 77 78 75 79 7A 7B 6A 7C
          73 73 7C 7D 7D 7E 73 79 7F 7A 7F 80 81 7F 81 82 83 7E 7D 84 83 7D 85 84 7D 81 86 82 81 87 86 85
          88 84 89 85 7D 8A 8B 87 8C 85 89 8B 8D 8E 89 8F 8C 8B 8E 90 91 90 8E 8E 92 91 91 92 93 91 93 2F
          93 94 2F 95 96 97 95 97 98 95 98 99 99 98 9A 9A 9B 99 9B 9A 9C 9C 9D 9B 95 9D 9C 9C 96 95 32 9E
          33 29 9E 32 32 2A 29 2E 91 2F 78 77 9F 9F A0 78 A1 A2 A3 A4 A5 A6 A4 A7 A5 A7 A4 27 A1 A3 18 A8
          A9 AA 27 25 A7 AB AC AD A6 23 A4 A4 23 27 A7 25 24 AE AF A7 B0 B1 70 AE A7 B2 B0 B3 B1 6D B4 B5
          6C 6D B5 8F B6 B7 B8 B9 A0 A0 9F B8 AD BA AB B6 BB BC 68 BD BE B2 24 BF B2 A7 24 68 BE C0 68 C0
          C1 C2 C3 C4 2F C5 2D C3 C6 C4 C7 C8 C9 C7 C9 46 CA CB C6 C3 CA C6 CC CD CE CC CF CD CC D0 CF D1
          CC D2 D3 D1 D2
        </verts>
        <numVertsInBytes value="2544" />
        <numTris value="32935" />
        <flags value="0" />
      </Item>
    "#;
    let re = Regex::new(r"\s+").unwrap();
    let xml = re.replace_all(xml, " ");
    let parsed: XmlYmapOccludeModel = from_str(&xml)?;
    let model: YmapOccludeModel = parsed.clone().into();
    let model_to_xml: XmlYmapOccludeModel = model.into();
    let serialized = quick_xml::se::to_string(&model_to_xml).unwrap();
    let re_parsed: XmlYmapOccludeModel = from_str(&serialized).unwrap();
    assert_eq!(parsed, re_parsed);
    Ok(())
  }
}
