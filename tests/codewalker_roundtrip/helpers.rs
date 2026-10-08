use super::*;

pub(super) fn sample_parent_refs_dir() -> std::path::PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs")
}

pub(super) fn local_ymap_schema_catalog(base: &Path) -> MetaSchemaCatalog {
  let mut catalog = MetaSchemaCatalog::default();
  for entry in ["asset/extracted", "asset/vanilla-cache/latest/ymap"]
    .into_iter()
    .flat_map(|directory| walkdir::WalkDir::new(base.join(directory)))
  {
    let Ok(entry) = entry else {
      continue;
    };
    if !entry.path().is_file()
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
}

pub(super) fn assert_sample_entity_refs(
  expected: &XmlYmap,
  actual: &XmlYmap,
  label: &str,
) {
  assert_eq!(actual.entities.items.len(), expected.entities.items.len(), "entity count: {label}");
  for (expected, actual) in expected.entities.items.iter().zip(&actual.entities.items) {
    assert_eq!(actual.guid.value, expected.guid.value, "entity GUID: {label}");
    assert_eq!(actual.parent_index.value, expected.parent_index.value, "parentIndex: {label}");
    assert_eq!(actual.flags.value & 8, expected.flags.value & 8, "external-parent flag: {label}");
  }
}

pub(super) fn compare_resource_with_codewalker(
  codewalker: &CodeWalker,
  format: NativeResourceFormat,
  input: &Path,
  name: &str,
  temp_dir: &Path,
) -> Result<(), String> {
  let reference_xml_path = temp_dir.join(format!("{name}.reference.xml"));
  codewalker.game_file_to_xml(input, &reference_xml_path).map_err(|error| error.to_string())?;
  let expected_xml =
    std::fs::read_to_string(&reference_xml_path).map_err(|error| error.to_string())?;
  let bytes = std::fs::read(input).map_err(|error| error.to_string())?;
  let mut catalog = MetaSchemaCatalog::default();
  if matches!(format, NativeResourceFormat::YmtRsc | NativeResourceFormat::Ytyp) {
    let resource = Rsc7Resource::decode(&bytes).map_err(|error| error.to_string())?;
    catalog.add_resource(&MetaResource::parse(&resource).map_err(|error| error.to_string())?);
  }
  let native_xml =
    resource_to_xml(format, &bytes, &catalog.hash_names).map_err(|error| error.to_string())?;
  assert_canonical_xml_eq(
    &native_xml,
    &expected_xml,
    &format!("Native export {}", input.display()),
  )?;

  let mut rebuilt_reference_xml = expected_xml.clone();
  let codewalker_binary_path =
    (format == NativeResourceFormat::Ybn).then(|| temp_dir.join(format!("{name}.codewalker.ybn")));
  if let Some(codewalker_binary_path) = &codewalker_binary_path {
    codewalker
      .game_file_from_xml(&reference_xml_path, codewalker_binary_path)
      .map_err(|error| error.to_string())?;
    let codewalker_xml_path = temp_dir.join(format!("{name}.codewalker.xml"));
    codewalker
      .game_file_to_xml(codewalker_binary_path, &codewalker_xml_path)
      .map_err(|error| error.to_string())?;
    rebuilt_reference_xml =
      std::fs::read_to_string(codewalker_xml_path).map_err(|error| error.to_string())?;
  }

  let native_binary =
    xml_to_resource(format, &native_xml, &catalog).map_err(|error| error.to_string())?;
  if let Some(codewalker_binary_path) = &codewalker_binary_path {
    let codewalker_binary =
      std::fs::read(codewalker_binary_path).map_err(|error| error.to_string())?;
    let native_quantums = ybn_resource_quantums(&native_binary)?;
    let codewalker_quantums = ybn_resource_quantums(&codewalker_binary)?;
    let rebuilt_path = temp_dir.join(name);
    std::fs::write(&rebuilt_path, native_binary).map_err(|error| error.to_string())?;
    let rebuilt_xml_path = temp_dir.join(format!("{name}.rebuilt.xml"));
    codewalker
      .game_file_to_xml(&rebuilt_path, &rebuilt_xml_path)
      .map_err(|error| error.to_string())?;
    let rebuilt_xml =
      std::fs::read_to_string(rebuilt_xml_path).map_err(|error| error.to_string())?;
    return assert_ybn_xml_eq(
      &rebuilt_xml,
      &rebuilt_reference_xml,
      &format!("Native rebuild {}", input.display()),
      true,
      Some((&native_quantums, &codewalker_quantums)),
    );
  }
  let rebuilt_path = temp_dir.join(name);
  std::fs::write(&rebuilt_path, native_binary).map_err(|error| error.to_string())?;
  let rebuilt_xml_path = temp_dir.join(format!("{name}.rebuilt.xml"));
  codewalker
    .game_file_to_xml(&rebuilt_path, &rebuilt_xml_path)
    .map_err(|error| error.to_string())?;
  let rebuilt_xml = std::fs::read_to_string(rebuilt_xml_path).map_err(|error| error.to_string())?;
  assert_canonical_xml_eq(
    &rebuilt_xml,
    &expected_xml,
    &format!("Native rebuild {}", input.display()),
  )?;
  Ok(())
}
