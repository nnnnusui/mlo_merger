use super::*;

impl PsoResource {
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

  pub(super) fn read_node(
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

  pub(super) fn string(
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

  pub(super) fn read_array(
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
    crate::core::format::gamefile::xml_tree::write_text_content(xml, depth, &node.text);
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

  #[test]
  fn multiline_leaf_text_uses_shared_indentation() {
    let node = XmlElement {
      name: "Data".into(),
      text: "1 2 3\n4 5 6".into(),
      ..XmlElement::default()
    };
    let mut xml = String::new();
    render(&node, 2, &mut xml);
    assert_eq!(xml, "  <Data>\n   1 2 3\n   4 5 6\n  </Data>\n");
  }
}
