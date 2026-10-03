use std::{collections::HashMap, io};

use super::{
  meta_resource::jenk_hash,
  meta_xml::known_hash_names,
  xml_tree::{XmlElement, parse_xml},
};

#[derive(Debug, Clone)]
struct Field {
  name: u32,
  kind: u8,
  subtype: u8,
  offset: usize,
  reference: u32,
}

#[derive(Debug, Clone)]
struct Schema {
  size: usize,
  fields: Vec<Field>,
}

#[derive(Debug)]
struct Block {
  name: u32,
  offset: usize,
  length: usize,
}

/// A big-endian PSO container with embedded schemas and data mappings.
///
/// # Examples
///
/// ```no_run
/// use mlo_merger::core::format::gamefile::pso::PsoResource;
/// let bytes = std::fs::read("asset/vanilla/ymap/id2_17.ymap")?;
/// let resource = PsoResource::parse(&bytes)?;
/// let xml = resource.to_xml(&Default::default())?;
/// let rebuilt = resource.rebuild_xml(&xml)?;
/// assert!(rebuilt.starts_with(b"PSIN"));
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug)]
pub struct PsoResource {
  data: Vec<u8>,
  root: usize,
  blocks: Vec<Block>,
  structures: HashMap<u32, Schema>,
  enums: HashMap<u32, Vec<(u32, i32)>>,
  sections: Vec<([u8; 4], Vec<u8>)>,
}

impl PsoResource {
  /// Rewrites existing PSO fields using this resource as a template.
  /// Array lengths, structure types, and allocated string capacities must remain unchanged.
  pub fn rebuild_xml(
    mut self,
    xml: &str,
  ) -> io::Result<Vec<u8>> {
    let root = parse_xml(xml)?;
    let kind = self.blocks[self.root - 1].name;
    if xml_hash(&root.name)? != kind {
      return Err(invalid("PSO XML root differs from template"));
    }
    let original = self.data.clone();
    self.patch_node(self.root, 0, kind, &root, 0)?;
    if self.data != original && self.sections.iter().any(|(tag, _)| tag == b"CHKS") {
      return Err(invalid("editing checksummed PSO is not supported yet"));
    }
    let mut output = Vec::new();
    for (tag, bytes) in self.sections {
      if &tag == b"PSIN" {
        output.extend_from_slice(&self.data);
      } else {
        output.extend_from_slice(&bytes);
      }
    }
    Ok(output)
  }

