use serde::{Deserialize, Serialize};

/// Identifies one parent YMAP within a vanilla version or source resource.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct YmapParent {
  /// The source resource ID owning the parent, when it comes from a resource.
  pub resource_name: Option<String>,
  /// The cumulative vanilla cache version ID.
  pub vanilla_version: String,
  pub ymap_name: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn serializes_parent_origin_and_map_name_in_camel_case() {
    let vanilla = YmapParent {
      resource_name: None,
      vanilla_version: "mp2026_01".into(),
      ymap_name: "parent.ymap".into(),
    };
    assert_eq!(
      serde_json::to_value(vanilla).unwrap(),
      serde_json::json!({
        "resourceName": null,
        "vanillaVersion": "mp2026_01",
        "ymapName": "parent.ymap"
      })
    );

    let resource = YmapParent {
      resource_name: Some("resource".into()),
      vanilla_version: "mp2026_01".into(),
      ymap_name: "parent.ymap".into(),
    };
    assert_eq!(
      serde_json::to_value(resource).unwrap(),
      serde_json::json!({
        "resourceName": "resource",
        "vanillaVersion": "mp2026_01",
        "ymapName": "parent.ymap"
      })
    );
  }
}
