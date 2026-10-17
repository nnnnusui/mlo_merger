use super::*;

pub(super) fn sample_parent_refs_dir() -> std::path::PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs")
}

/// Reads a YMAP binary into the model, leaving names unresolved.
pub(super) fn read_ymap(path: &Path) -> Ymap {
  ymap_to_model(&std::fs::read(path).unwrap(), &Default::default()).unwrap()
}

/// Rebuilds the YMAP binary at `input` through CodeWalker's XML and reads the result.
pub(super) fn codewalker_rebuild(
  codewalker: &CodeWalker,
  input: &Path,
  temp_dir: &Path,
  name: &str,
) -> Ymap {
  let xml = temp_dir.join(format!("{name}.dll.ymap.xml"));
  let binary = temp_dir.join(format!("{name}.dll.ymap"));
  codewalker.ymap_to_xml(input, &xml).unwrap();
  codewalker.xml_to_ymap(&xml, &binary).unwrap();
  read_ymap(&binary)
}

pub(super) fn assert_sample_entity_refs(
  expected: &Ymap,
  actual: &Ymap,
  label: &str,
) {
  assert_eq!(actual.entities.len(), expected.entities.len(), "entity count: {label}");
  for (expected, actual) in expected.entities.iter().zip(&actual.entities) {
    assert_eq!(actual.guid, expected.guid, "entity GUID: {label}");
    assert_eq!(actual.parent_index, expected.parent_index, "parentIndex: {label}");
    assert_eq!(actual.flags & 8, expected.flags & 8, "external-parent flag: {label}");
  }
}

pub(super) fn compare_resource_with_codewalker(
  codewalker: &CodeWalker,
  format: FileType,
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
  if matches!(format, FileType::Ymt | FileType::Ytyp) {
    let resource = Rsc7Resource::decode(&bytes).map_err(|error| error.to_string())?;
    catalog.add_resource(&MetaResource::parse(&resource).map_err(|error| error.to_string())?);
  }
  let native_xml = StreamFile::decode_to_xml(format, &bytes, &catalog.hash_names)
    .map_err(|error| error.to_string())?;
  assert_canonical_xml_eq(
    &native_xml,
    &expected_xml,
    &format!("Native export {}", input.display()),
  )?;

  let mut rebuilt_reference_xml = expected_xml.clone();
  let codewalker_binary_path =
    (format == FileType::Ybn).then(|| temp_dir.join(format!("{name}.codewalker.ybn")));
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
    StreamFile::encode_xml(format, &native_xml, &catalog).map_err(|error| error.to_string())?;
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