  fn patch_node(
    &mut self,
    block: usize,
    offset: usize,
    kind: u32,
    node: &XmlElement,
    depth: usize,
  ) -> io::Result<()> {
    if depth > 64 {
      return Err(invalid("PSO reference nesting exceeds 64 levels"));
    }
    let schema =
      self.structures.get(&kind).cloned().ok_or_else(|| invalid("PSO embedded schema missing"))?;
    let address = self.address(block, offset, schema.size)?;
    for field in &schema.fields {
      if field.name == 0x100 {
        continue;
      }
      let child = node
        .children
        .iter()
        .find(|child| xml_hash(&child.name).ok() == Some(field.name))
        .ok_or_else(|| invalid(&format!("PSO XML field hash_{:08X} missing", field.name)))?;
      let location = address + field.offset;
      let raw_value =
        child.attributes.get("value").map(String::as_str).unwrap_or(child.text.trim());
      let number = || -> io::Result<u64> {
        if let Some(hex) = raw_value.strip_prefix("0x").or_else(|| raw_value.strip_prefix("0X")) {
          u64::from_str_radix(hex, 16).map_err(|_| invalid("invalid PSO hex integer"))
        } else if raw_value.starts_with('-') {
          raw_value
            .parse::<i64>()
            .map(|value| value as u64)
            .map_err(|_| invalid("invalid PSO integer"))
        } else {
          raw_value.parse().map_err(|_| invalid("invalid PSO integer"))
        }
      };
      match field.kind {
        0 => {
          self.data[location] = match raw_value {
            "true" => 1,
            "false" => 0,
            _ => return Err(invalid("invalid PSO bool")),
          }
        }
        1 | 2 => self.data[location] = number()? as u8,
        3 | 4 | 30 => self.put(location, &(number()? as u16).to_be_bytes())?,
        5 | 6 => self.put(location, &(number()? as u32).to_be_bytes())?,
        7 => self.put(location, &float_text(raw_value)?.to_bits().to_be_bytes())?,
        8 | 9 | 10 | 20 | 21 => {
          let axes = if field.kind == 8 {
            "xy"
          } else if field.kind == 10 {
            "xyzw"
          } else {
            "xyz"
          };
          for (index, axis) in axes.chars().enumerate() {
            let value = child
              .attributes
              .get(&axis.to_string())
              .ok_or_else(|| invalid("PSO vector component missing"))?;
            self.put(location + index * 4, &float_text(value)?.to_bits().to_be_bytes())?;
          }
        }
        11 => {
          let text = quick_xml::escape::unescape(child.text.trim())
            .map_err(|_| invalid("invalid PSO XML string"))?;
          if matches!(field.subtype, 7 | 8) {
            self.put(location, &xml_hash(&text)?.to_be_bytes())?;
          } else {
            let (start, capacity) = if field.subtype == 0 {
              (location, (field.reference >> 16) as usize)
            } else {
              let (target, position) = self.pointer(location)?;
              if target == 0 {
                if text.is_empty() {
                  continue;
                }
                return Err(invalid("cannot allocate a new PSO string pointer"));
              }
              let start = self.address(target, position, 1)?;
              let end = self.blocks[target - 1].offset + self.blocks[target - 1].length;
              let length =
                self.data[start..end].iter().position(|byte| *byte == 0).unwrap_or(end - start);
              (start, length)
            };
            if text.len() > capacity {
              return Err(invalid("PSO string exceeds template capacity"));
            }
            slice(&self.data, start, capacity)?;
            self.data[start..start + capacity].fill(0);
            self.put(start, text.as_bytes())?;
            if field.subtype == 3 {
              let length = u16::try_from(text.len()).map_err(|_| invalid("PSO string too long"))?;
              self.put(location + 8, &length.to_be_bytes())?;
              self.put(location + 10, &length.to_be_bytes())?;
            }
          }
        }
        12 => {
          if field.subtype == 0 {
            self.patch_node(block, offset + field.offset, field.reference, child, depth + 1)?;
          } else {
            let (target, position) = self.pointer(location)?;
            if target == 0 {
              if !child.children.is_empty() {
                return Err(invalid("cannot add a PSO structure pointer"));
              }
            } else {
              let kind = self.blocks[target - 1].name;
              if child
                .attributes
                .get("type")
                .map(|name| xml_hash(name))
                .transpose()?
                .is_some_and(|name| name != kind)
              {
                return Err(invalid("PSO structure type differs from template"));
              }
              self.patch_node(
                target,
                position,
                if field.reference == 0 { kind } else { field.reference },
                child,
                depth + 1,
              )?;
            }
          }
        }
        13 => {
          let mut item_index = (field.reference & 0xffff) as usize;
          if item_index >= schema.fields.len() {
            item_index = (field.reference & 0xfff) as usize;
          }
          let item =
            schema.fields.get(item_index).ok_or_else(|| invalid("PSO array schema missing"))?;
          let (target, position, count) = if matches!(field.subtype, 0 | 4) {
            let (target, position) = self.pointer(location)?;
            (target, position, u16_at(&self.data, location + 8)? as usize)
          } else {
            (block, offset + field.offset, (field.reference >> 16) as usize)
          };
          if item.kind == 12 {
            if child.children.len() != count {
              return Err(invalid("PSO array length differs from template"));
            }
            for (index, element) in child.children.iter().enumerate() {
              if item.subtype == 3 {
                let pointer = self.address(target, position + index * 8, 8)?;
                let (target, position) = self.pointer(pointer)?;
                if target == 0 {
                  if !element.children.is_empty() {
                    return Err(invalid("cannot add a PSO array pointer"));
                  }
                  continue;
                }
                let kind = self.blocks[target - 1].name;
                if element
                  .attributes
                  .get("type")
                  .map(|name| xml_hash(name))
                  .transpose()?
                  .is_some_and(|name| name != kind)
                {
                  return Err(invalid("PSO array structure type differs from template"));
                }
                self.patch_node(target, position, kind, element, depth + 1)?;
              } else {
                let size = self
                  .structures
                  .get(&item.reference)
                  .ok_or_else(|| invalid("PSO array structure missing"))?
                  .size;
                self.patch_node(
                  target,
                  position + index * size,
                  item.reference,
                  element,
                  depth + 1,
                )?;
              }
            }
          } else {
            let values = if item.kind == 11 {
              child.children.iter().map(|child| child.text.trim()).collect::<Vec<_>>()
            } else {
              child
                .text
                .split(|character: char| character.is_whitespace() || character == ',')
                .filter(|value| !value.is_empty())
                .collect()
            };
            let components = if item.kind == 9 { 3 } else { 1 };
            if values.len() != count * components {
              return Err(invalid("PSO scalar array length differs from template"));
            }
            let width = match item.kind {
              0..=2 => 1,
              3 | 4 => 2,
              5..=7 | 11 => 4,
              9 => 16,
              _ => return Err(invalid("unsupported PSO scalar array rewrite")),
            };
            for index in 0..count {
              let start = self.address(target, position + index * width, width)?;
              for component in 0..components {
                let text = values[index * components + component];
                let bits = match item.kind {
                  7 | 9 => float_text(text)?.to_bits(),
                  11 => xml_hash(text)?,
                  0 => match text {
                    "true" => 1,
                    "false" => 0,
                    _ => return Err(invalid("invalid PSO bool array")),
                  },
                  _ => {
                    text.parse::<i64>().map_err(|_| invalid("invalid PSO integer array"))? as u32
                  }
                };
                match width {
                  1 => self.data[start] = bits as u8,
                  2 => self.put(start, &(bits as u16).to_be_bytes())?,
                  _ => self.put(start + component * 4, &bits.to_be_bytes())?,
                }
              }
            }
          }
        }
        14 => {
          let text = child.text.trim();
          let number = self
            .enums
            .get(&field.reference)
            .and_then(|entries| entries.iter().find(|(name, _)| xml_hash(text).ok() == Some(*name)))
            .map(|(_, value)| *value)
            .ok_or_else(|| invalid("PSO enum name is not in schema"))?;
          if field.subtype == 2 {
            self.data[location] = number as u8;
          } else {
            self.put(location, &(number as u32).to_be_bytes())?;
          }
        }
        15 => {
          let flags = schema
            .fields
            .get((field.reference & 0xfff) as usize)
            .filter(|entry| entry.name == 0x100)
            .and_then(|entry| self.enums.get(&entry.reference));
          let number = if let Some(flags) = flags {
            let mut value = 0u32;
            for name in child.text.split_whitespace() {
              let bit = flags
                .iter()
                .find(|(hash, _)| xml_hash(name).ok() == Some(*hash))
                .map(|(_, bit)| *bit)
                .ok_or_else(|| invalid("unknown PSO flag"))?;
              if !(0..32).contains(&bit) {
                return Err(invalid("invalid PSO flag bit"));
              }
              value |= 1 << bit;
            }
            value
          } else if raw_value.is_empty() {
            0
          } else {
            number()? as u32
          };
          match field.subtype {
            0 => self.put(location, &number.to_be_bytes())?,
            1 => self.put(location, &(number as u16).to_be_bytes())?,
            2 => self.data[location] = number as u8,
            _ => return Err(invalid("unsupported PSO flags subtype")),
          }
        }
        _ => return Err(invalid("unsupported PSO field rewrite")),
      }
    }
    Ok(())
  }

