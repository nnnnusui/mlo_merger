use serde::{Deserialize, Serialize};

use crate::core::{common::position::Position, format::xml::XmlValueAttr};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XmlPositionAttr {
  #[serde(rename = "@x")]
  x: f32,
  #[serde(rename = "@y")]
  y: f32,
  #[serde(rename = "@z")]
  z: f32,
}

impl From<XmlPositionAttr> for Position {
  fn from(v: XmlPositionAttr) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
    }
  }
}

impl From<Position> for XmlPositionAttr {
  fn from(v: Position) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
    }
  }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XmlPositionChildValueAttr {
  pub x: XmlValueAttr<f32>,
  pub y: XmlValueAttr<f32>,
  pub z: XmlValueAttr<f32>,
}

impl From<XmlPositionChildValueAttr> for Position {
  fn from(v: XmlPositionChildValueAttr) -> Self {
    Self {
      x: v.x.value,
      y: v.y.value,
      z: v.z.value,
    }
  }
}

impl From<Position> for XmlPositionChildValueAttr {
  fn from(v: Position) -> Self {
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
