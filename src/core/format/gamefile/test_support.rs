use std::{path::Path, sync::OnceLock};

use super::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  resource_convert::{NativeResourceFormat, xml_to_resource},
  resource_file::Rsc7Resource,
};

pub(crate) fn sample_ymap_xml(relative_path: &str) -> String {
  let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample").join(relative_path);
  std::fs::read_to_string(path).expect("sample YMAP XML is missing")
}

pub(crate) fn sample_ymap_catalog() -> &'static MetaSchemaCatalog {
  static CATALOG: OnceLock<MetaSchemaCatalog> = OnceLock::new();
  CATALOG.get_or_init(|| {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut catalog = MetaSchemaCatalog::default();
    for entry in walkdir::WalkDir::new(base.join("asset/extracted")) {
      let Ok(entry) = entry else {
        continue;
      };
      if !entry.file_type().is_file()
        || entry.path().extension().is_none_or(|extension| extension != "ymap")
      {
        continue;
      }
      let Ok(bytes) = std::fs::read(entry.path()) else {
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
  xml_to_resource(NativeResourceFormat::Ymap, xml, sample_ymap_catalog())
    .expect("sample YMAP XML could not be rebuilt")
}