  fn put(
    &mut self,
    offset: usize,
    bytes: &[u8],
  ) -> io::Result<()> {
    slice(&self.data, offset, bytes.len())?;
    self.data[offset..offset + bytes.len()].copy_from_slice(bytes);
    Ok(())
  }

  /// Exports supported embedded PSO schemas as CodeWalker-style XML.
  pub fn to_xml(
    &self,
    shared_names: &HashMap<u32, String>,
  ) -> io::Result<String> {
    let mut names = known_hash_names();
    for (tag, data) in &self.sections {
      if matches!(tag, b"STRF" | b"STRS") {
        for value in data[8..].split(|byte| *byte == 0).filter(|value| !value.is_empty()) {
          if let Ok(value) = std::str::from_utf8(value) {
            names.insert(jenk_hash(value), value.to_string());
            let lower = value.to_ascii_lowercase();
            names.insert(jenk_hash(&lower), lower);
          }
        }
      }
    }
    names.extend(shared_names.iter().map(|(hash, name)| (*hash, name.clone())));
    let block = &self.blocks[self.root - 1];
    let mut root = XmlElement {
      name: resolve(block.name, &names),
      ..XmlElement::default()
    };
    self.read_node(self.root, 0, block.name, &names, &mut root, 0)?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    render(&root, 0, &mut xml);
    Ok(xml)
  }

