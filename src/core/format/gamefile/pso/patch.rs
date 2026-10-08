use super::*;

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

  pub(super) fn patch_node(
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

  pub(super) fn put(
    &mut self,
    offset: usize,
    bytes: &[u8],
  ) -> io::Result<()> {
    slice(&self.data, offset, bytes.len())?;
    self.data[offset..offset + bytes.len()].copy_from_slice(bytes);
    Ok(())
  }
}
