//! Structured YBN changes for vanilla history.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{CachedFile, Result};
use crate::core::format::ybn::{
  diff::YbnDiff,
  model::Bound,
  read_ybn, write_ybn,
  xml::{xml_to_ybn, ybn_to_xml},
};

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StructuredYbnChanges {
  Semantic {
    diff: YbnDiff,
  },
  ModelReplacement {
    reason: String,
    model: Box<Bound>,
  },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VanillaYbnDelta {
  pub(crate) base: CachedFile,
  pub(crate) target_native_sha256: String,
  before_model_sha256: String,
  canonical_xml_sha256: String,
  canonical_binary_sha256: String,
  pub(crate) changes: StructuredYbnChanges,
}

fn hash(bytes: &[u8]) -> String {
  format!("{:x}", Sha256::digest(bytes))
}

fn model_hash(model: &Bound) -> Result<String> {
  Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(model)?)))
}

fn stable_model(source_binary: &[u8]) -> Result<(Bound, String, Vec<u8>)> {
  let source_xml = ybn_to_xml(source_binary)?;
  let first_binary = xml_to_ybn(&source_xml)?;
  let first_xml = ybn_to_xml(&first_binary)?;
  let second_binary = xml_to_ybn(&first_xml)?;
  let second_xml = ybn_to_xml(&second_binary)?;
  if first_binary != second_binary || first_xml != second_xml {
    let xml_difference = first_xml
      .lines()
      .zip(second_xml.lines())
      .find(|(first, second)| first != second)
      .map(|(first, second)| format!("{first} != {second}"));
    let binary_offset =
      first_binary.iter().zip(&second_binary).position(|(first, second)| first != second);
    return Err(format!(
      "YBN XML/binary conversion is not stable (binary {} vs {}, first byte {binary_offset:?}; XML {xml_difference:?})",
      first_binary.len(),
      second_binary.len()
    ).into());
  }
  Ok((read_ybn(&second_binary)?, second_xml, second_binary))
}

impl VanillaYbnDelta {
  /// Stores semantic changes, falling back to the complete decoded model when needed.
  pub(crate) fn extract_from(
    before: &[u8],
    after: &[u8],
    base: CachedFile,
  ) -> Result<Self> {
    if hash(before) != base.sha256 {
      return Err("YBN delta predecessor binary hash mismatch".into());
    }
    let before_model = read_ybn(before)?;
    let (after_model, canonical_xml, canonical_binary) = stable_model(after)?;
    let changes = match YbnDiff::extract_from(&before_model, &after_model) {
      Ok(diff) => match diff.apply_to(&before_model) {
        Ok(applied) => {
          let applied_binary = write_ybn(&applied)?;
          match stable_model(&applied_binary) {
            Ok((_, applied_xml, applied_binary))
              if applied_xml == canonical_xml && applied_binary == canonical_binary =>
            {
              StructuredYbnChanges::Semantic {
                diff,
              }
            }
            Ok(_) => StructuredYbnChanges::ModelReplacement {
              reason: "semantic changes do not reproduce canonical XML and binary".into(),
              model: Box::new(after_model.clone()),
            },
            Err(error) => StructuredYbnChanges::ModelReplacement {
              reason: format!("semantic reconstruction is not conversion-stable: {error}"),
              model: Box::new(after_model.clone()),
            },
          }
        }
        Err(error) => StructuredYbnChanges::ModelReplacement {
          reason: error.to_string(),
          model: Box::new(after_model.clone()),
        },
      },
      Err(error) => StructuredYbnChanges::ModelReplacement {
        reason: error.to_string(),
        model: Box::new(after_model.clone()),
      },
    };
    Ok(Self {
      base,
      target_native_sha256: hash(after),
      before_model_sha256: model_hash(&before_model)?,
      canonical_xml_sha256: hash(canonical_xml.as_bytes()),
      canonical_binary_sha256: hash(&canonical_binary),
      changes,
    })
  }

