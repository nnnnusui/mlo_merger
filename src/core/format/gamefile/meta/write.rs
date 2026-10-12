use super::*;

impl MetaResource {
  /// Rebuilds this META resource as an RSC7 file with newly assigned page addresses.
  pub fn to_rsc7(
    &self,
    version: u32,
  ) -> io::Result<Rsc7Resource> {
    if self.root_block_index < 1 || self.root_block_index as usize > self.data_blocks.len() {
      return Err(invalid_data("META root block index is out of range"));
    }
    for count in [self.structures.len(), self.enums.len(), self.data_blocks.len()] {
      if count > i16::MAX as usize {
        return Err(invalid_data("META table exceeds the supported record count"));
      }
    }

    let mut system = vec![0u8; 0x70];
    let structure_table = reserve(&mut system, self.structures.len() * 32)?;
    let enum_table = reserve(&mut system, self.enums.len() * 24)?;
    let data_block_table = reserve(&mut system, self.data_blocks.len() * 16)?;

    for (index, structure) in self.structures.iter().enumerate() {
      if structure.entries.len() > i16::MAX as usize {
        return Err(invalid_data("META structure exceeds the supported field count"));
      }
      let entries_offset = append_records(&mut system, &structure.entries, 16)?;
      let record = structure_table + index * 32;
      write_u32(&mut system, record, structure.name_hash)?;
      write_u32(&mut system, record + 4, structure.structure_key)?;
      write_u32(&mut system, record + 8, structure.unknown_8)?;
      write_u32(&mut system, record + 12, structure.unknown_12)?;
      write_u64(
        &mut system,
        record + 16,
        address_if_nonempty(entries_offset, structure.entries.len()),
      )?;
      write_u32(&mut system, record + 24, structure.structure_size)?;
      write_i16(&mut system, record + 28, structure.unknown_28)?;
      write_i16(&mut system, record + 30, structure.entries.len() as i16)?;

      for (entry_index, entry) in structure.entries.iter().enumerate() {
        let offset = entries_offset + entry_index * 16;
        write_u32(&mut system, offset, entry.name_hash)?;
        write_u32(&mut system, offset + 4, entry.data_offset)?;
        system[offset + 8] = entry.data_type;
        system[offset + 9] = entry.unknown;
        write_i16(&mut system, offset + 10, entry.reference_type_index)?;
        write_u32(&mut system, offset + 12, entry.reference_key)?;
      }
    }

    for (index, enum_info) in self.enums.iter().enumerate() {
      let entries_offset = append_records(&mut system, &enum_info.entries, 8)?;
      let record = enum_table + index * 24;
      write_u32(&mut system, record, enum_info.name_hash)?;
      write_u32(&mut system, record + 4, enum_info.enum_key)?;
      write_u64(
        &mut system,
        record + 8,
        address_if_nonempty(entries_offset, enum_info.entries.len()),
      )?;
      write_i32(&mut system, record + 16, enum_info.entries.len() as i32)?;
      write_u32(&mut system, record + 20, enum_info.unknown_20)?;
      for (entry_index, entry) in enum_info.entries.iter().enumerate() {
        let offset = entries_offset + entry_index * 8;
        write_u32(&mut system, offset, entry.name_hash)?;
        write_i32(&mut system, offset + 4, entry.value)?;
      }
    }

    for (index, block) in self.data_blocks.iter().enumerate() {
      let data_offset = append_aligned(&mut system, &block.data)?;
      let record = data_block_table + index * 16;
      write_u32(&mut system, record, block.structure_name_hash)?;
      write_i32(&mut system, record + 4, block.data.len() as i32)?;
      write_u64(&mut system, record + 8, address_if_nonempty(data_offset, block.data.len()))?;
    }

    let name_pointer = match self.name.as_deref().filter(|name| !name.is_empty()) {
      Some(name) => {
        let mut bytes = name.as_bytes().to_vec();
        bytes.push(0);
        address(append_aligned(&mut system, &bytes)?)
      }
      None => 0,
    };

    let meta_end = system.len();
    let mut page_info_count = 1usize;
    let (pages_info_offset, system_page_count) = loop {
      system.truncate(meta_end);
      let offset = reserve(&mut system, 16 + page_info_count * 8)?;
      let probe = Rsc7Resource::from_pages(version, &system, &[])?;
      let actual_page_count = page_count(probe.system_flags).max(1);
      if actual_page_count <= page_info_count {
        break (offset, actual_page_count);
      }
      page_info_count = actual_page_count;
    };
    system[pages_info_offset + 8] = system_page_count as u8;
    system[pages_info_offset + 9] = 0;

    write_u32(&mut system, 0, 0x405b_c808)?;
    write_u32(&mut system, 4, 1)?;
    write_u64(&mut system, 8, address(pages_info_offset))?;

    let root = 16usize;
    write_u32(&mut system, root, 0x5052_4430)?;
    write_i16(&mut system, root + 4, 0x79)?;
    write_i32(&mut system, root + 12, self.root_block_index)?;
    write_u64(&mut system, root + 16, address_if_nonempty(structure_table, self.structures.len()))?;
    write_u64(&mut system, root + 24, address_if_nonempty(enum_table, self.enums.len()))?;
    write_u64(
      &mut system,
      root + 32,
      address_if_nonempty(data_block_table, self.data_blocks.len()),
    )?;
    write_u64(&mut system, root + 40, name_pointer)?;
    write_i16(&mut system, root + 56, self.structures.len() as i16)?;
    write_i16(&mut system, root + 58, self.enums.len() as i16)?;
    write_i16(&mut system, root + 60, self.data_blocks.len() as i16)?;

    Rsc7Resource::from_pages(version, &system, &[])
  }
}

