use super::*;

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

  pub(super) fn is_generic_meta(self) -> bool {
    matches!(self, Self::Ymap | Self::Ytyp | Self::YmtRsc)
  }
}
