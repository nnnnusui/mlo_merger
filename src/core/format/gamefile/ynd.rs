use std::io;

use super::{
  resource_file::Rsc7Resource,
  xml_tree::{XmlElement, parse_xml},
};

const BASE: u64 = 0x5000_0000;
const ROOT_SIZE: usize = 16;
const DICTIONARY_SIZE: usize = 96;
const NODE_SIZE: usize = 40;

#[derive(Debug, Clone)]
struct Node {
  area: u16,
  id: u16,
  street: u32,
  position: [i16; 3],
  flags: [u8; 5],
  link_unknown: u8,
  links: Vec<Link>,
}

#[derive(Debug, Clone)]
struct Link {
  area: u16,
  id: u16,
  flags: [u8; 4],
}

#[derive(Debug, Clone)]
struct Junction {
  max_z: i16,
  x: i16,
  y: i16,
  min_z: i16,
  width: u8,
  height: u8,
  heightmap: Vec<u8>,
}

#[derive(Debug, Clone)]
struct JunctionRef {
  area: u16,
  node: u16,
  junction: u16,
  unknown: u16,
}

#[derive(Debug, Clone)]
struct Dictionary {
  vehicle_count: u32,
  ped_count: u32,
  nodes: Vec<Node>,
  junctions: Vec<Junction>,
  refs: Vec<JunctionRef>,
}

/// Converts an RSC7 YND NodeDictionary resource into CodeWalker-style XML.
pub fn ynd_to_xml(bytes: &[u8]) -> io::Result<String> {
  let resource = Rsc7Resource::decode(bytes)?;
  let root = resource.read_address(BASE, ROOT_SIZE + DICTIONARY_SIZE)?;
  let body = &root[ROOT_SIZE..];
  let node_count = read_u32(body, 8)? as usize;
  let vehicle_count = read_u32(body, 12)?;
  let ped_count = read_u32(body, 16)?;
  let link_count = read_u32(body, 32)? as usize;
  let ref_count = read_u16(body, 72)? as usize;
  let junction_count = read_u32(body, 80)? as usize;
  let heightmap_count = read_u32(body, 84)? as usize;

  let node_bytes = read_table(&resource, read_u64(body, 0)?, node_count, NODE_SIZE)?;
  let link_bytes = read_table(&resource, read_u64(body, 24)?, link_count, 8)?;
  let junction_bytes = read_table(&resource, read_u64(body, 40)?, junction_count, 12)?;
  let heightmap_bytes = read_table(&resource, read_u64(body, 48)?, heightmap_count, 1)?;
  let ref_bytes = read_table(&resource, read_u64(body, 64)?, ref_count, 8)?;

  let links = link_bytes
    .chunks_exact(8)
    .map(|record| Link {
      area: u16_at(record, 0).unwrap(),
      id: u16_at(record, 2).unwrap(),
      flags: [record[4], record[5], record[6], record[7]],
    })
    .collect::<Vec<_>>();
  let mut nodes = Vec::with_capacity(node_count);
  for record in node_bytes.chunks_exact(NODE_SIZE) {
    let first_link = u16_at(record, 26)? as usize;
    let link_flags = record[37];
    let count = (link_flags >> 3) as usize;
    let end = first_link.checked_add(count).ok_or_else(|| invalid("YND link index overflows"))?;
    let node_links = links
      .get(first_link..end)
      .ok_or_else(|| invalid("YND node link range is out of bounds"))?
      .to_vec();
    nodes.push(Node {
      area: u16_at(record, 16)?,
      id: u16_at(record, 18)?,
      street: u32_at(record, 20)?,
      position: [i16_at(record, 28)?, i16_at(record, 30)?, i16_at(record, 34)?],
      flags: [record[32], record[33], record[36], record[38], record[39]],
      link_unknown: link_flags & 7,
      links: node_links,
    });
  }
  let mut junctions = Vec::with_capacity(junction_count);
  for record in junction_bytes.chunks_exact(12) {
    let offset = u16_at(record, 8)? as usize;
    let width = record[10];
    let height = record[11];
    let end = offset
      .checked_add(width as usize * height as usize)
      .ok_or_else(|| invalid("YND heightmap range overflows"))?;
    let heightmap = heightmap_bytes
      .get(offset..end)
      .ok_or_else(|| invalid("YND heightmap range is out of bounds"))?
      .to_vec();
    junctions.push(Junction {
      max_z: i16_at(record, 0)?,
      x: i16_at(record, 2)?,
      y: i16_at(record, 4)?,
      min_z: i16_at(record, 6)?,
      width,
      height,
      heightmap,
    });
  }
  let refs = ref_bytes
    .chunks_exact(8)
    .map(|record| JunctionRef {
      area: u16_at(record, 0).unwrap(),
      node: u16_at(record, 2).unwrap(),
      junction: u16_at(record, 4).unwrap(),
      unknown: u16_at(record, 6).unwrap(),
    })
    .collect();
  Ok(to_xml(&Dictionary {
    vehicle_count,
    ped_count,
    nodes,
    junctions,
    refs,
  }))
}

