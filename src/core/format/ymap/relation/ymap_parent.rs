use serde::{Deserialize, Serialize};

/// Identifies the origin that owns a referenced parent YMAP.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum YmapParentSource {
  /// A cumulative vanilla cache version ID.
  Vanilla(String),
  /// A source resource ID.
  ResourceName(String),
}

/// Identifies one parent YMAP within a vanilla version or source resource.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct YmapParent {
  pub source: YmapParentSource,
  pub ymap_name: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn serializes_the_selected_parent_source_and_camel_case_map_name() {
    let vanilla = YmapParent {
      source: YmapParentSource::Vanilla("0001-update".into()),
      ymap_name: "parent.ymap".into(),
    };
    assert_eq!(
      serde_json::to_value(vanilla).unwrap(),
      serde_json::json!({
        "source": {"Vanilla": "0001-update"},
        "ymapName": "parent.ymap"
      })
    );

    let resource = YmapParent {
      source: YmapParentSource::ResourceName("[gabz]/resource".into()),
      ymap_name: "parent.ymap".into(),
    };
    assert_eq!(
      serde_json::to_value(resource).unwrap(),
      serde_json::json!({
        "source": {"ResourceName": "[gabz]/resource"},
        "ymapName": "parent.ymap"
      })
    );
  }
}
