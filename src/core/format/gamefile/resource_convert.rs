use std::{
  collections::HashMap,
  fs, io,
  path::{Path, PathBuf},
};

use super::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  meta_xml::{meta_to_xml, ymap_to_xml},
  resource_file::Rsc7Resource,
  xml_meta_builder::meta_from_xml,
  xml_tree::parse_xml,
  ybn::{xml_to_ybn, ybn_to_xml},
  ynd::{xml_to_ynd, ynd_to_xml},
};
use walkdir::WalkDir;

/// Supported native conversion families selected by the input extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeResourceFormat {
  Ymap,
  Ytyp,
  YmtRsc,
  Ybn,
  Ynd,
}

impl NativeResourceFormat {
  /// Selects the native format adapter from a filename extension.
  pub fn from_path(path: &Path) -> io::Result<Self> {
    match path
      .extension()
      .and_then(|extension| extension.to_str())
      .map(str::to_ascii_lowercase)
      .as_deref()
    {
      Some("ymap") => Ok(Self::Ymap),
      Some("ytyp") => Ok(Self::Ytyp),
      Some("ymt") => Ok(Self::YmtRsc),
      Some("ybn") => Ok(Self::Ybn),
      Some("ynd") => Ok(Self::Ynd),
      _ => Err(invalid_data("unsupported native resource extension")),
    }
  }

  /// Selects a format from an XML filename such as `map.ymap.xml`.
  pub fn from_xml_path(path: &Path) -> io::Result<Self> {
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or_else(|| invalid_data("XML path has no UTF-8 filename"))?;
    let source_name = name
      .strip_suffix(".xml")
      .or_else(|| name.strip_suffix(".XML"))
      .ok_or_else(|| invalid_data("input filename must end in .xml"))?;
    let source_name = source_name
      .strip_suffix(".pso")
      .or_else(|| source_name.strip_suffix(".PSO"))
      .unwrap_or(source_name);
    Self::from_path(Path::new(source_name))
  }

  /// Returns the binary RSC version used when rebuilding this resource family.
  pub fn rsc_version(self) -> io::Result<u32> {
    match self {
      Self::Ymap | Self::Ytyp | Self::YmtRsc => Ok(2),
      Self::Ybn => Ok(43),
      Self::Ynd => Ok(1),
    }
  }

  fn is_generic_meta(self) -> bool {
    matches!(self, Self::Ymap | Self::Ytyp | Self::YmtRsc)
  }
}

/// Converts supported native binary resources from a file or directory to XML.
pub fn convert_files_to_xml(
  input: &Path,
  output_dir: &Path,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
  let inputs = collect_inputs(input, &|path| NativeResourceFormat::from_path(path).is_ok())?;
  fs::create_dir_all(output_dir)?;
  let mut shared_names = HashMap::new();
  for source in &inputs {
    if !matches!(
      NativeResourceFormat::from_path(source),
      Ok(NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp | NativeResourceFormat::YmtRsc)
    ) {
      continue;
    }
    if let Ok(bytes) = fs::read(source)
      && let Ok(resource) = Rsc7Resource::decode(&bytes)
      && let Ok(meta) = MetaResource::parse(&resource)
    {
      shared_names.extend(meta.hash_names());
    }
  }

  let mut converted = 0;
  let mut failed = 0;
  for source in &inputs {
    let relative = relative_input_path(input, source)?;
    let bytes = fs::read(source)?;
    let output_name = if bytes.starts_with(b"PSIN") {
      append_suffix(&relative, ".pso.xml")
    } else {
      append_xml_suffix(&relative)
    };
    let output = output_dir.join(output_name);
    if let Some(parent) = output.parent() {
      fs::create_dir_all(parent)?;
    }
    let result = resource_to_xml(NativeResourceFormat::from_path(source)?, &bytes, &shared_names);
    match result {
      Ok(xml) => {
        fs::write(output, xml)?;
        converted += 1;
      }
      Err(error) => {
        log::error!("Failed to convert {} to XML: {error}", source.display());
        failed += 1;
      }
    }
  }
  Ok((converted, failed))
}

