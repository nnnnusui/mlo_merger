use super::*;

impl PsoResource {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_invalid_sections() {
    for bytes in [b"PSIN".as_slice(), b"PSIN\0\0\0\0", b"PSIN\xff\xff\xff\xff"] {
      assert!(PsoResource::parse(bytes).is_err());
    }
  }
}