  fn address(
    &self,
    block: usize,
    offset: usize,
    length: usize,
  ) -> io::Result<usize> {
    let block = self
      .blocks
      .get(block.checked_sub(1).ok_or_else(|| invalid("null PSO block"))?)
      .ok_or_else(|| invalid("PSO pointer block is out of range"))?;
    let offset = if offset >= block.length { offset >> 8 } else { offset };
    if offset.checked_add(length).is_none_or(|end| end > block.length) {
      return Err(invalid("PSO field exceeds its block"));
    }
    Ok(block.offset + offset)
  }

  fn pointer(
    &self,
    address: usize,
  ) -> io::Result<(usize, usize)> {
    slice(&self.data, address, 8)?;
    let pointer = u32_at(&self.data, address)? as u64;
    Ok(((pointer & 0xfff) as usize, ((pointer >> 12) & 0xfffff) as usize))
  }

  fn read_node(
    &self,
    block: usize,
    offset: usize,
    kind: u32,
    names: &HashMap<u32, String>,
    node: &mut XmlElement,
    depth: usize,
  ) -> io::Result<()> {
    if depth > 64 {
      return Err(invalid("PSO reference nesting exceeds 64 levels"));
    }
    let schema =
      self.structures.get(&kind).ok_or_else(|| invalid("PSO embedded structure schema missing"))?;
    let address = self.address(block, offset, schema.size)?;
    for field in &schema.fields {
      if field.name == 0x100 {
        continue;
      }
      let mut child = XmlElement {
        name: resolve(field.name, names),
        ..XmlElement::default()
      };
      let location =
        address.checked_add(field.offset).ok_or_else(|| invalid("PSO field offset overflow"))?;
      match field.kind {
        0 => value(
          &mut child,
          if *slice(&self.data, location, 1)?.first().unwrap() != 0 { "true" } else { "false" },
        ),
        1 => value(&mut child, (slice(&self.data, location, 1)?[0] as i8).to_string()),
        2 => value(&mut child, slice(&self.data, location, 1)?[0].to_string()),
        3 | 30 => value(&mut child, (u16_at(&self.data, location)? as i16).to_string()),
        4 => value(&mut child, u16_at(&self.data, location)?.to_string()),
        5 => value(&mut child, (u32_at(&self.data, location)? as i32).to_string()),
        6 => match field.subtype {
          0 => value(&mut child, (u32_at(&self.data, location)? as i32).to_string()),
          1 => value(&mut child, format!("0x{:08X}", u32_at(&self.data, location)?)),
          _ => return Err(invalid("unsupported PSO integer subtype")),
        },
        7 => value(&mut child, f32::from_bits(u32_at(&self.data, location)?).to_string()),
        8 | 9 | 10 | 20 | 21 => {
          let axes = if field.kind == 8 {
            "xy"
          } else if field.kind == 10 {
            "xyzw"
          } else {
            "xyz"
          };
          for (index, axis) in axes.chars().enumerate() {
            child.attributes.insert(
              axis.to_string(),
              f32::from_bits(u32_at(&self.data, location + index * 4)?).to_string(),
            );
          }
        }
        11 => child.text = self.string(field, location, names)?,
        12 => {
          if field.subtype == 0 {
            self.read_node(
              block,
              offset + field.offset,
              field.reference,
              names,
              &mut child,
              depth + 1,
            )?;
          } else if matches!(field.subtype, 3 | 4) {
            let (target, position) = self.pointer(location)?;
            if target != 0 {
              let kind = self
                .blocks
                .get(target - 1)
                .ok_or_else(|| invalid("invalid PSO structure pointer"))?
                .name;
              child.attributes.insert("type".into(), resolve(kind, names));
              self.read_node(
                target,
                position,
                if field.reference == 0 { kind } else { field.reference },
                names,
                &mut child,
                depth + 1,
              )?;
            }
          } else {
            return Err(invalid("unsupported PSO structure subtype"));
          }
        }
        13 => self.read_array(field, schema, (block, offset), names, &mut child, depth + 1)?,
        14 => {
          let number = match field.subtype {
            0 => u32_at(&self.data, location)? as i32,
            2 => slice(&self.data, location, 1)?[0] as i32,
            _ => return Err(invalid("unsupported PSO enum subtype")),
          };
          let enum_values =
            self.enums.get(&field.reference).ok_or_else(|| invalid("PSO enum schema missing"))?;
          child.text = resolve(
            enum_values
              .iter()
              .find(|(_, value)| *value == number)
              .map(|(name, _)| *name)
              .unwrap_or(0),
            names,
          );
        }
        15 => {
          let number = match field.subtype {
            0 => u32_at(&self.data, location)?,
            1 => u16_at(&self.data, location)? as u32,
            2 => slice(&self.data, location, 1)?[0] as u32,
            _ => return Err(invalid("unsupported PSO flags subtype")),
          };
          let flags = schema
            .fields
            .get((field.reference & 0xfff) as usize)
            .filter(|info| info.name == 0x100)
            .and_then(|info| self.enums.get(&info.reference));
          if let Some(flags) = flags {
            child.text = flags
              .iter()
              .filter(|(_, bit)| *bit >= 0 && *bit < 32 && number & (1 << *bit) != 0)
              .map(|(name, _)| resolve(*name, names))
              .collect::<Vec<_>>()
              .join(" ");
          } else if number != 0 {
            value(&mut child, number.to_string());
          }
        }
        32 => value(
          &mut child,
          (u32_at(&self.data, location)? as u64
            | ((u32_at(&self.data, location + 4)? as u64) << 32))
            .to_string(),
        ),
        _ => return Err(invalid(&format!("unsupported PSO field type {}", field.kind))),
      }
      node.children.push(child);
    }
    Ok(())
  }

