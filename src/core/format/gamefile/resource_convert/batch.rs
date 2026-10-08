use super::*;

/// Converts supported native binary resources from a file or directory to XML.
pub fn convert_files_to_xml(
  input: &Path,
  output_dir: &Path,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
  let inputs = collect_inputs(input, &|path| NativeResourceFormat::from_path(path).is_ok())?;
  fs::create_dir_all(output_dir)?;
  let mut shared_names = load_vanilla_hash_names();
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
  schema_dir: Option<&Path>,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
  let inputs = collect_inputs(input, &|path| NativeResourceFormat::from_xml_path(path).is_ok())?;
  if schema_dir.is_none() && inputs.iter().any(|path| is_pso_xml(path)) {
    return Err(
      io::Error::new(
        io::ErrorKind::InvalidInput,
        "--vanilla is required when converting .pso.xml files",
      )
      .into(),
    );
  }
  fs::create_dir_all(output_dir)?;
  if input.is_dir() {
    prune_managed_ymap_outputs(input, output_dir)?;
  }

  let mut catalog = MetaSchemaCatalog::default();
  let schema_inputs = if let Some(schema_dir) = schema_dir {
    collect_inputs(schema_dir, &|path| {
      matches!(
        NativeResourceFormat::from_path(path),
        Ok(NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp | NativeResourceFormat::YmtRsc)
      )
    })?
  } else {
    discover_schema_inputs(input, &inputs)?
  };
  if schema_dir.is_none()
    && schema_inputs.is_empty()
    && inputs.iter().any(|path| {
      NativeResourceFormat::from_xml_path(path)
        .is_ok_and(|format| format.is_generic_meta() && !is_pso_xml(path))
    })
  {
    return Err(
      io::Error::new(
        io::ErrorKind::NotFound,
        "no matching binary META schemas found; provide --vanilla",
      )
      .into(),
    );
  }
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
        let schema_dir = schema_dir.expect("PSO schema directory was validated");
        let template = if schema_dir.is_file() {
          schema_dir.to_path_buf()
        } else {
          schema_dir.join(&output_name)
        };
        crate::core::format::gamefile::pso::PsoResource::parse(&fs::read(template)?)?
          .rebuild_xml(&xml)
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

/// Removes stale managed YMAP binaries when their XML has been demoted to a clone or vanilla.
pub(crate) fn prune_managed_ymap_outputs(
  input_dir: &Path,
  output_dir: &Path,
) -> io::Result<()> {
  let manifest = input_dir.join("_managed_ymaps.txt");
  if !manifest.is_file() {
    return Ok(());
  }
  let content = fs::read_to_string(manifest)?;
  let names = content.lines().filter(|name| !name.is_empty()).collect::<Vec<_>>();
  for name in &names {
    let path = Path::new(name);
    let mut components = path.components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
      || components.next().is_some()
      || path.extension().is_none_or(|extension| extension != "ymap")
    {
      return Err(invalid_data("invalid generated YMAP manifest entry"));
    }
  }
  for name in names {
    let xml = input_dir.join(name).with_extension("ymap.xml");
    let binary = output_dir.join(name);
    if !xml.is_file() && binary.is_file() {
      fs::remove_file(binary)?;
    }
  }
  Ok(())
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

pub(super) fn discover_schema_inputs(
  input: &Path,
  sources: &[PathBuf],
) -> io::Result<Vec<PathBuf>> {
  let mut schemas = BTreeSet::new();
  for source in sources {
    let format = NativeResourceFormat::from_xml_path(source)?;
    let vanilla_family = match format {
      NativeResourceFormat::Ymap => "ymap",
      NativeResourceFormat::Ytyp => "ytyp",
      NativeResourceFormat::YmtRsc => "ymt",
      _ => continue,
    };
    let relative = relative_input_path(input, source)?;
    let binary_name = strip_xml_suffix(&relative)?;
    let Some(file_name) = binary_name.file_name() else {
      continue;
    };
    let mut candidates = vec![source.with_file_name(file_name)];
    for ancestor in input.ancestors() {
      candidates.push(ancestor.join("extracted").join(file_name));
      candidates.push(ancestor.join("vanilla").join(vanilla_family).join(file_name));
    }
    if let Some(schema) = candidates.into_iter().find(|candidate| candidate.is_file()) {
      schemas.insert(schema);
    }
  }
  Ok(schemas.into_iter().collect())
}

/// Converts an RSC-META resource to its generic CodeWalker META XML representation.
fn append_suffix(
  path: &Path,
  suffix: &str,
) -> PathBuf {
  let mut name = path.as_os_str().to_os_string();
  name.push(suffix);
  PathBuf::from(name)
}