/// Converts supported native XML files from a file or directory into binary resources.
pub fn convert_files_from_xml(
  input: &Path,
  output_dir: &Path,
  schema_dir: &Path,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
  let inputs = collect_inputs(input, &|path| NativeResourceFormat::from_xml_path(path).is_ok())?;
  fs::create_dir_all(output_dir)?;

  let mut catalog = MetaSchemaCatalog::default();
  let schema_inputs = collect_inputs(schema_dir, &|path| {
    matches!(
      NativeResourceFormat::from_path(path),
      Ok(NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp | NativeResourceFormat::YmtRsc)
    )
  })?;
  for source in schema_inputs {
    let result = fs::read(&source)
      .and_then(|bytes| Rsc7Resource::decode(&bytes))
      .and_then(|resource| MetaResource::parse(&resource));
    if let Ok(meta) = result {
      catalog.add_resource(&meta);
    }
  }

  let mut converted = 0;
  let mut failed = 0;
  for source in &inputs {
    let relative = relative_input_path(input, source)?;
    let output_name = strip_xml_suffix(&relative)?;
    let output = output_dir.join(&output_name);
    if let Some(parent) = output.parent() {
      fs::create_dir_all(parent)?;
    }
    let format = NativeResourceFormat::from_xml_path(source)?;
    let result = fs::read_to_string(source).and_then(|xml| {
      if is_pso_xml(source) {
        let template = if schema_dir.is_file() {
          schema_dir.to_path_buf()
        } else {
          schema_dir.join(&output_name)
        };
        super::pso::PsoResource::parse(&fs::read(template)?)?.rebuild_xml(&xml)
      } else {
        xml_to_resource(format, &xml, &catalog)
      }
    });
    match result {
      Ok(bytes) => {
        fs::write(output, bytes)?;
        converted += 1;
      }
      Err(error) => {
        log::error!("Failed to convert {} to binary: {error}", source.display());
        failed += 1;
      }
    }
  }
  Ok((converted, failed))
}

fn collect_inputs(
  input: &Path,
  supported: &dyn Fn(&Path) -> bool,
) -> io::Result<Vec<PathBuf>> {
  if input.is_file() {
    return Ok(if supported(input) { vec![input.to_path_buf()] } else { Vec::new() });
  }
  if !input.is_dir() {
    return Err(io::Error::new(
      io::ErrorKind::NotFound,
      format!("input path does not exist: {}", input.display()),
    ));
  }
  let mut paths = WalkDir::new(input)
    .follow_links(false)
    .into_iter()
    .filter_map(Result::ok)
    .filter(|entry| entry.file_type().is_file())
    .map(|entry| entry.into_path())
    .filter(|path| supported(path))
    .collect::<Vec<_>>();
  paths.sort();
  Ok(paths)
}

fn relative_input_path(
  input: &Path,
  source: &Path,
) -> io::Result<PathBuf> {
  if input.is_file() {
    return source
      .file_name()
      .map(PathBuf::from)
      .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "input filename is missing"));
  }
  source
    .strip_prefix(input)
    .map(Path::to_path_buf)
    .map_err(|error| io::Error::other(error.to_string()))
}

fn append_xml_suffix(path: &Path) -> PathBuf {
  let mut name = path.as_os_str().to_os_string();
  name.push(".xml");
  PathBuf::from(name)
}

fn strip_xml_suffix(path: &Path) -> io::Result<PathBuf> {
  let name = path
    .file_name()
    .and_then(|name| name.to_str())
    .and_then(|name| name.strip_suffix(".xml").or_else(|| name.strip_suffix(".XML")))
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "XML filename must end in .xml"))?;
  let name = name.strip_suffix(".pso").or_else(|| name.strip_suffix(".PSO")).unwrap_or(name);
  Ok(path.with_file_name(name))
}

fn is_pso_xml(path: &Path) -> bool {
  path
    .file_name()
    .and_then(|name| name.to_str())
    .is_some_and(|name| name.to_ascii_lowercase().ends_with(".pso.xml"))
}

