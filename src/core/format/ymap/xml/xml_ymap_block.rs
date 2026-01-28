use crate::core::format::{xml::XmlValueAttr, ymap::model::YmapBlock};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlYmapBlock {
  pub version: XmlValueAttr<u32>,
  pub flags: XmlValueAttr<u32>,
  pub name: String,
  pub exported_by: String,
  pub owner: String,
  pub time: String,
}

impl From<XmlYmapBlock> for YmapBlock {
  fn from(v: XmlYmapBlock) -> Self {
    Self {
      version: v.version.value,
      flags: v.flags.value,
      name: v.name,
      exported_by: v.exported_by,
      owner: v.owner,
      time: v.time,
    }
  }
}

impl From<YmapBlock> for XmlYmapBlock {
  fn from(v: YmapBlock) -> Self {
    Self {
      version: XmlValueAttr {
        value: v.version,
      },
      flags: XmlValueAttr {
        value: v.flags,
      },
      name: v.name,
      exported_by: v.exported_by,
      owner: v.owner,
      time: v.time,
    }
  }
}

#[cfg(test)]
mod tests {
  use quick_xml::de::from_str;

  use super::*;

  #[test]
  fn test() {
    let xml = r#"
      <XmlYmapBlock>
        <version value="0" />
        <flags value="0" />
        <name>lr_sc1_02_strm_0</name>
        <exportedBy>CodeWalker</exportedBy>
        <owner>own</owner>
        <time>01 lutego 2023 13:51</time>
      </XmlYmapBlock>
    "#;
    let parsed: XmlYmapBlock = from_str(xml).unwrap();
    assert_eq!(parsed.version.value, 0);
    assert_eq!(parsed.flags.value, 0);
    assert_eq!(parsed.name, "lr_sc1_02_strm_0");
    assert_eq!(parsed.exported_by, "CodeWalker");
    assert_eq!(parsed.owner, "own");
    assert_eq!(parsed.time, "01 lutego 2023 13:51");

    let model: YmapBlock = parsed.clone().into();
    let model_to_xml: XmlYmapBlock = model.into();
    let serialized = quick_xml::se::to_string(&model_to_xml).unwrap();
    let re_parsed: XmlYmapBlock = from_str(&serialized).unwrap();
    assert_eq!(parsed, re_parsed);
  }
}
