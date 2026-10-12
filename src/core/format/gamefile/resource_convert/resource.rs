use super::*;
use crate::core::format::gamefile::resource::Resource;

/// Converts native resource contents to XML using the selected file-family adapter.
pub fn resource_to_xml(
  format: NativeResourceFormat,
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<String> {
  if format == NativeResourceFormat::Ynd {
    return ynd_to_xml(bytes, shared_hash_names);
  }
  if format == NativeResourceFormat::Ybn {
    return ybn_to_xml(bytes);
  }
  if format == NativeResourceFormat::YmtRsc
    && !bytes.starts_with(b"RSC7")
    && !bytes.starts_with(b"PSIN")
  {
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
  match Resource::decode(bytes)? {
    Resource::Pso(resource) => resource.to_xml(shared_hash_names),
    Resource::Rsc7(resource) => {
      let meta = MetaResource::parse(&resource)?;
      meta_to_xml(&meta, shared_hash_names)
    }
  }
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

fn unsupported_format(format: NativeResourceFormat) -> io::Error {
  let detail = match format {
    NativeResourceFormat::Ybn => "YBN Bounds resource graph",
    NativeResourceFormat::Ynd => "YND NodeDictionary codec",
    NativeResourceFormat::YmtRsc => unreachable!(),
    NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp => unreachable!(),
  };
  invalid_data(&format!("native {detail} adapter is not implemented yet"))
}