/// Converts an RSC-META resource to its generic CodeWalker META XML representation.
pub fn resource_to_xml(
  format: NativeResourceFormat,
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<String> {
  if format.is_generic_meta() && bytes.starts_with(b"PSIN") {
    return super::pso::PsoResource::parse(bytes)?.to_xml(shared_hash_names);
  }
  if format == NativeResourceFormat::Ynd {
    return ynd_to_xml(bytes);
  }
  if format == NativeResourceFormat::Ybn {
    return ybn_to_xml(bytes);
  }
  if format == NativeResourceFormat::YmtRsc && !bytes.starts_with(b"RSC7") {
    if let Ok(xml) = std::str::from_utf8(bytes)
      && xml.trim_start().starts_with('<')
      && parse_xml(xml).is_ok()
    {
      return Ok(xml.to_string());
    }
    let variant = match bytes.get(..4) {
      Some(b"PSIN") | Some(b"PSO ") => "PSO/PSIN",
      Some(b"RBF ") => "RBF",
      _ => "unknown",
    };
    return Err(invalid_data(&format!("Native YMT {variant} variant is not supported yet")));
  }
  if !format.is_generic_meta() {
    return Err(unsupported_format(format));
  }
  if format == NativeResourceFormat::Ymap {
    return ymap_to_xml(bytes, shared_hash_names);
  }
  let resource = Rsc7Resource::decode(bytes)?;
  let meta = MetaResource::parse(&resource)?;
  meta_to_xml(&meta, shared_hash_names)
}

/// Converts generic CodeWalker META XML into a compressed RSC7 resource.
pub fn xml_to_resource(
  format: NativeResourceFormat,
  xml: &str,
  catalog: &MetaSchemaCatalog,
) -> io::Result<Vec<u8>> {
  if format == NativeResourceFormat::Ynd {
    return xml_to_ynd(xml);
  }
  if format == NativeResourceFormat::Ybn {
    return xml_to_ybn(xml);
  }
  if !format.is_generic_meta() {
    return Err(unsupported_format(format));
  }
  let meta = match meta_from_xml(xml, catalog) {
    Ok(meta) => meta,
    Err(_error) if format == NativeResourceFormat::YmtRsc && parse_xml(xml).is_ok() => {
      return Ok(xml.as_bytes().to_vec());
    }
    Err(error) => return Err(error),
  };
  let resource = meta.to_rsc7(format.rsc_version()?)?;
  resource.encode()
}

/// Converts one file or recursively discovered resource files to XML.
pub fn convert_path_to_xml(
  input: &Path,
  output_dir: &Path,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
  let inputs = collect_paths(input, |path| NativeResourceFormat::from_path(path).is_ok())?;
  fs::create_dir_all(output_dir)?;
  let mut names = HashMap::new();
  for path in &inputs {
    if !matches!(
      NativeResourceFormat::from_path(path),
      Ok(NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp | NativeResourceFormat::YmtRsc)
    ) {
      continue;
    }
    if let Ok(bytes) = fs::read(path)
      && let Ok(resource) = Rsc7Resource::decode(&bytes)
      && let Ok(meta) = MetaResource::parse(&resource)
    {
      names.extend(meta.hash_names());
    }
  }

  let mut converted = 0;
  let mut failed = 0;
  for path in &inputs {
    let format = NativeResourceFormat::from_path(path)?;
    let relative = relative_path(input, path)?;
    let bytes = fs::read(path)?;
    let suffix = if bytes.starts_with(b"PSIN") { ".pso.xml" } else { ".xml" };
    let output = output_dir.join(append_suffix(&relative, suffix));
    if let Some(parent) = output.parent() {
      fs::create_dir_all(parent)?;
    }
    match resource_to_xml(format, &bytes, &names) {
      Ok(xml) => {
        fs::write(output, xml)?;
        converted += 1;
      }
      Err(error) => {
        log::error!("Failed to convert {} to XML: {error}", path.display());
        failed += 1;
      }
    }
  }
  Ok((converted, failed))
}

/// Converts one XML file or recursively discovered XML files to binary resources.
pub fn convert_path_from_xml(
  input: &Path,
  output_dir: &Path,
  schema_dir: &Path,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
  let inputs = collect_paths(input, |path| NativeResourceFormat::from_xml_path(path).is_ok())?;
  fs::create_dir_all(output_dir)?;
  let schema_paths = collect_paths(schema_dir, |path| {
    matches!(
      NativeResourceFormat::from_path(path),
      Ok(NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp | NativeResourceFormat::YmtRsc)
    )
  })?;
  let mut catalog = MetaSchemaCatalog::default();
  for path in schema_paths {
    if let Ok(bytes) = fs::read(&path)
      && let Ok(resource) = Rsc7Resource::decode(&bytes)
      && let Ok(meta) = MetaResource::parse(&resource)
    {
      catalog.add_resource(&meta);
    }
  }

  let mut converted = 0;
  let mut failed = 0;
  for path in &inputs {
    let format = NativeResourceFormat::from_xml_path(path)?;
    let relative = relative_path(input, path)?;
    let output_name = strip_xml_suffix(&relative)?;
    let output = output_dir.join(&output_name);
    if let Some(parent) = output.parent() {
      fs::create_dir_all(parent)?;
    }
    let result = fs::read_to_string(path).and_then(|xml| {
      if is_pso_xml(path) {
        let template = if schema_dir.is_file() {
          schema_dir.to_path_buf()
        } else {
          schema_dir.join(&output_name)
        };
        super::pso::PsoResource::parse(&fs::read(template)?)?.rebuild_xml(&xml)
      } else {
        xml_to_resource(format, &xml, &catalog)
      }
    });
    match result {
      Ok(bytes) => {
        fs::write(output, bytes)?;
        converted += 1;
      }
      Err(error) => {
        log::error!("Failed to convert {} to binary: {error}", path.display());
        failed += 1;
      }
    }
  }
  Ok((converted, failed))
}

fn collect_paths(
  input: &Path,
  predicate: impl Fn(&Path) -> bool,
) -> io::Result<Vec<PathBuf>> {
  if input.is_file() {
    return Ok(if predicate(input) { vec![input.to_path_buf()] } else { Vec::new() });
  }
  if !input.is_dir() {
    return Err(io::Error::new(
      io::ErrorKind::NotFound,
      format!("input path not found: {}", input.display()),
    ));
  }
  let mut paths = WalkDir::new(input)
    .follow_links(false)
    .into_iter()
    .filter_map(Result::ok)
    .filter(|entry| entry.file_type().is_file())
    .map(|entry| entry.into_path())
    .filter(|path| predicate(path))
    .collect::<Vec<_>>();
  paths.sort();
  Ok(paths)
}

fn relative_path(
  input: &Path,
  path: &Path,
) -> io::Result<PathBuf> {
  if input.is_file() {
    return path
      .file_name()
      .map(PathBuf::from)
      .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "input filename is missing"));
  }
  path
    .strip_prefix(input)
    .map(Path::to_path_buf)
    .map_err(|error| io::Error::other(error.to_string()))
}

