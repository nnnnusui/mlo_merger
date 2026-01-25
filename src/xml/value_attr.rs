use serde::Deserialize;

/// Generic wrapper for XML attributes with a single `value` attribute
#[derive(Debug, Deserialize)]
pub struct XmlValueAttr<T> {
  #[serde(rename = "@value")]
  pub value: T,
}
