use std::{collections::BTreeMap, fs, path::Path};

use super::extract::Sample;

#[test]
#[ignore = "summarizes local per-file results after running the differential tests"]
fn summarize_sample_results() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/sample");
  let mut summary = BTreeMap::new();
  for extension in ["ymap", "ybn", "ymt", "ynd", "ytyp"] {
    let samples: Vec<Sample> =
      serde_json::from_slice(&fs::read(base.join(extension).join("manifest.json")).unwrap())
        .unwrap();
    for direction in ["to_xml", "from_xml"] {
      let mut counts = BTreeMap::<String, usize>::new();
      for sample in &samples {
        let path =
          base.join("reports").join(extension).join(format!("{}.{}.json", sample.file, direction));
        let Ok(bytes) = fs::read(path) else {
          *counts.entry("not_run".into()).or_default() += 1;
          continue;
        };
        let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let status = match report["error"].as_str() {
          None => "passed",
          Some(error) if error.starts_with("isolated rebuild") => "worker_timeout_or_exit",
          Some(error) if error.contains("CodeWalker export contains an error node") => {
            "reference_schema_error"
          }
          Some(error) if error.starts_with("CodeWalker") => "reference_conversion_error",
          Some(error) if error.contains("unsupported") || error.contains("not supported") => {
            "native_unsupported"
          }
          Some(error)
            if error.starts_with("Native to-xml:") || error.starts_with("Native from-xml:") =>
          {
            "native_conversion_error"
          }
          Some(_) => "mismatch",
        };
        *counts.entry(status.into()).or_default() += 1;
        for (field, label) in [
          ("native_binary_identical_to_source", "native_byte_identical"),
          ("codewalker_binary_identical_to_source", "codewalker_byte_identical"),
          ("native_rebuild_preserves_source_xml", "native_source_xml_preserved"),
          ("codewalker_rebuild_preserves_source_xml", "codewalker_source_xml_preserved"),
        ] {
          if let Some(identical) = report[field].as_bool() {
            *counts.entry(format!("{label}_measured")).or_default() += 1;
            if identical {
              *counts.entry(label.into()).or_default() += 1;
            }
          }
        }
      }
      summary.insert(format!("{extension}/{direction}"), counts);
    }
  }
  let json = serde_json::to_string_pretty(&summary).unwrap();
  fs::create_dir_all(base.join("reports")).unwrap();
  fs::write(base.join("reports/summary.json"), &json).unwrap();
  eprintln!("{json}");
}
