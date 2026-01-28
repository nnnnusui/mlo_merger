use serde::{Deserialize, Serialize};

use crate::core::format::{
  xml::{XmlValueAttr, serialize_break_line_par_10::serialize_break_line_par_10},
  ymap::model::{Direction, YmapLodLightsSoa},
};

#[derive(Debug, Deserialize, Default, Serialize)]
#[serde(rename = "LODLightsSOA", rename_all = "camelCase")]
pub struct XmlYmapLodLightsSoa {
  #[serde(default)]
  pub error: Option<String>,
  #[serde(default)]
  pub direction: XmlYmapDirectionList,
  #[serde(default)]
  pub falloff: XmlYmapNumberList,
  #[serde(default)]
  pub falloff_exponent: XmlYmapNumberList,
  #[serde(default)]
  pub time_and_state_flags: XmlYmapNumberList,
  #[serde(default)]
  pub hash: XmlYmapNumberList,
  #[serde(default)]
  pub cone_inner_angle: XmlYmapNumberList,
  #[serde(default)]
  pub cone_outer_angle_or_cap_ext: XmlYmapNumberList,
  #[serde(default)]
  pub corona_intensity: XmlYmapNumberList,
}

impl From<XmlYmapLodLightsSoa> for YmapLodLightsSoa {
  fn from(v: XmlYmapLodLightsSoa) -> Self {
    // If there's an error, log it and return empty data
    if let Some(error) = v.error {
      log::warn!("LODLightsSOA error: {}", error);
      return Self::default();
    }

    let direction = v.direction.items.into_iter().map(Direction::from).collect();

    Self {
      direction,
      falloff: parse_number_list(&v.falloff.text),
      falloff_exponent: parse_number_list(&v.falloff_exponent.text),
      time_and_state_flags: parse_number_list(&v.time_and_state_flags.text),
      hash: parse_number_list(&v.hash.text),
      cone_inner_angle: parse_number_list(&v.cone_inner_angle.text),
      cone_outer_angle_or_cap_ext: parse_number_list(&v.cone_outer_angle_or_cap_ext.text),
      corona_intensity: parse_number_list(&v.corona_intensity.text),
    }
  }
}

impl From<YmapLodLightsSoa> for XmlYmapLodLightsSoa {
  fn from(v: YmapLodLightsSoa) -> Self {
    Self {
      error: None,
      direction: XmlYmapDirectionList {
        items: v.direction.into_iter().map(XmlYmapDirection::from).collect(),
      },
      falloff: XmlYmapNumberList {
        text: v.falloff.join(" "),
      },
      falloff_exponent: XmlYmapNumberList {
        text: v.falloff_exponent.join(" "),
      },
      time_and_state_flags: XmlYmapNumberList {
        text: v.time_and_state_flags.join(" "),
      },
      hash: XmlYmapNumberList {
        text: v.hash.join(" "),
      },
      cone_inner_angle: XmlYmapNumberList {
        text: v.cone_inner_angle.join(" "),
      },
      cone_outer_angle_or_cap_ext: XmlYmapNumberList {
        text: v.cone_outer_angle_or_cap_ext.join(" "),
      },
      corona_intensity: XmlYmapNumberList {
        text: v.corona_intensity.join(" "),
      },
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlYmapDirectionList {
  #[serde(rename = "$value", default)]
  pub items: Vec<XmlYmapDirection>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
#[serde(rename = "Item")]
pub struct XmlYmapDirection {
  #[serde(default)]
  pub x: XmlValueAttr<f32>,
  #[serde(default)]
  pub y: XmlValueAttr<f32>,
  #[serde(default)]
  pub z: XmlValueAttr<f32>,
}

impl From<XmlYmapDirection> for Direction {
  fn from(v: XmlYmapDirection) -> Self {
    Self {
      x: v.x.value,
      y: v.y.value,
      z: v.z.value,
    }
  }
}

impl From<Direction> for XmlYmapDirection {
  fn from(v: Direction) -> Self {
    Self {
      x: XmlValueAttr {
        value: v.x,
      },
      y: XmlValueAttr {
        value: v.y,
      },
      z: XmlValueAttr {
        value: v.z,
      },
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlYmapNumberList {
  #[serde(rename = "$value", default, serialize_with = "serialize_break_line_par_10")]
  pub text: String,
}

fn parse_number_list(text: &str) -> Vec<String> {
  text.split_whitespace().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_lod_lights_with_data() {
    let xml = r#"
      <LODLightsSOA>
        <direction itemType="FloatXYZ">
          <Item>
            <x value="0" />
            <y value="0" />
            <z value="-1" />
          </Item>
          <Item>
            <x value="1" />
            <y value="0" />
            <z value="0" />
          </Item>
        </direction>
        <falloff>8 4.2 9</falloff>
        <falloffExponent>128 8 8</falloffExponent>
        <timeAndStateFlags>150994943 148897919</timeAndStateFlags>
        <hash>10038078 20018242</hash>
        <coneInnerAngle>70 1 56</coneInnerAngle>
        <coneOuterAngleOrCapExt>113 45 127</coneOuterAngleOrCapExt>
        <coronaIntensity>0 0 0</coronaIntensity>
      </LODLightsSOA>
    "#;

    let parsed: XmlYmapLodLightsSoa = quick_xml::de::from_str(xml).unwrap();
    assert!(parsed.error.is_none());
    assert_eq!(parsed.direction.items.len(), 2);

    let converted: YmapLodLightsSoa = parsed.into();
    assert_eq!(converted.direction.len(), 2);
    assert_eq!(converted.direction[0].x, 0.0);
    assert_eq!(converted.direction[0].y, 0.0);
    assert_eq!(converted.direction[0].z, -1.0);
    assert_eq!(converted.direction[1].x, 1.0);
    assert_eq!(converted.direction[1].y, 0.0);
    assert_eq!(converted.direction[1].z, 0.0);
    assert_eq!(converted.falloff, vec!["8", "4.2", "9"]);
    assert_eq!(converted.falloff_exponent, vec!["128", "8", "8"]);
  }

  #[test]
  fn test_parse_lod_lights_with_error() {
    let xml = r#"
      <LODLightsSOA>
        <error>Couldn't find structure info CLODLight!</error>
      </LODLightsSOA>
    "#;

    let parsed: XmlYmapLodLightsSoa = quick_xml::de::from_str(xml).unwrap();
    assert!(parsed.error.is_some());
    assert_eq!(parsed.error.as_ref().unwrap(), "Couldn't find structure info CLODLight!");

    let converted: YmapLodLightsSoa = parsed.into();
    assert_eq!(converted.direction.len(), 0);
    assert_eq!(converted.falloff.len(), 0);
  }

  #[test]
  fn test_parse_empty_lod_lights() {
    let xml = r#"<LODLightsSOA />"#;

    let parsed: XmlYmapLodLightsSoa = quick_xml::de::from_str(xml).unwrap();
    assert!(parsed.error.is_none());
    assert_eq!(parsed.direction.items.len(), 0);

    let converted: YmapLodLightsSoa = parsed.into();
    assert_eq!(converted.direction.len(), 0);
  }

  #[test]
  fn test_parse_number_list() {
    assert_eq!(parse_number_list("1 2 3"), vec!["1", "2", "3"]);
    assert_eq!(parse_number_list("  4.5  6.7  "), vec!["4.5", "6.7"]);
    assert_eq!(parse_number_list(""), Vec::<String>::new());
  }
}