/// Builds a compressed RSC7 YND resource from CodeWalker-style XML.
pub fn xml_to_ynd(xml: &str) -> io::Result<Vec<u8>> {
  let root = parse_xml(xml)?;
  if root.name != "NodeDictionary" {
    return Err(invalid("YND XML root must be NodeDictionary"));
  }
  let dictionary = from_xml(&root)?;
  encode(&dictionary)?.encode()
}

fn from_xml(root: &XmlElement) -> io::Result<Dictionary> {
  let vehicle_count = number(root, "VehicleNodeCount")?;
  let ped_count = number(root, "PedNodeCount")?;
  let mut nodes = Vec::new();
  for item in items(root, "Nodes") {
    let position = child(item, "Position")?;
    let links = items(item, "Links")
      .map(|link| {
        Ok(Link {
          area: number(link, "ToAreaID")?,
          id: number(link, "ToNodeID")?,
          flags: [
            number(link, "Flags0")?,
            number(link, "Flags1")?,
            number(link, "Flags2")?,
            number(link, "LinkLength")?,
          ],
        })
      })
      .collect::<io::Result<Vec<_>>>()?;
    nodes.push(Node {
      area: number(item, "AreaID")?,
      id: number(item, "NodeID")?,
      street: parse_hash(&child(item, "StreetName")?.text)?,
      position: [
        (float_attr(position, "x")? * 4.0) as i16,
        (float_attr(position, "y")? * 4.0) as i16,
        (float_attr(position, "z")? * 32.0) as i16,
      ],
      flags: [
        number(item, "Flags0")?,
        number(item, "Flags1")?,
        number(item, "Flags2")?,
        number(item, "Flags3")?,
        number(item, "Flags4")?,
      ],
      link_unknown: number(item, "Flags5")?,
      links,
    });
  }
  let mut junctions = Vec::new();
  for item in items(root, "Junctions") {
    let position = child(item, "Position")?;
    let width: u8 = number(item, "SizeX")?;
    let height: u8 = number(item, "SizeY")?;
    let heightmap = parse_hex(child(item, "Heightmap")?.text.as_str())?;
    if heightmap.len() != width as usize * height as usize {
      return Err(invalid("YND heightmap length does not match dimensions"));
    }
    junctions.push(Junction {
      max_z: (float_number(item, "MaxZ")? * 32.0) as i16,
      x: (float_attr(position, "x")? * 4.0) as i16,
      y: (float_attr(position, "y")? * 4.0) as i16,
      min_z: (float_number(item, "MinZ")? * 32.0) as i16,
      width,
      height,
      heightmap,
    });
  }
  let refs = items(root, "JunctionRefs")
    .map(|item| {
      Ok(JunctionRef {
        area: number(item, "AreaID")?,
        node: number(item, "NodeID")?,
        junction: number(item, "JunctionID")?,
        unknown: number(item, "Unk0")?,
      })
    })
    .collect::<io::Result<Vec<_>>>()?;
  Ok(Dictionary {
    vehicle_count,
    ped_count,
    nodes,
    junctions,
    refs,
  })
}

