//! CodeWalker-compatible YBN XML reading and formatted output.

use super::*;
pub(super) mod helpers;
mod read;
mod write;
pub(in crate::core::format::ybn) use read::read_bound_xml;
pub(in crate::core::format::ybn) use write::write_bound_xml;

/// Converts an RSC7 YBN Bounds resource to CodeWalker-style XML.
pub fn ybn_to_xml(bytes: &[u8]) -> io::Result<String> {
  let resource = Rsc7Resource::decode(bytes)?;
  let root = read_bound(&resource, BASE + ROOT_OFFSET as u64, None)?;
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BoundsFile>\n");
  write_bound_xml(&root, 1, "Bounds", &mut xml);
  xml.push_str("</BoundsFile>\n");
  Ok(xml)
}

/// Rebuilds a Native RSC7 YBN from CodeWalker Bounds XML.
pub fn xml_to_ybn(xml: &str) -> io::Result<Vec<u8>> {
  encode_ybn_bound(&read_xml(xml)?)
}

/// Reads CodeWalker-compatible XML into the shared indexed YBN model.
///
/// ```no_run
/// let xml = std::fs::read_to_string("collision.ybn.xml")?;
/// let bounds = mlo_merger::core::format::ybn::xml::read_xml(&xml)?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn read_xml(xml: &str) -> io::Result<Bound> {
  let root = parse_xml(xml)?;
  if root.name != "BoundsFile" {
    return Err(invalid("YBN XML root must be BoundsFile"));
  }
  let bound_node = child(&root, "Bounds")?;
  read_bound_xml(bound_node, None)
}