  /// Applies a structured YBN delta and verifies the canonical conversion result.
  pub(crate) fn apply_to(
    &self,
    before: &Bound,
  ) -> Result<Bound> {
    if model_hash(before)? != self.before_model_sha256 {
      return Err("YBN structured delta predecessor model hash mismatch".into());
    }
    let result = match &self.changes {
      StructuredYbnChanges::Semantic {
        diff,
      } => diff.apply_to(before)?,
      StructuredYbnChanges::ModelReplacement {
        model,
        ..
      } => model.as_ref().clone(),
    };
    let result_binary = write_ybn(&result)?;
    let (result, xml, binary) = stable_model(&result_binary)?;
    let xml_matches = hash(xml.as_bytes()) == self.canonical_xml_sha256;
    let binary_matches = hash(&binary) == self.canonical_binary_sha256;
    if !xml_matches || !binary_matches {
      return Err(format!(
        "YBN structured delta reconstructed XML/binary mismatch (XML: {xml_matches}, binary: {binary_matches})"
      ).into());
    }
    Ok(result)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::ybn::xml::xml_to_ybn;

  fn base(bytes: &[u8]) -> CachedFile {
    CachedFile {
      sha256: hash(bytes),
      object: "0000-base/ybn/collision.ybn".into(),
      native: None,
      source: "fixture.rpf/collision.ybn".into(),
    }
  }

  fn geometry_fixture() -> &'static str {
    include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
    ))
  }

  #[test]
  fn serializes_semantic_changes_without_binary_patch_fields() {
    let before_xml = geometry_fixture();
    let after_xml = before_xml.replacen("<Margin value=\"0\" />", "<Margin value=\"0.1\" />", 1);
    let before = xml_to_ybn(before_xml).unwrap();
    let after = xml_to_ybn(&after_xml).unwrap();
    let delta = VanillaYbnDelta::extract_from(&before, &after, base(&before)).unwrap();
    let json = serde_json::to_value(&delta).unwrap();

    assert_eq!(json["changes"]["kind"], "semantic");
    assert!(!json["changes"]["diff"]["bound_diffs"].as_array().unwrap().is_empty());
    let loaded: VanillaYbnDelta = serde_json::from_value(json.clone()).unwrap();
    assert!(matches!(loaded.changes, StructuredYbnChanges::Semantic { .. }));
    let before_model = read_ybn(&before).unwrap();
    let applied = loaded.apply_to(&before_model).unwrap();
    let (expected, expected_xml, expected_binary) = stable_model(&after).unwrap();
    let actual_binary_seed = write_ybn(&applied).unwrap();
    let (actual, actual_xml, actual_binary) = stable_model(&actual_binary_seed).unwrap();
    assert_eq!(actual_xml, expected_xml);
    assert_eq!(actual_binary, expected_binary);
    assert_eq!(model_hash(&actual).unwrap(), model_hash(&expected).unwrap());
    for field in
      ["format", "before_size", "after_size", "prefix_length", "suffix_length", "replacement"]
    {
      assert!(json.get(field).is_none(), "unexpected field {field}");
    }
  }

  #[test]
  fn compares_out_of_range_material_colours_as_structured_changes() {
    let before_xml = geometry_fixture();
    let after_xml = before_xml
      .replace(
        "<MaterialColourIndex value=\"0\" />\n      <Unk value=\"0\" />",
        "<MaterialColourIndex value=\"9\" />\n      <Unk value=\"0\" />",
      )
      .replace(
        "    <Polygons>",
        "    <MaterialColours>1, 2, 3, 4</MaterialColours>\n    <Polygons>",
      );
    assert_ne!(before_xml, after_xml);
    let before = xml_to_ybn(before_xml).unwrap();
    let after = xml_to_ybn(&after_xml).unwrap();
    let delta = VanillaYbnDelta::extract_from(&before, &after, base(&before)).unwrap();
    let json = serde_json::to_value(delta).unwrap();

    assert_eq!(json["changes"]["kind"], "model_replacement");
    let loaded: VanillaYbnDelta = serde_json::from_value(json).unwrap();
    assert!(matches!(loaded.changes, StructuredYbnChanges::ModelReplacement { .. }));
    let applied = loaded.apply_to(&read_ybn(&before).unwrap()).unwrap();
    let (expected, expected_xml, expected_binary) = stable_model(&after).unwrap();
    let actual_binary_seed = write_ybn(&applied).unwrap();
    let (actual, actual_xml, actual_binary) = stable_model(&actual_binary_seed).unwrap();
    assert_eq!(actual_xml, expected_xml);
    assert_eq!(actual_binary, expected_binary);
    assert_eq!(model_hash(&actual).unwrap(), model_hash(&expected).unwrap());
  }

  #[test]
  fn stores_the_decoded_model_when_polygon_references_cannot_be_resolved() {
    use crate::core::format::ybn::model::Polygon;

    let before_xml = geometry_fixture();
    let after_xml = before_xml.replace("type=\"GeometryBVH\"", "type=\"Geometry\"").replacen(
      "<Triangle m=\"0\" v1=\"0\" v2=\"1\" v3=\"2\" f1=\"0\" f2=\"0\" f3=\"0\" />",
      "<Sphere m=\"0\" v=\"55\" radius=\"1\" />",
      1,
    );
    assert_ne!(before_xml, after_xml);
    let before = xml_to_ybn(before_xml).unwrap();
    let after = xml_to_ybn(&after_xml).unwrap();
    let delta = VanillaYbnDelta::extract_from(&before, &after, base(&before)).unwrap();
    let StructuredYbnChanges::ModelReplacement {
      model,
      ..
    } = &delta.changes
    else {
      panic!("expected full-model fallback for invalid polygon reference");
    };
    assert!(matches!(
      model.children[0].geometry.as_ref().unwrap().polygons[0],
      Polygon::Sphere {
        vertex: 55,
        ..
      }
    ));
    let applied = delta.apply_to(&read_ybn(&before).unwrap()).unwrap();
    let (expected, expected_xml, expected_binary) = stable_model(&after).unwrap();
    let actual_binary_seed = write_ybn(&applied).unwrap();
    let (actual, actual_xml, actual_binary) = stable_model(&actual_binary_seed).unwrap();
    assert_eq!(actual_xml, expected_xml);
    assert_eq!(actual_binary, expected_binary);
    assert_eq!(model_hash(&actual).unwrap(), model_hash(&expected).unwrap());
  }

  #[test]
  fn rejects_invalid_predecessors_and_undecodable_resources() {
    let bytes = xml_to_ybn(geometry_fixture()).unwrap();
    assert!(VanillaYbnDelta::extract_from(&bytes, &bytes, base(b"wrong")).is_err());
    assert!(VanillaYbnDelta::extract_from(b"not-ybn", b"not-ybn", base(b"not-ybn")).is_err());
  }
}
