use serde::{Deserialize, Serialize};

use crate::core::{
  common::position::Position,
  format::{
    xml::{
      position::XmlPositionChildValueAttr, serialize_break_line_par_10::serialize_break_line_par_10,
    },
    ymap::model::YmapLodLight,
  },
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

impl From<XmlYmapLodLightsSoa> for Vec<YmapLodLight> {
  fn from(v: XmlYmapLodLightsSoa) -> Self {
    // If there's an error, log it and return empty data
    if let Some(error) = v.error {
      log::warn!("LODLightsSOA error: {}", error);
      return Self::default();
    }

    let direction = v.direction.items.into_iter().map(Position::from).collect::<Vec<_>>();
    let falloffs = v.falloff.text.split_whitespace().collect::<Vec<_>>();
    let falloff_exponents = v.falloff_exponent.text.split_whitespace().collect::<Vec<_>>();
    let time_and_state_flags = v.time_and_state_flags.text.split_whitespace().collect::<Vec<_>>();
    let hashes = v.hash.text.split_whitespace().collect::<Vec<_>>();
    let cone_inner_angles = v.cone_inner_angle.text.split_whitespace().collect::<Vec<_>>();
    let cone_outer_angles_or_cap_exts =
      v.cone_outer_angle_or_cap_ext.text.split_whitespace().collect::<Vec<_>>();
    let corona_intensities = v.corona_intensity.text.split_whitespace().collect::<Vec<_>>();

    (0..direction.len())
      .map(|i| YmapLodLight {
        direction: direction[i].clone(),
        falloff: falloffs.get(i).unwrap().parse::<f32>().unwrap(),
        falloff_exponent: falloff_exponents.get(i).unwrap().parse::<f32>().unwrap(),
        time_and_state_flags: time_and_state_flags.get(i).unwrap().to_string(),
        hash: hashes.get(i).unwrap().to_string(),
        cone_inner_angle: cone_inner_angles.get(i).unwrap().parse::<u32>().unwrap(),
        cone_outer_angle_or_cap_ext: cone_outer_angles_or_cap_exts
          .get(i)
          .unwrap()
          .parse::<u32>()
          .unwrap(),
        corona_intensity: corona_intensities.get(i).unwrap().parse::<f32>().unwrap(),
      })
      .collect()
  }
}

impl From<Vec<YmapLodLight>> for XmlYmapLodLightsSoa {
  fn from(v: Vec<YmapLodLight>) -> Self {
    let direction: Vec<Position> = v.iter().map(|light| light.direction.clone()).collect();
    let falloff: Vec<String> = v.iter().map(|light| light.falloff.to_string()).collect();
    let falloff_exponent: Vec<String> =
      v.iter().map(|light| light.falloff_exponent.to_string()).collect();
    let time_and_state_flags: Vec<String> =
      v.iter().map(|light| light.time_and_state_flags.clone()).collect();
    let hash: Vec<String> = v.iter().map(|light| light.hash.clone()).collect();
    let cone_inner_angle: Vec<String> =
      v.iter().map(|light| light.cone_inner_angle.to_string()).collect();
    let cone_outer_angle_or_cap_ext: Vec<String> =
      v.iter().map(|light| light.cone_outer_angle_or_cap_ext.to_string()).collect();
    let corona_intensity: Vec<String> =
      v.iter().map(|light| light.corona_intensity.to_string()).collect();

    Self {
      error: None,
      direction: XmlYmapDirectionList {
        items: direction.into_iter().map(XmlPositionChildValueAttr::from).collect(),
      },
      falloff: XmlYmapNumberList {
        text: falloff.join(" "),
      },
      falloff_exponent: XmlYmapNumberList {
        text: falloff_exponent.join(" "),
      },
      time_and_state_flags: XmlYmapNumberList {
        text: time_and_state_flags.join(" "),
      },
      hash: XmlYmapNumberList {
        text: hash.join(" "),
      },
      cone_inner_angle: XmlYmapNumberList {
        text: cone_inner_angle.join(" "),
      },
      cone_outer_angle_or_cap_ext: XmlYmapNumberList {
        text: cone_outer_angle_or_cap_ext.join(" "),
      },
      corona_intensity: XmlYmapNumberList {
        text: corona_intensity.join(" "),
      },
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlYmapDirectionList {
  #[serde(rename = "$value", default)]
  pub items: Vec<XmlPositionChildValueAttr>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlYmapNumberList {
  #[serde(rename = "$value", default, serialize_with = "serialize_break_line_par_10")]
  pub text: String,
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

    let converted: Vec<YmapLodLight> = parsed.into();
    assert_eq!(converted.len(), 2);
    assert_eq!(converted[0].direction.x, 0.0);
    assert_eq!(converted[0].direction.y, 0.0);
    assert_eq!(converted[0].direction.z, -1.0);
    assert_eq!(converted[1].direction.x, 1.0);
    assert_eq!(converted[1].direction.y, 0.0);
    assert_eq!(converted[1].direction.z, 0.0);
    assert_eq!(converted[0].falloff, 8.0);
    assert_eq!(converted[1].falloff, 4.2);
    assert_eq!(converted[0].falloff_exponent, 128.0);
    assert_eq!(converted[1].falloff_exponent, 8.0);
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

    let converted: Vec<YmapLodLight> = parsed.into();
    assert_eq!(converted.len(), 0);
  }

  #[test]
  fn test_parse_empty_lod_lights() {
    let xml = r#"<LODLightsSOA />"#;

    let parsed: XmlYmapLodLightsSoa = quick_xml::de::from_str(xml).unwrap();
    assert!(parsed.error.is_none());
    assert_eq!(parsed.direction.items.len(), 0);

    let converted: Vec<YmapLodLight> = parsed.into();
    assert_eq!(converted.len(), 0);
  }
}
