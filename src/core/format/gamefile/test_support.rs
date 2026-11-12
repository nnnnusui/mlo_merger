use std::{path::Path, sync::OnceLock};

use super::{
  meta::{MetaResource, MetaSchemaCatalog},
  rsc7::Rsc7Resource,
};
use crate::core::format::stream_file::{FileType, StreamFile};

pub(crate) fn sample_ymap_xml(relative_path: &str) -> String {
  let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample").join(relative_path);
  crate::platform::fs::read_to_string(path).expect("sample YMAP XML is missing")
}

/// The model of a native sample YMAP under `docs/sample`, its names resolved with
/// `docs/sample/hash_names.json`.
pub(crate) fn sample_ymap(relative_path: &str) -> crate::core::format::ymap::model::Ymap {
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample");
  let names = crate::core::conversion::load_hash_names_or_empty(&base.join("hash_names.json"));
  let bytes = crate::platform::fs::read(base.join(relative_path)).expect("sample YMAP is missing");
  crate::core::format::ymap::binary::ymap_to_model(&bytes, &names).expect("sample YMAP is invalid")
}

pub(crate) fn sample_ymap_catalog() -> &'static MetaSchemaCatalog {
  static CATALOG: OnceLock<MetaSchemaCatalog> = OnceLock::new();
  CATALOG.get_or_init(|| {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut catalog = MetaSchemaCatalog::default();
    // The latest vanilla YMAPs are in the vanilla archive the local vanilla cache names.
    let latest = crate::core::vanilla_cache::latest_files(&base.join("asset/vanilla-cache"))
      .unwrap_or_default()
      .into_iter()
      .map(|file| file.path);
    let paths = ["docs/sample", "asset/extracted"]
      .into_iter()
      .flat_map(|directory| crate::platform::fs::WalkDir::new(base.join(directory)))
      .filter_map(|entry| Some(entry.ok()?.into_path()))
      .chain(latest);
    for path in paths {
      if !path.is_file() || path.extension().is_none_or(|extension| extension != "ymap") {
        continue;
      }
      let Ok(bytes) = crate::platform::fs::read(&path) else {
        continue;
      };
      let Ok(resource) = Rsc7Resource::decode(&bytes) else {
        continue;
      };
      if let Ok(meta) = MetaResource::parse(&resource) {
        catalog.add_resource(&meta);
      }
    }
    catalog
  })
}

pub(crate) fn sample_ymap_binary(xml: &str) -> Vec<u8> {
  StreamFile::encode_xml(FileType::Ymap, xml, sample_ymap_catalog())
    .expect("sample YMAP XML could not be rebuilt")
}