fn page_count(flags: u32) -> usize {
  ((flags >> 27) & 1) as usize
    + ((flags >> 26) & 1) as usize
    + ((flags >> 25) & 1) as usize
    + ((flags >> 24) & 1) as usize
    + ((flags >> 17) & 0x7f) as usize
    + ((flags >> 11) & 0x3f) as usize
    + ((flags >> 7) & 0xf) as usize
    + ((flags >> 5) & 3) as usize
    + ((flags >> 4) & 1) as usize
}

fn append_records(
  data: &mut Vec<u8>,
  _records: &[impl Sized],
  record_size: usize,
) -> io::Result<usize> {
  reserve(data, _records.len() * record_size)
}

fn append_aligned(
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
  let end = offset.checked_add(length).ok_or_else(|| invalid_data("META layout size overflows"))?;
  data.resize(end, 0);
  Ok(offset)
}

fn address(offset: usize) -> u64 {
  0x5000_0000u64 + offset as u64
}

fn address_if_nonempty(
  offset: usize,
  length: usize,
) -> u64 {
  if length == 0 { 0 } else { address(offset) }
}

fn write_i16(
  bytes: &mut [u8],
  offset: usize,
  value: i16,
) -> io::Result<()> {
  write_bytes(bytes, offset, &value.to_le_bytes())
}

fn write_i32(
  bytes: &mut [u8],
  offset: usize,
  value: i32,
) -> io::Result<()> {
  write_bytes(bytes, offset, &value.to_le_bytes())
}

fn write_u32(
  bytes: &mut [u8],
  offset: usize,
  value: u32,
) -> io::Result<()> {
  write_bytes(bytes, offset, &value.to_le_bytes())
}

fn write_u64(
  bytes: &mut [u8],
  offset: usize,
  value: u64,
) -> io::Result<()> {
  write_bytes(bytes, offset, &value.to_le_bytes())
}

fn write_bytes(
  bytes: &mut [u8],
  offset: usize,
  value: &[u8],
) -> io::Result<()> {
  let end =
    offset.checked_add(value.len()).ok_or_else(|| invalid_data("META write offset overflows"))?;
  bytes
    .get_mut(offset..end)
    .ok_or_else(|| invalid_data("META write is out of bounds"))?
    .copy_from_slice(value);
  Ok(())
}

#[cfg(test)]
mod tests {
  #[test]
  fn page_info_counts_pages_not_size_units() {
    assert_eq!(super::page_count(0x0402_0000), 2);
    assert_eq!(super::page_count(0x0400_0004), 1);
    assert_eq!(super::page_count(0x0800_0000), 1);
    assert_eq!(super::page_count(1 << 4), 1);
  }
}