fn encode(dictionary: &Dictionary) -> io::Result<Rsc7Resource> {
  let mut data = vec![0; ROOT_SIZE + DICTIONARY_SIZE];
  let mut nodes = Vec::new();
  let mut links = Vec::new();
  for node in &dictionary.nodes {
    let first_link =
      u16::try_from(links.len() / 8).map_err(|_| invalid("YND link table exceeds u16"))?;
    nodes.extend_from_slice(&[0; 16]);
    nodes.extend_from_slice(&node.area.to_le_bytes());
    nodes.extend_from_slice(&node.id.to_le_bytes());
    nodes.extend_from_slice(&node.street.to_le_bytes());
    nodes.extend_from_slice(&[0; 2]);
    nodes.extend_from_slice(&first_link.to_le_bytes());
    nodes.extend_from_slice(&node.position[0].to_le_bytes());
    nodes.extend_from_slice(&node.position[1].to_le_bytes());
    nodes.push(node.flags[0]);
    nodes.push(node.flags[1]);
    nodes.extend_from_slice(&node.position[2].to_le_bytes());
    nodes.push(node.flags[2]);
    let link_count =
      u8::try_from(node.links.len()).map_err(|_| invalid("YND node link count exceeds 31"))?;
    nodes.push((link_count << 3) | (node.link_unknown & 7));
    nodes.push(node.flags[3]);
    nodes.push(node.flags[4]);
    for link in &node.links {
      links.extend_from_slice(&link.area.to_le_bytes());
      links.extend_from_slice(&link.id.to_le_bytes());
      links.extend_from_slice(&link.flags);
    }
  }
  let nodes_at = append(&mut data, &nodes)?;
  let links_at = append(&mut data, &links)?;
  let mut junction_records = Vec::new();
  let mut heightmaps = Vec::new();
  for junction in &dictionary.junctions {
    let offset =
      u16::try_from(heightmaps.len()).map_err(|_| invalid("YND heightmap exceeds u16 offsets"))?;
    junction_records.extend_from_slice(&junction.max_z.to_le_bytes());
    junction_records.extend_from_slice(&junction.x.to_le_bytes());
    junction_records.extend_from_slice(&junction.y.to_le_bytes());
    junction_records.extend_from_slice(&junction.min_z.to_le_bytes());
    junction_records.extend_from_slice(&offset.to_le_bytes());
    junction_records.push(junction.width);
    junction_records.push(junction.height);
    heightmaps.extend_from_slice(&junction.heightmap);
  }
  let junctions_at = append(&mut data, &junction_records)?;
  let heightmaps_at = append(&mut data, &heightmaps)?;
  let mut refs = Vec::new();
  for item in &dictionary.refs {
    refs.extend_from_slice(&item.area.to_le_bytes());
    refs.extend_from_slice(&item.node.to_le_bytes());
    refs.extend_from_slice(&item.junction.to_le_bytes());
    refs.extend_from_slice(&item.unknown.to_le_bytes());
  }
  let refs_at = append(&mut data, &refs)?;
  let content_end = data.len();
  let mut page_slots = 1;
  let (page_info, page_count) = loop {
    data.truncate(content_end);
    let offset = reserve(&mut data, 16 + page_slots * 8)?;
    let probe = Rsc7Resource::from_pages(1, &data, &[])?;
    let actual = page_count(probe.system_flags).max(1);
    if actual <= page_slots {
      break (offset, actual);
    }
    page_slots = actual;
  };
  put_u32(&mut data, 0, 0x405b_c808)?;
  put_u32(&mut data, 4, 1)?;
  put_u64(&mut data, 8, addr(page_info))?;
  data[page_info + 8] = page_count as u8;
  let body = ROOT_SIZE;
  put_u64(&mut data, body, addr(nodes_at))?;
  put_u32(&mut data, body + 8, dictionary.nodes.len() as u32)?;
  put_u32(&mut data, body + 12, dictionary.vehicle_count)?;
  put_u32(&mut data, body + 16, dictionary.ped_count)?;
  put_u64(&mut data, body + 24, addr(links_at))?;
  put_u32(&mut data, body + 32, (links.len() / 8) as u32)?;
  put_u64(&mut data, body + 40, addr(junctions_at))?;
  put_u64(&mut data, body + 48, addr(heightmaps_at))?;
  put_u32(&mut data, body + 56, 1)?;
  put_u64(&mut data, body + 64, addr(refs_at))?;
  put_u16(&mut data, body + 72, dictionary.refs.len() as u16)?;
  put_u16(&mut data, body + 74, dictionary.refs.len() as u16)?;
  put_u32(&mut data, body + 80, dictionary.junctions.len() as u32)?;
  put_u32(&mut data, body + 84, heightmaps.len() as u32)?;
  Rsc7Resource::from_pages(1, &data, &[])
}

