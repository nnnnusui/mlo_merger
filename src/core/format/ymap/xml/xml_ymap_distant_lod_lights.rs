use serde::{Deserialize, Serialize, Serializer};

use crate::core::format::{
  xml::{
    XmlValueAttr, position::XmlPositionChildValueAttr,
    serialize_break_line_par_10::serialize_break_line_par_10,
  },
  ymap::model::YmapDistantLodLightsSoa,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename = "DistantLODLightsSOA", rename_all = "camelCase")]
pub struct XmlYmapDistantLodLightsSoa {
  #[serde(default)]
  pub error: Option<String>,
  #[serde(default)]
  pub position: XmlYmapDistantLodLightsSoaPositions,
  #[serde(rename = "RGBI", default)]
  pub rgbi: XmlYmapNumberList,
  #[serde(default)]
  pub num_street_lights: XmlValueAttr<u32>,
  #[serde(default)]
  pub category: XmlValueAttr<u32>,
}

impl From<XmlYmapDistantLodLightsSoa> for YmapDistantLodLightsSoa {
  fn from(v: XmlYmapDistantLodLightsSoa) -> Self {
    // If there's an error, log it and return empty data
    if let Some(error) = v.error {
      log::warn!("DistantLODLightsSOA error: {}", error);
      return Self::default();
    }

    let position = v
      .position
      .items
      .iter()
      .map(|p| format!("{} {} {}", p.x.value, p.y.value, p.z.value))
      .collect();

    Self {
      position,
      rgbi: parse_number_list(&v.rgbi.text),
      num_street_lights: v.num_street_lights.value,
      category: v.category.value,
    }
  }
}

impl From<YmapDistantLodLightsSoa> for XmlYmapDistantLodLightsSoa {
  fn from(v: YmapDistantLodLightsSoa) -> Self {
    let position = XmlYmapDistantLodLightsSoaPositions {
      items: v
        .position
        .iter()
        .map(|pos_str| {
          let coords: Vec<&str> = pos_str.split_whitespace().collect();
          XmlPositionChildValueAttr {
            x: XmlValueAttr {
              value: coords[0].parse().unwrap_or(0.0),
            },
            y: XmlValueAttr {
              value: coords[1].parse().unwrap_or(0.0),
            },
            z: XmlValueAttr {
              value: coords[2].parse().unwrap_or(0.0),
            },
          }
        })
        .collect(),
    };

    let rgbi_text = v.rgbi.join(" ");

    Self {
      error: None,
      position,
      rgbi: XmlYmapNumberList {
        text: rgbi_text,
      },
      num_street_lights: XmlValueAttr {
        value: v.num_street_lights,
      },
      category: XmlValueAttr {
        value: v.category,
      },
    }
  }
}

impl Default for XmlYmapDistantLodLightsSoa {
  fn default() -> Self {
    Self {
      error: None,
      position: XmlYmapDistantLodLightsSoaPositions::default(),
      rgbi: XmlYmapNumberList::default(),
      num_street_lights: XmlValueAttr {
        value: 0,
      },
      category: XmlValueAttr {
        value: 0,
      },
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlYmapDistantLodLightsSoaPositions {
  #[serde(rename = "$value", default)]
  pub items: Vec<XmlPositionChildValueAttr>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlYmapNumberList {
  #[serde(rename = "$value", default, serialize_with = "serialize_break_line_par_10")]
  pub text: String,
}

fn parse_number_list(text: &str) -> Vec<String> {
  text.split_whitespace().map(|s| s.to_string()).collect()
}
