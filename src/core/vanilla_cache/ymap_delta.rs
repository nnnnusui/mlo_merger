//! Lossless parsed-model deltas for vanilla history; independent of MLO merge policy.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use structdiff::StructDiff;

use crate::core::format::ymap::model::{Ymap, YmapStructDiffEnum};

use super::CachedFile;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize, Serialize)]
enum DeltaFormat {
  #[serde(rename = "vanilla_ymap_delta_v1")]
  VanillaYmapV1,
}

/// Exact parsed-YMAP changes, with a predecessor reference and model integrity checks.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VanillaYmapDelta {
  format: DeltaFormat,
  /// Previous native artifact or delta from which this change was derived.
  pub(crate) base: CachedFile,
  /// Original target binary hash; applying a model delta does not recreate its compressed bytes.
  pub(crate) target_native_sha256: String,
  before_model_sha256: String,
  after_model_sha256: String,
  entity_order: Vec<u32>,
  changes: Vec<YmapStructDiffEnum>,
}

fn model_hash(model: &Ymap) -> Result<String> {
  Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(model)?)))
}

impl VanillaYmapDelta {
  /// Captures all modeled changes, including removals and replacement metadata.
  pub(crate) fn extract_from(
    before: &Ymap,
    after: &Ymap,
    base: CachedFile,
    target_native_sha256: String,
  ) -> Result<Self> {
    Ok(Self {
      format: DeltaFormat::VanillaYmapV1,
      base,
      target_native_sha256,
      before_model_sha256: model_hash(before)?,
      after_model_sha256: model_hash(after)?,
      entity_order: after.entity_map.keys().copied().collect(),
      changes: before.diff(after),
    })
  }

