//! Binary resource envelope dispatch, independent of filename extensions.

use std::io;

use super::{pso::PsoResource, rsc7::Rsc7Resource};

/// A decoded binary resource before its file-specific contents are interpreted.
#[derive(Debug)]
pub enum Resource {
  /// Decompressed page regions; their contents may be META, Bounds or another resource graph.
  Rsc7(Rsc7Resource),
  /// A PSO container with its own sections, schemas and mappings.
  Pso(PsoResource),
}

impl Resource {
  /// Detects and decodes an extracted binary resource using its magic bytes.
  ///
  /// XML and unsupported binary representations are rejected rather than treated as RSC7.
  ///
  /// # Examples
  ///
  /// ```
  /// use mlo_merger::core::format::gamefile::{resource::Resource, rsc7::Rsc7Resource};
  /// let bytes = Rsc7Resource::from_pages(2, &[1, 2, 3], &[])?.encode()?;
  /// assert!(matches!(Resource::decode(&bytes)?, Resource::Rsc7(_)));
  /// # Ok::<(), std::io::Error>(())
  /// ```
  pub fn decode(bytes: &[u8]) -> io::Result<Self> {
    match bytes.get(..4) {
      Some(b"RSC7") => Rsc7Resource::decode(bytes).map(Self::Rsc7),
      Some(b"PSIN") => PsoResource::parse(bytes).map(Self::Pso),
      Some(_) => Err(io::Error::new(io::ErrorKind::InvalidData, "unsupported resource magic")),
      None => Err(io::Error::new(io::ErrorKind::InvalidData, "resource magic is truncated")),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rsc7_dispatch_does_not_require_meta_contents() {
    let bytes = Rsc7Resource::from_pages(43, &[1, 2, 3], &[4, 5, 6]).unwrap().encode().unwrap();
    let Resource::Rsc7(resource) = Resource::decode(&bytes).unwrap() else {
      panic!("expected RSC7");
    };
    assert_eq!(resource.version, 43);
    assert_eq!(&resource.system_data[..3], &[1, 2, 3]);
    assert_eq!(&resource.graphics_data[..3], &[4, 5, 6]);
  }

  #[test]
  fn pso_magic_dispatches_to_the_pso_parser() {
    let bytes = b"PSIN";
    assert_eq!(
      Resource::decode(bytes).unwrap_err().to_string(),
      PsoResource::parse(bytes).unwrap_err().to_string()
    );
  }

  #[test]
  fn rejects_xml_unknown_and_truncated_magic() {
    for bytes in [b"<CMapData />".as_slice(), b"RBF ", b"", b"RSC"] {
      assert_eq!(Resource::decode(bytes).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
  }
}
