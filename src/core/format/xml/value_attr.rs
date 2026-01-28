use serde::{Deserialize, Serialize};

/// Generic wrapper for XML attributes with a single `value` attribute
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct XmlValueAttr<T> {
  #[serde(rename = "@value", default)]
  pub value: T,
}