fn append_suffix(
  path: &Path,
  suffix: &str,
) -> PathBuf {
  let mut name = path.as_os_str().to_os_string();
  name.push(suffix);
  PathBuf::from(name)
}

fn unsupported_format(format: NativeResourceFormat) -> io::Error {
  let detail = match format {
    NativeResourceFormat::Ybn => "YBN Bounds resource graph",
    NativeResourceFormat::Ynd => "YND NodeDictionary codec",
    NativeResourceFormat::YmtRsc => unreachable!(),
    NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp => unreachable!(),
  };
  invalid_data(&format!("native {detail} adapter is not implemented yet"))
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use super::{
    NativeResourceFormat, convert_files_from_xml, convert_files_to_xml, resource_to_xml,
    xml_to_resource,
  };
  use crate::core::format::gamefile::{
    meta_resource::{MetaResource, MetaSchemaCatalog},
    meta_xml::meta_to_xml,
    resource_file::Rsc7Resource,
  };

  #[test]
  fn pso_batch_commands_preserve_suffix_and_template_format() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = std::env::temp_dir().join(format!("mlo_pso_batch_{}", std::process::id()));
    let input = temp.join("input");
    let xml_dir = temp.join("xml");
    let rebuilt_dir = temp.join("rebuilt");
    std::fs::create_dir_all(input.join("nested")).unwrap();
    let source = base.join("asset/vanilla/ymap/cs1_railwyc.ymap");
    std::fs::copy(&source, input.join("nested/cs1_railwyc.ymap")).unwrap();
    assert_eq!(convert_files_to_xml(&input, &xml_dir).unwrap(), (1, 0));
    assert!(xml_dir.join("nested/cs1_railwyc.ymap.pso.xml").is_file());
    assert_eq!(
      NativeResourceFormat::from_xml_path(&xml_dir.join("nested/cs1_railwyc.ymap.pso.xml"))
        .unwrap(),
      NativeResourceFormat::Ymap
    );
    assert_eq!(convert_files_from_xml(&xml_dir, &rebuilt_dir, &input).unwrap(), (1, 0));
    let bytes = std::fs::read(rebuilt_dir.join("nested/cs1_railwyc.ymap")).unwrap();
    assert!(bytes.starts_with(b"PSIN"));
    let xml = resource_to_xml(NativeResourceFormat::Ymap, &bytes, &HashMap::new()).unwrap();
    assert_eq!(
      xml,
      std::fs::read_to_string(xml_dir.join("nested/cs1_railwyc.ymap.pso.xml")).unwrap()
    );
    std::fs::remove_dir_all(temp).unwrap();
  }

  #[test]
  fn ytyp_uses_shared_rsc_meta_conversion_path() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp");
    let bytes = std::fs::read(path).unwrap();
    let format = NativeResourceFormat::Ytyp;
    let resource = Rsc7Resource::decode(&bytes).unwrap();
    let meta = MetaResource::parse(&resource).unwrap();
    let xml = resource_to_xml(format, &bytes, &meta.hash_names()).unwrap();
    assert!(xml.contains("<CMapTypes"));

    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&meta);
    let rebuilt = xml_to_resource(format, &xml, &catalog).unwrap();
    let reparsed = MetaResource::parse(&Rsc7Resource::decode(&rebuilt).unwrap()).unwrap();
    assert_eq!(reparsed.root_block_index, meta.root_block_index);
    assert!(
      reparsed
        .structures
        .iter()
        .any(|schema| schema.name_hash == meta.data_blocks[0].structure_name_hash)
    );
    let rebuilt_xml = meta_to_xml(&reparsed, &catalog.hash_names).unwrap();
    assert!(rebuilt_xml.contains("<CMapTypes"));
  }

  #[test]
  fn ybn_dispatch_rejects_invalid_resource_envelopes() {
    let error = resource_to_xml(NativeResourceFormat::Ybn, &[], &HashMap::new()).unwrap_err();
    assert!(error.to_string().contains("RSC7 header is truncated"));
  }

  #[test]
  fn ybn_composite_geometry_bvh_fixture_round_trips_natively() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/source/wxmaps_lshospital_v/stream/dt1_01_0.ybn");
    let bytes = std::fs::read(path).unwrap();
    let xml = resource_to_xml(NativeResourceFormat::Ybn, &bytes, &HashMap::new()).unwrap();
    assert!(xml.contains("<BoundsFile>"));
    assert_eq!(xml.matches("type=\"Composite\"").count(), 1);
    assert_eq!(xml.matches("type=\"GeometryBVH\"").count(), 3);
    assert!(xml.contains("<Box m=\""));

    let rebuilt =
      xml_to_resource(NativeResourceFormat::Ybn, &xml, &MetaSchemaCatalog::default()).unwrap();
    assert_eq!(&rebuilt[..4], b"RSC7");
    let rebuilt_xml =
      resource_to_xml(NativeResourceFormat::Ybn, &rebuilt, &HashMap::new()).unwrap();
    assert_eq!(rebuilt_xml.matches("type=\"GeometryBVH\"").count(), 3);
    assert!(rebuilt_xml.contains("<Box m=\""));
  }

  #[test]
  fn ybn_remaining_primitive_bounds_round_trip_natively() {
    for kind in ["Sphere", "Capsule", "Box", "Disc", "Cylinder", "Cloth"] {
      let xml = format!(
        r#"<BoundsFile><Bounds type="{kind}">
          <BoxMin x="-2" y="-2" z="-2" /><BoxMax x="2" y="2" z="2" />
          <BoxCenter x="0" y="0" z="0" /><SphereCenter x="0" y="0" z="0" />
          <SphereRadius value="2" /><Margin value="0.1" /><Volume value="1" />
          <Inertia x="0" y="0" z="0" /><MaterialIndex value="0" />
          <MaterialColourIndex value="0" /><ProceduralID value="0" />
          <RoomID value="0" /><PedDensity value="0" /><UnkFlags value="0" />
          <PolyFlags value="0" /><UnkType value="1" />
        </Bounds></BoundsFile>"#
      );
      let binary =
        xml_to_resource(NativeResourceFormat::Ybn, &xml, &MetaSchemaCatalog::default()).unwrap();
      let output = resource_to_xml(NativeResourceFormat::Ybn, &binary, &HashMap::new()).unwrap();
      assert!(output.contains(&format!("<Bounds type=\"{kind}\">")), "missing {kind}");
      assert!(output.contains("<SphereRadius value=\"2\""), "lost common fields for {kind}");
    }
  }

  #[test]
  fn ybn_composite_none_child_round_trips_natively() {
    let xml = r#"<BoundsFile><Bounds type="Composite">
      <BoxMin x="0" y="0" z="0" /><BoxMax x="0" y="0" z="0" />
      <BoxCenter x="0" y="0" z="0" /><SphereCenter x="0" y="0" z="0" />
      <SphereRadius value="0" /><Margin value="0" /><Volume value="0" />
      <Inertia x="0" y="0" z="0" /><MaterialIndex value="0" />
      <MaterialColourIndex value="0" /><ProceduralID value="0" />
      <RoomID value="0" /><PedDensity value="0" /><UnkFlags value="0" />
      <PolyFlags value="0" /><UnkType value="1" />
      <Children><Item type="None" /></Children>
    </Bounds></BoundsFile>"#;
    let binary =
      xml_to_resource(NativeResourceFormat::Ybn, xml, &MetaSchemaCatalog::default()).unwrap();
    let output = resource_to_xml(NativeResourceFormat::Ybn, &binary, &HashMap::new()).unwrap();
    assert!(output.contains("<Item type=\"None\" />"));
  }

  #[test]
  fn ybn_remaining_geometry_polygons_round_trip_natively() {
    let xml = r#"<BoundsFile><Bounds type="GeometryBVH">
      <BoxMin x="-2" y="-2" z="-2" /><BoxMax x="2" y="2" z="2" />
      <BoxCenter x="0" y="0" z="0" /><SphereCenter x="0" y="0" z="0" />
      <SphereRadius value="4" /><Margin value="0" /><Volume value="1" />
      <Inertia x="0" y="0" z="0" /><MaterialIndex value="0" />
      <MaterialColourIndex value="0" /><ProceduralID value="0" />
      <RoomID value="0" /><PedDensity value="0" /><UnkFlags value="0" />
      <PolyFlags value="0" /><UnkType value="1" />
      <GeometryCenter x="0" y="0" z="0" /><UnkFloat1 value="0" /><UnkFloat2 value="0" />
      <Materials><Item><Type value="1" /><ProceduralID value="0" /><RoomID value="0" />
        <PedDensity value="0" /><Flags>NONE</Flags><MaterialColourIndex value="0" /><Unk value="0" />
      </Item></Materials>
      <Vertices>0, 0, 0
        1, 0, 0
        0, 1, 0
        0, 0, 1</Vertices>
      <Polygons>
        <Sphere m="0" v="0" radius="0.5" />
        <Capsule m="0" v1="0" v2="1" radius="0.25" />
        <Cylinder m="0" v1="2" v2="3" radius="0.75" />
      </Polygons>
    </Bounds></BoundsFile>"#;
    let binary =
      xml_to_resource(NativeResourceFormat::Ybn, xml, &MetaSchemaCatalog::default()).unwrap();
    let output = resource_to_xml(NativeResourceFormat::Ybn, &binary, &HashMap::new()).unwrap();
    assert!(output.contains("<Sphere m=\"0\" v=\"0\" radius=\"0.5\""));
    assert!(output.contains("<Capsule m=\"0\" v1=\"0\" v2=\"1\" radius=\"0.25\""));
    assert!(output.contains("<Cylinder m=\"0\" v1=\"2\" v2=\"3\" radius=\"0.75\""));
  }

  #[test]
  fn ynd_xml_round_trips_through_native_resource_codec() {
    let xml = r#"<NodeDictionary>
      <VehicleNodeCount value="1" />
      <PedNodeCount value="0" />
      <Nodes><Item>
        <AreaID value="7" /><NodeID value="0" /><StreetName>hash_1234ABCD</StreetName>
        <Position x="12.5" y="-8" z="3.25" />
        <Flags0 value="1" /><Flags1 value="2" /><Flags2 value="3" />
        <Flags3 value="4" /><Flags4 value="5" /><Flags5 value="1" />
        <Links><Item><ToAreaID value="8" /><ToNodeID value="9" />
          <Flags0 value="10" /><Flags1 value="11" /><Flags2 value="12" /><LinkLength value="13" />
        </Item></Links>
      </Item></Nodes>
      <Junctions><Item>
        <Position x="1.25" y="2.5" /><MinZ value="-1" /><MaxZ value="4" />
        <SizeX value="1" /><SizeY value="1" /><Heightmap>0x7F</Heightmap>
      </Item></Junctions>
      <JunctionRefs><Item>
        <AreaID value="7" /><NodeID value="0" /><JunctionID value="0" /><Unk0 value="0" />
      </Item></JunctionRefs>
    </NodeDictionary>"#;
    let binary =
      xml_to_resource(NativeResourceFormat::Ynd, xml, &MetaSchemaCatalog::default()).unwrap();
    assert_eq!(&binary[..4], b"RSC7");
    let output = resource_to_xml(NativeResourceFormat::Ynd, &binary, &HashMap::new()).unwrap();
    assert!(output.contains("<VehicleNodeCount value=\"1\""));
    assert!(output.contains("<StreetName>hash_1234ABCD</StreetName>"));
    assert!(output.contains("<LinkLength value=\"13\""));
    assert!(output.contains("<Heightmap>7F</Heightmap>"));
    assert!(output.contains("<JunctionID value=\"0\""));
  }

  #[test]
  fn ymt_dispatch_preserves_text_xml_and_rejects_malformed_pso() {
    let xml = "<ScenarioManifest><Item /></ScenarioManifest>";
    assert_eq!(
      resource_to_xml(NativeResourceFormat::YmtRsc, xml.as_bytes(), &HashMap::new()).unwrap(),
      xml
    );
    assert_eq!(
      xml_to_resource(NativeResourceFormat::YmtRsc, xml, &MetaSchemaCatalog::default()).unwrap(),
      xml.as_bytes()
    );
    let error =
      resource_to_xml(NativeResourceFormat::YmtRsc, b"PSIN\0\0\0\0", &HashMap::new()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("PSO section length"));
  }

  #[test]
  fn ymt_scenario_flags_use_codewalker_enum_names() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/source/[nteam]/cfx-nteam-acrt/SCENARIO/elysian_island.ymt");
    let bytes = std::fs::read(path).unwrap();
    let resource = Rsc7Resource::decode(&bytes).unwrap();
    let meta = MetaResource::parse(&resource).unwrap();
    let xml = resource_to_xml(NativeResourceFormat::YmtRsc, &bytes, &meta.hash_names()).unwrap();

    assert!(xml.contains("<Flags>NoSpawn, FlyOffToOblivion, ExtendedRange</Flags>"));
    assert!(!xml.contains("hash_8A5F2D90"));
  }

  #[test]
  fn batch_commands_convert_ytyp_xml_and_binary() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp");
    let temp_dir = std::env::temp_dir().join(format!("native_ytyp_batch_{}", std::process::id()));
    let binary_input_dir = temp_dir.join("binary-input");
    let xml_output_dir = temp_dir.join("xml-output");
    let binary_output_dir = temp_dir.join("binary-output");
    std::fs::create_dir_all(&binary_input_dir).unwrap();
    std::fs::copy(&source, binary_input_dir.join(source.file_name().unwrap())).unwrap();

    assert_eq!(convert_files_to_xml(&binary_input_dir, &xml_output_dir).unwrap(), (1, 0));
    let xml_path = xml_output_dir.join("sb_train_addonprops.ytyp.xml");
    assert!(std::fs::read_to_string(&xml_path).unwrap().contains("<CMapTypes"));

    assert_eq!(
      convert_files_from_xml(&xml_output_dir, &binary_output_dir, &binary_input_dir).unwrap(),
      (1, 0)
    );
    let output_path = binary_output_dir.join("sb_train_addonprops.ytyp");
    let output = std::fs::read(output_path).unwrap();
    assert_eq!(&output[..4], b"RSC7");

    std::fs::remove_dir_all(temp_dir).unwrap();
  }
}