fn to_xml(dictionary: &Dictionary) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<NodeDictionary>\n");
  tag(&mut xml, 1, "VehicleNodeCount", dictionary.vehicle_count);
  tag(&mut xml, 1, "PedNodeCount", dictionary.ped_count);
  open(&mut xml, 1, "Nodes");
  for node in &dictionary.nodes {
    open(&mut xml, 2, "Item");
    tag(&mut xml, 3, "AreaID", node.area);
    tag(&mut xml, 3, "NodeID", node.id);
    if node.street == 0 {
      empty(&mut xml, 3, "StreetName");
    } else {
      text_tag(&mut xml, 3, "StreetName", &format!("hash_{:08X}", node.street));
    }
    empty(
      &mut xml,
      3,
      &format!(
        "Position x=\"{}\" y=\"{}\" z=\"{}\"",
        node.position[0] as f32 / 4.0,
        node.position[1] as f32 / 4.0,
        node.position[2] as f32 / 32.0
      ),
    );
    for (i, flag) in node.flags.iter().enumerate() {
      tag(&mut xml, 3, &format!("Flags{i}"), flag);
    }
    tag(&mut xml, 3, "Flags5", node.link_unknown);
    open(&mut xml, 3, "Links");
    for link in &node.links {
      open(&mut xml, 4, "Item");
      tag(&mut xml, 5, "ToAreaID", link.area);
      tag(&mut xml, 5, "ToNodeID", link.id);
      for (i, field) in ["Flags0", "Flags1", "Flags2", "LinkLength"].iter().enumerate() {
        tag(&mut xml, 5, field, link.flags[i]);
      }
      close(&mut xml, 4, "Item");
    }
    close(&mut xml, 3, "Links");
    close(&mut xml, 2, "Item");
  }
  close(&mut xml, 1, "Nodes");
  open(&mut xml, 1, "Junctions");
  for item in &dictionary.junctions {
    open(&mut xml, 2, "Item");
    empty(
      &mut xml,
      3,
      &format!("Position x=\"{}\" y=\"{}\"", item.x as f32 / 4.0, item.y as f32 / 4.0),
    );
    tag(&mut xml, 3, "MinZ", item.min_z as f32 / 32.0);
    tag(&mut xml, 3, "MaxZ", item.max_z as f32 / 32.0);
    tag(&mut xml, 3, "SizeX", item.width);
    tag(&mut xml, 3, "SizeY", item.height);
    text_tag(
      &mut xml,
      3,
      "Heightmap",
      &item.heightmap.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
    );
    close(&mut xml, 2, "Item");
  }
  close(&mut xml, 1, "Junctions");
  open(&mut xml, 1, "JunctionRefs");
  for item in &dictionary.refs {
    open(&mut xml, 2, "Item");
    tag(&mut xml, 3, "AreaID", item.area);
    tag(&mut xml, 3, "NodeID", item.node);
    tag(&mut xml, 3, "JunctionID", item.junction);
    tag(&mut xml, 3, "Unk0", item.unknown);
    close(&mut xml, 2, "Item");
  }
  close(&mut xml, 1, "JunctionRefs");
  close(&mut xml, 0, "NodeDictionary");
  xml
}