  fn string(
    &self,
    field: &Field,
    address: usize,
    names: &HashMap<u32, String>,
  ) -> io::Result<String> {
    if matches!(field.subtype, 7 | 8) {
      return Ok(resolve(u32_at(&self.data, address)?, names));
    }
    if field.subtype == 0 {
      let bytes = slice(&self.data, address, (field.reference >> 16) as usize)?;
      return Ok(String::from_utf8_lossy(bytes).replace('\0', ""));
    }
    if matches!(field.subtype, 1..=3) {
      let (block, offset) = self.pointer(address)?;
      if block == 0 {
        return Ok(String::new());
      }
      let start = self.address(block, offset, 1)?;
      let end = self.blocks[block - 1].offset + self.blocks[block - 1].length;
      let bytes = &self.data[start..end];
      let length = if field.subtype == 3 {
        (u16_at(&self.data, address + 8)? as usize).min(bytes.len())
      } else {
        bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len())
      };
      return Ok(String::from_utf8_lossy(&bytes[..length]).trim_end_matches('\0').to_string());
    }
    Err(invalid("unsupported PSO string subtype"))
  }

  fn read_array(
    &self,
    field: &Field,
    schema: &Schema,
    location: (usize, usize),
    names: &HashMap<u32, String>,
    node: &mut XmlElement,
    depth: usize,
  ) -> io::Result<()> {
    let (block, offset) = location;
    let address = self.address(block, offset, schema.size)? + field.offset;
    let mut index = (field.reference & 0xffff) as usize;
    if index >= schema.fields.len() {
      index = (field.reference & 0xfff) as usize;
    }
    let item =
      schema.fields.get(index).ok_or_else(|| invalid("PSO array element schema missing"))?;
    let (target, position, count) = match field.subtype {
      0 | 4 => {
        let (target, position) = self.pointer(address)?;
        (target, position, u16_at(&self.data, address + 8)? as usize)
      }
      1 | 2 | 129 => (block, offset + field.offset, (field.reference >> 16) as usize),
      _ => return Err(invalid("unsupported PSO array subtype")),
    };
    if item.kind == 12 {
      if count == 0 || item.subtype == 0 {
        node.attributes.insert("itemType".into(), resolve(item.reference, names));
      }
      for element in 0..count {
        let mut child = XmlElement {
          name: "Item".into(),
          ..XmlElement::default()
        };
        if item.subtype == 3 {
          let pointer = self.address(target, position + element * 8, 8)?;
          let (target, position) = self.pointer(pointer)?;
          if target != 0 {
            let kind = self
              .blocks
              .get(target - 1)
              .ok_or_else(|| invalid("PSO array pointer out of range"))?
              .name;
            child.attributes.insert("type".into(), resolve(kind, names));
            self.read_node(target, position, kind, names, &mut child, depth)?;
          }
        } else if item.subtype == 0 {
          let kind = item.reference;
          let size =
            self.structures.get(&kind).ok_or_else(|| invalid("PSO array structure missing"))?.size;
          self.read_node(target, position + element * size, kind, names, &mut child, depth)?;
        } else {
          return Err(invalid("unsupported PSO structure array subtype"));
        }
        node.children.push(child);
      }
      return Ok(());
    }
    if item.kind == 11 && matches!(item.subtype, 7 | 8) {
      for element in 0..count {
        let address = self.address(target, position + element * 4, 4)?;
        node.children.push(XmlElement {
          name: "Item".into(),
          text: resolve(u32_at(&self.data, address)?, names),
          ..XmlElement::default()
        });
      }
      return Ok(());
    }
    let (label, width) = match item.kind {
      0 => ("bool", 1),
      1 => ("sbyte", 1),
      2 => ("byte", 1),
      3 => ("short", 2),
      4 => ("ushort", 2),
      5 => ("int", 4),
      6 => ("uint", 4),
      7 => ("float", 4),
      9 => ("Vector3", 16),
      _ => return Err(invalid(&format!("unsupported PSO array element type {}", item.kind))),
    };
    if item.kind == 9 {
      node.attributes.insert("itemType".into(), label.into());
    }
    let mut values = Vec::new();
    for element in 0..count {
      let address = self.address(target, position + element * width, width)?;
      let value = match item.kind {
        0 => (self.data[address] != 0).to_string(),
        1 => (self.data[address] as i8).to_string(),
        2 => self.data[address].to_string(),
        3 => (u16_at(&self.data, address)? as i16).to_string(),
        4 => u16_at(&self.data, address)?.to_string(),
        5 => (u32_at(&self.data, address)? as i32).to_string(),
        6 => u32_at(&self.data, address)?.to_string(),
        7 => f32::from_bits(u32_at(&self.data, address)?).to_string(),
        9 => (0..3)
          .map(|axis| {
            u32_at(&self.data, address + axis * 4).map(|value| f32::from_bits(value).to_string())
          })
          .collect::<io::Result<Vec<_>>>()?
          .join(", "),
        _ => unreachable!(),
      };
      values.push(value);
    }
    node.text = values.join("\n");
    Ok(())
  }

  /// Reads PSIN/PMAP/PSCH sections, rejecting truncated or ambiguous containers.
  pub fn parse(bytes: &[u8]) -> io::Result<Self> {
    if !bytes.starts_with(b"PSIN") {
      return Err(invalid("invalid PSO magic"));
    }
    let mut sections = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
      let tag: [u8; 4] = slice(bytes, offset, 4)?.try_into().unwrap();
      let length = u32_at(bytes, offset + 4)? as usize;
      if length < 8 {
        return Err(invalid("PSO section length is smaller than its header"));
      }
      if sections.iter().any(|(previous, _)| *previous == tag) {
        return Err(invalid("duplicate PSO section"));
      }
      sections.push((tag, slice(bytes, offset, length)?.to_vec()));
      offset = offset.checked_add(length).ok_or_else(|| invalid("PSO offset overflow"))?;
    }
    let section = |tag: &[u8; 4]| -> io::Result<&[u8]> {
      sections
        .iter()
        .find(|(name, _)| name == tag)
        .map(|(_, bytes)| bytes.as_slice())
        .ok_or_else(|| invalid("required PSO section missing"))
    };
    let data = section(b"PSIN")?.to_vec();
    let map = section(b"PMAP")?;
    let root = u32_at(map, 8)? as usize;
    let initial_count = u16_at(map, 12)?;
    let (count, start) = if initial_count == 0 || initial_count > i16::MAX as u16 {
      (u16_at(map, 16)? as usize, 24)
    } else {
      (initial_count as usize, 16)
    };
    let mut blocks = Vec::new();
    for index in 0..count {
      let record = start + index * 16;
      let block = Block {
        name: u32_at(map, record)?,
        offset: u32_at(map, record + 4)? as usize,
        length: u32_at(map, record + 12)? as usize,
      };
      slice(&data, block.offset, block.length)?;
      blocks.push(block);
    }
    if root == 0 || root > blocks.len() {
      return Err(invalid("PSO root block is out of range"));
    }
    let schema = section(b"PSCH")?;
    let count = u32_at(schema, 8)? as usize;
    slice(schema, 12, count.checked_mul(8).ok_or_else(|| invalid("PSO schema count overflow"))?)?;
    let mut structures = HashMap::new();
    let mut enums = HashMap::new();
    for index in 0..count {
      let name = u32_at(schema, 12 + index * 8)?;
      let record = u32_at(schema, 16 + index * 8)? as usize;
      let header = u32_at(schema, record)?;
      match header >> 24 {
        0 => {
          let size = u32_at(schema, record + 4)? as usize;
          let field_count = (header & 0xffff) as usize;
          slice(schema, record + 12, field_count * 12)?;
          let mut fields = Vec::new();
          for field_index in 0..field_count {
            let location = record + 12 + field_index * 12;
            fields.push(Field {
              name: u32_at(schema, location)?,
              kind: schema[location + 4],
              subtype: schema[location + 5],
              offset: u16_at(schema, location + 6)? as usize,
              reference: u32_at(schema, location + 8)?,
            });
          }
          structures.insert(
            name,
            Schema {
              size,
              fields,
            },
          );
        }
        1 => {
          let entry_count = (header & 0xffffff) as usize;
          slice(schema, record + 4, entry_count * 8)?;
          let mut entries = Vec::new();
          for entry in 0..entry_count {
            entries.push((
              u32_at(schema, record + 4 + entry * 8)?,
              u32_at(schema, record + 8 + entry * 8)? as i32,
            ));
          }
          enums.insert(name, entries);
        }
        _ => return Err(invalid("unsupported PSO schema kind")),
      }
    }
    Ok(Self {
      data,
      root,
      blocks,
      structures,
      enums,
      sections,
    })
  }
}