  /// Verify both states and restore entity order explicitly: map equality alone
  /// does not preserve order-only changes needed by parent references.
  pub(crate) fn apply_to(
    self,
    before: &Ymap,
  ) -> Result<Ymap> {
    if model_hash(before)? != self.before_model_sha256 {
      return Err("Vanilla delta predecessor model hash mismatch".into());
    }
    let mut result = before.clone().apply(self.changes);
    let mut entities = std::mem::take(&mut result.entity_map);
    result.entity_map = self
      .entity_order
      .into_iter()
      .map(|guid| {
        entities.shift_remove(&guid).map(|entity| (guid, entity)).ok_or_else(|| {
          format!("Vanilla delta entity order references missing GUID {guid}").into()
        })
      })
      .collect::<Result<_>>()?;
    if !entities.is_empty() {
      return Err("Vanilla delta entity order omits GUIDs".into());
    }
    if model_hash(&result)? != self.after_model_sha256 {
      return Err("Vanilla delta reconstructed model hash mismatch".into());
    }
    Ok(result)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::ymap::xml::XmlYmap;

  fn original() -> Ymap {
    let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
    )))
    .unwrap();
    xml.into()
  }

  fn predecessor() -> CachedFile {
    CachedFile {
      sha256: "1".repeat(64),
      object: "0000-base/ymap/map.ymap".into(),
      native: None,
      source: "fixture.rpf/map.ymap".into(),
    }
  }

  #[test]
  fn vanilla_delta_round_trip_preserves_deletions_flags_and_metadata() {
    let mut before = original();
    before.content_flags |= 1 << 30;
    let mut expected = before.clone();
    expected.entity_map.shift_remove_index(0).unwrap();
    expected.content_flags &= !(1 << 30);
    expected.flags ^= 1;
    expected.name = "renamed_map".into();
    expected.parent = "new_parent".into();
    expected.streaming_extents_min.x += 5.0;
    expected.physics_dictionaries.push("added_dictionary".into());
    let delta =
      VanillaYmapDelta::extract_from(&before, &expected, predecessor(), "2".repeat(64)).unwrap();
    let json = serde_json::to_vec(&delta).unwrap();
    let loaded: VanillaYmapDelta = serde_json::from_slice(&json).unwrap();
    let actual = loaded.apply_to(&before).unwrap();
    assert_eq!(actual, expected);
    assert!(actual.diff(&expected).is_empty());
  }

  #[test]
  fn vanilla_delta_rejects_wrong_predecessor_and_corrupted_result() {
    let before = original();
    let mut expected = before.clone();
    expected.parent = "new_parent".into();
    let delta =
      VanillaYmapDelta::extract_from(&before, &expected, predecessor(), "2".repeat(64)).unwrap();
    let mut wrong = before.clone();
    wrong.flags ^= 1;
    assert!(delta.apply_to(&wrong).is_err());
    let mut delta =
      VanillaYmapDelta::extract_from(&before, &expected, predecessor(), "2".repeat(64)).unwrap();
    delta.after_model_sha256 = "0".repeat(64);
    assert!(delta.apply_to(&before).is_err());
  }

  #[test]
  fn vanilla_delta_preserves_entity_reordering_without_field_changes() {
    let before = original();
    let mut expected = before.clone();
    expected.entity_map.reverse();
    let delta =
      VanillaYmapDelta::extract_from(&before, &expected, predecessor(), "2".repeat(64)).unwrap();
    let bytes = serde_json::to_vec(&delta).unwrap();
    let actual =
      serde_json::from_slice::<VanillaYmapDelta>(&bytes).unwrap().apply_to(&before).unwrap();
    assert_eq!(
      actual.entity_map.keys().collect::<Vec<_>>(),
      expected.entity_map.keys().collect::<Vec<_>>()
    );
  }

  #[test]
  fn applied_delta_preserves_repeated_xml_and_binary_conversion() {
    use crate::core::format::gamefile::{
      resource_convert::{NativeResourceFormat, resource_to_xml, xml_to_resource},
      test_support::{sample_ymap_catalog, sample_ymap_xml},
    };

    fn stable_conversion(
      xml: &str,
      catalog: &crate::core::format::gamefile::meta_resource::MetaSchemaCatalog,
    ) -> (Vec<u8>, String) {
      let first_binary = xml_to_resource(NativeResourceFormat::Ymap, xml, catalog).unwrap();
      let first_xml =
        resource_to_xml(NativeResourceFormat::Ymap, &first_binary, &catalog.hash_names).unwrap();
      let second_binary = xml_to_resource(NativeResourceFormat::Ymap, &first_xml, catalog).unwrap();
      let second_xml =
        resource_to_xml(NativeResourceFormat::Ymap, &second_binary, &catalog.hash_names).unwrap();
      assert_eq!(first_binary, second_binary);
      assert_eq!(first_xml, second_xml);
      (second_binary, second_xml)
    }

    let source_xml = sample_ymap_xml("parent_refs/vanilla_parent.ymap.xml");
    let catalog = sample_ymap_catalog();
    let xml: XmlYmap = quick_xml::de::from_str(&source_xml).unwrap();
    let before: Ymap = xml.into();
    let mut expected = before.clone();
    expected.flags ^= 1;
    let expected_xml = quick_xml::se::to_string(&XmlYmap::from(expected.clone())).unwrap();
    let (expected_binary, expected_xml) = stable_conversion(&expected_xml, catalog);

    let delta =
      VanillaYmapDelta::extract_from(&before, &expected, predecessor(), "2".repeat(64)).unwrap();
    let json = serde_json::to_vec(&delta).unwrap();
    let loaded: VanillaYmapDelta = serde_json::from_slice(&json).unwrap();
    let actual = loaded.apply_to(&before).unwrap();
    let actual_xml = quick_xml::se::to_string(&XmlYmap::from(actual)).unwrap();
    let (actual_binary, actual_xml) = stable_conversion(&actual_xml, catalog);

    assert_eq!(actual_xml, expected_xml);
    assert_eq!(actual_binary, expected_binary);
  }
}