fn read_table(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
  size: usize,
) -> io::Result<Vec<u8>> {
  if count == 0 {
    return Ok(Vec::new());
  }
  let length = count.checked_mul(size).ok_or_else(|| invalid("YND table size overflows"))?;
  Ok(resource.read_address(pointer, length)?.to_vec())
}
fn append(
  data: &mut Vec<u8>,
  bytes: &[u8],
) -> io::Result<usize> {
  let offset = reserve(data, bytes.len())?;
  data[offset..offset + bytes.len()].copy_from_slice(bytes);
  Ok(offset)
}
fn reserve(
  data: &mut Vec<u8>,
  length: usize,
) -> io::Result<usize> {
  let padding = (16 - data.len() % 16) % 16;
  data.resize(data.len() + padding, 0);
  let offset = data.len();
  let end = offset.checked_add(length).ok_or_else(|| invalid("YND layout overflows"))?;
  data.resize(end, 0);
  Ok(offset)
}
fn addr(offset: usize) -> u64 {
  BASE + offset as u64
}
fn page_count(flags: u32) -> usize {
  ((flags >> 27) & 1) as usize
    + (((flags >> 26) & 1) << 1) as usize
    + (((flags >> 25) & 1) << 2) as usize
    + (((flags >> 24) & 1) << 3) as usize
    + (((flags >> 17) & 0x7f) << 4) as usize
    + (((flags >> 11) & 0x3f) << 5) as usize
    + (((flags >> 7) & 0xf) << 6) as usize
    + (((flags >> 5) & 3) << 7) as usize
    + (((flags >> 4) & 1) << 8) as usize
}
fn put_u16(
  data: &mut [u8],
  offset: usize,
  value: u16,
) -> io::Result<()> {
  put(data, offset, &value.to_le_bytes())
}
fn put_u32(
  data: &mut [u8],
  offset: usize,
  value: u32,
) -> io::Result<()> {
  put(data, offset, &value.to_le_bytes())
}
fn put_u64(
  data: &mut [u8],
  offset: usize,
  value: u64,
) -> io::Result<()> {
  put(data, offset, &value.to_le_bytes())
}
fn put(
  data: &mut [u8],
  offset: usize,
  value: &[u8],
) -> io::Result<()> {
  let end =
    offset.checked_add(value.len()).ok_or_else(|| invalid("YND output offset overflows"))?;
  data
    .get_mut(offset..end)
    .ok_or_else(|| invalid("YND output range is out of bounds"))?
    .copy_from_slice(value);
  Ok(())
}
fn u16_at(
  data: &[u8],
  offset: usize,
) -> io::Result<u16> {
  Ok(u16::from_le_bytes(array(data, offset)?))
}
fn i16_at(
  data: &[u8],
  offset: usize,
) -> io::Result<i16> {
  Ok(i16::from_le_bytes(array(data, offset)?))
}
fn u32_at(
  data: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_le_bytes(array(data, offset)?))
}
fn u64_at(
  data: &[u8],
  offset: usize,
) -> io::Result<u64> {
  Ok(u64::from_le_bytes(array(data, offset)?))
}
fn read_u16(
  data: &[u8],
  offset: usize,
) -> io::Result<u16> {
  u16_at(data, offset)
}
fn read_u32(
  data: &[u8],
  offset: usize,
) -> io::Result<u32> {
  u32_at(data, offset)
}
fn read_u64(
  data: &[u8],
  offset: usize,
) -> io::Result<u64> {
  u64_at(data, offset)
}
fn array<const N: usize>(
  data: &[u8],
  offset: usize,
) -> io::Result<[u8; N]> {
  let end = offset.checked_add(N).ok_or_else(|| invalid("YND read offset overflows"))?;
  data
    .get(offset..end)
    .ok_or_else(|| invalid("YND record is truncated"))?
    .try_into()
    .map_err(|_| invalid("YND record is truncated"))
}
fn child<'a>(
  node: &'a XmlElement,
  name: &str,
) -> io::Result<&'a XmlElement> {
  node
    .children
    .iter()
    .find(|child| child.name == name)
    .ok_or_else(|| invalid(&format!("YND XML missing {name}")))
}
fn value(node: &XmlElement) -> &str {
  node.attributes.get("value").map(String::as_str).unwrap_or(node.text.trim())
}
fn number<T: std::str::FromStr>(
  node: &XmlElement,
  name: &str,
) -> io::Result<T> {
  value(child(node, name)?).parse().map_err(|_| invalid(&format!("YND XML {name} is invalid")))
}
fn float_number(
  node: &XmlElement,
  name: &str,
) -> io::Result<f32> {
  number(node, name)
}
fn float_attr(
  node: &XmlElement,
  name: &str,
) -> io::Result<f32> {
  node
    .attributes
    .get(name)
    .ok_or_else(|| invalid(&format!("YND vector missing {name}")))?
    .parse()
    .map_err(|_| invalid("YND vector coordinate is invalid"))
}
fn items<'a>(
  node: &'a XmlElement,
  name: &str,
) -> impl Iterator<Item = &'a XmlElement> + 'a {
  node
    .children
    .iter()
    .find(|child| child.name == name)
    .into_iter()
    .flat_map(|array| array.children.iter().filter(|item| item.name == "Item"))
}
fn parse_hash(text: &str) -> io::Result<u32> {
  let text = text.trim();
  if let Some(hash) = text.strip_prefix("hash_") {
    u32::from_str_radix(hash, 16).map_err(|_| invalid("YND hash is invalid"))
  } else {
    Ok(jenk_hash(text))
  }
}
fn jenk_hash(text: &str) -> u32 {
  let mut hash = 0u32;
  for byte in text.bytes() {
    hash = hash.wrapping_add(byte as u32);
    hash = hash.wrapping_add(hash << 10);
    hash ^= hash >> 6;
  }
  hash = hash.wrapping_add(hash << 3);
  hash ^= hash >> 11;
  hash.wrapping_add(hash << 15)
}
fn parse_hex(text: &str) -> io::Result<Vec<u8>> {
  text
    .split_whitespace()
    .map(|byte| {
      u8::from_str_radix(byte.trim_start_matches("0x"), 16)
        .map_err(|_| invalid("YND heightmap contains invalid hex"))
    })
    .collect()
}
fn tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: impl std::fmt::Display,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name} value=\"{value}\" />\n"));
}
fn text_tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name}>{value}</{name}>\n"));
}
fn open(
  xml: &mut String,
  depth: usize,
  name: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name}>\n"));
}
fn close(
  xml: &mut String,
  depth: usize,
  name: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("</{name}>\n"));
}
fn empty(
  xml: &mut String,
  depth: usize,
  tag: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{tag} />\n"));
}
fn indent(
  xml: &mut String,
  depth: usize,
) {
  xml.extend(std::iter::repeat_n(' ', depth));
}
fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