fn slice(
  bytes: &[u8],
  offset: usize,
  length: usize,
) -> io::Result<&[u8]> {
  let end = offset.checked_add(length).ok_or_else(|| invalid("PSO range overflow"))?;
  bytes.get(offset..end).ok_or_else(|| invalid("truncated PSO data"))
}

fn u16_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u16> {
  Ok(u16::from_be_bytes(slice(bytes, offset, 2)?.try_into().unwrap()))
}

fn u32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_be_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn xml_hash(value: &str) -> io::Result<u32> {
  let value = value.trim();
  if value.is_empty() {
    return Ok(0);
  }
  if let Some(hex) = value.strip_prefix("hash_") {
    u32::from_str_radix(hex, 16).map_err(|_| invalid("invalid explicit PSO hash"))
  } else {
    Ok(jenk_hash(value))
  }
}

fn float_text(value: &str) -> io::Result<f32> {
  value.parse().map_err(|_| invalid("invalid PSO float"))
}

fn resolve(
  hash: u32,
  names: &HashMap<u32, String>,
) -> String {
  if hash == 0 {
    String::new()
  } else {
    names.get(&hash).cloned().unwrap_or_else(|| format!("hash_{hash:08X}"))
  }
}

fn value(
  node: &mut XmlElement,
  value: impl Into<String>,
) {
  node.attributes.insert("value".into(), value.into());
}

fn render(
  node: &XmlElement,
  depth: usize,
  xml: &mut String,
) {
  let indent = " ".repeat(depth);
  xml.push_str(&format!("{indent}<{}", node.name));
  let mut attributes = node.attributes.iter().collect::<Vec<_>>();
  attributes.sort();
  for (name, value) in attributes {
    xml.push_str(&format!(" {name}=\"{}\"", quick_xml::escape::escape(value)));
  }
  if node.children.is_empty() && node.text.is_empty() {
    xml.push_str(" />\n");
    return;
  }
  xml.push('>');
  if node.children.is_empty() {
    xml.push_str(&quick_xml::escape::escape(&node.text));
  } else {
    xml.push('\n');
    for child in &node.children {
      render(child, depth + 1, xml);
    }
    xml.push_str(&indent);
  }
  xml.push_str(&format!("</{}>\n", node.name));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::gamefile::meta_resource::jenk_hash;

  #[test]
  fn parses_four_vanilla_pso_ymaps() {
    for name in ["cs1_railwyc", "cs1_railwyc_long_0", "id2_17", "id2_17_strm_0"] {
      let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("asset/vanilla/ymap/{name}.ymap"));
      let resource = PsoResource::parse(&std::fs::read(path).unwrap()).unwrap();
      assert_eq!(resource.blocks[resource.root - 1].name, jenk_hash("CMapData"));
      assert!(resource.structures.contains_key(&jenk_hash("CMapData")));
      let xml = resource.to_xml(&HashMap::new()).unwrap();
      assert!(xml.contains("<CMapData>"));
      assert!(xml.contains(&format!("<name>{name}</name>")));
      assert!(!xml.contains("<error"));
      let rebuilt = resource.rebuild_xml(&xml).unwrap();
      assert_eq!(PsoResource::parse(&rebuilt).unwrap().to_xml(&HashMap::new()).unwrap(), xml);
    }
  }

  #[test]
  fn rejects_invalid_sections() {
    for bytes in [b"PSIN".as_slice(), b"PSIN\0\0\0\0", b"PSIN\xff\xff\xff\xff"] {
      assert!(PsoResource::parse(bytes).is_err());
    }
  }

  #[test]
  fn template_rebuild_edits_flags_and_rejects_shape_changes() {
    let path =
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla/ymap/cs1_railwyc.ymap");
    let bytes = std::fs::read(path).unwrap();
    let xml = PsoResource::parse(&bytes).unwrap().to_xml(&HashMap::new()).unwrap();
    let edited = xml.replacen("<flags value=\"2\"", "<flags value=\"3\"", 1);
    assert_ne!(edited, xml);
    let binary = PsoResource::parse(&bytes).unwrap().rebuild_xml(&edited).unwrap();
    assert_eq!(PsoResource::parse(&binary).unwrap().to_xml(&HashMap::new()).unwrap(), edited);
    let changed_shape = xml.replacen("</entities>", "<Item /></entities>", 1);
    let error = PsoResource::parse(&bytes).unwrap().rebuild_xml(&changed_shape).unwrap_err();
    assert!(error.to_string().contains("array length"));
  }
}
