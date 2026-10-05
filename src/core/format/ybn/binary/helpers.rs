//! Checked binary record and pointer helpers.

use super::super::*;

pub(in crate::core::format::ybn) fn read_address_table(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
  stride: usize,
) -> io::Result<&[u8]> {
  if count == 0 {
    return Ok(&[]);
  }
  if pointer == 0 {
    return Err(invalid("YBN non-empty table has a null pointer"));
  }
  let size = count.checked_mul(stride).ok_or_else(|| invalid("YBN table size overflows"))?;
  resource.read_address(pointer, size)
}
pub(in crate::core::format::ybn) fn read_optional_address_table(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
  stride: usize,
) -> io::Result<Vec<u8>> {
  if pointer == 0 || count == 0 {
    return Ok(Vec::new());
  }
  let size = count.checked_mul(stride).ok_or_else(|| invalid("YBN table size overflows"))?;
  let address =
    u32::try_from(pointer).map_err(|_| invalid("YBN optional table pointer is invalid"))?;
  let (data, offset) = match address & 0xf000_0000 {
    0x5000_0000 => (&resource.system_data, (address & 0x0fff_ffff) as usize),
    0x6000_0000 => (&resource.graphics_data, (address & 0x0fff_ffff) as usize),
    _ => return Err(invalid("YBN optional table pointer uses an unknown page region")),
  };
  let available = if offset >= data.len() { 0 } else { size.min(data.len() - offset) };
  let mut result = vec![0; size];
  if available > 0 {
    result[..available].copy_from_slice(&data[offset..offset + available]);
  }
  Ok(result)
}
pub(in crate::core::format::ybn) fn read_pointer_array(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
) -> io::Result<Vec<u64>> {
  if count == 0 {
    return Ok(Vec::new());
  }
  let bytes = read_address_table(resource, pointer, count, 8)?;
  (0..count).map(|index| read_u64(bytes, index * 8)).collect()
}
pub(in crate::core::format::ybn) fn bound_type(value: u8) -> io::Result<String> {
  match value {
    255 => Ok("None".into()),
    0 => Ok("Sphere".into()),
    1 => Ok("Capsule".into()),
    3 => Ok("Box".into()),
    4 => Ok("Geometry".into()),
    8 => Ok("GeometryBVH".into()),
    10 => Ok("Composite".into()),
    12 => Ok("Disc".into()),
    13 => Ok("Cylinder".into()),
    15 => Ok("Cloth".into()),
    _ => Err(invalid(&format!("Native YBN does not support Bounds type id {value}"))),
  }
}
pub(in crate::core::format::ybn) fn size_for_bound(bound: &Bound) -> usize {
  match bound.kind.as_str() {
    "None" => 0,
    "Composite" => COMPOSITE_SIZE,
    "Geometry" => GEOMETRY_SIZE,
    "GeometryBVH" => GEOMETRY_BVH_SIZE,
    "Capsule" | "Disc" | "Cylinder" => 128,
    _ => BOUNDS_SIZE,
  }
}
pub(in crate::core::format::ybn) fn empty_bound(transform: Option<[f32; 16]>) -> Bound {
  Bound {
    kind: "None".into(),
    common: vec![0; BOUNDS_SIZE],
    extension: Vec::new(),
    geometry: None,
    children: Vec::new(),
    transform,
    composite_flags: [0; 2],
    transforms: Vec::new(),
    flags: Vec::new(),
  }
}
pub(in crate::core::format::ybn) fn read_u16(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u16> {
  Ok(u16::from_le_bytes(array(bytes, offset)?))
}
pub(in crate::core::format::ybn) fn read_i16(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i16> {
  Ok(i16::from_le_bytes(array(bytes, offset)?))
}
pub(in crate::core::format::ybn) fn read_u32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_le_bytes(array(bytes, offset)?))
}
pub(in crate::core::format::ybn) fn read_u64(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u64> {
  Ok(u64::from_le_bytes(array(bytes, offset)?))
}
pub(in crate::core::format::ybn) fn read_f32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<f32> {
  Ok(f32::from_le_bytes(array(bytes, offset)?))
}
pub(in crate::core::format::ybn) fn read_vec3(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[f32; 3]> {
  Ok([read_f32(bytes, offset)?, read_f32(bytes, offset + 4)?, read_f32(bytes, offset + 8)?])
}
pub(in crate::core::format::ybn) fn read_matrix(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[f32; 16]> {
  Ok([
    read_f32(bytes, offset)?,
    read_f32(bytes, offset + 4)?,
    read_f32(bytes, offset + 8)?,
    0.0,
    read_f32(bytes, offset + 16)?,
    read_f32(bytes, offset + 20)?,
    read_f32(bytes, offset + 24)?,
    0.0,
    read_f32(bytes, offset + 32)?,
    read_f32(bytes, offset + 36)?,
    read_f32(bytes, offset + 40)?,
    0.0,
    read_f32(bytes, offset + 48)?,
    read_f32(bytes, offset + 52)?,
    read_f32(bytes, offset + 56)?,
    1.0,
  ])
}
pub(in crate::core::format::ybn) fn write_matrix_record(
  bytes: &mut Vec<u8>,
  matrix: &[f32; 16],
) {
  let flags = [0u32, 1, 1, 0];
  for row in 0..4 {
    for column in 0..3 {
      bytes.extend_from_slice(&matrix[row * 4 + column].to_le_bytes());
    }
    bytes.extend_from_slice(&flags[row].to_le_bytes());
  }
}
pub(in crate::core::format::ybn) fn array<const N: usize>(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[u8; N]> {
  let end = offset.checked_add(N).ok_or_else(|| invalid("YBN read offset overflows"))?;
  bytes
    .get(offset..end)
    .ok_or_else(|| invalid("YBN record is truncated"))?
    .try_into()
    .map_err(|_| invalid("YBN record is truncated"))
}
pub(in crate::core::format::ybn) fn put_u16(
  bytes: &mut [u8],
  offset: usize,
  value: u16,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
pub(in crate::core::format::ybn) fn put_u32(
  bytes: &mut [u8],
  offset: usize,
  value: u32,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
pub(in crate::core::format::ybn) fn put_u64(
  bytes: &mut [u8],
  offset: usize,
  value: u64,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
pub(in crate::core::format::ybn) fn write_f32(
  bytes: &mut [u8],
  offset: usize,
  value: f32,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
pub(in crate::core::format::ybn) fn write_vec3(
  bytes: &mut [u8],
  offset: usize,
  value: [f32; 3],
) -> io::Result<()> {
  for (index, component) in value.iter().enumerate() {
    write_f32(bytes, offset + index * 4, *component)?;
  }
  Ok(())
}
pub(in crate::core::format::ybn) fn write(
  bytes: &mut [u8],
  offset: usize,
  value: &[u8],
) -> io::Result<()> {
  let end = offset.checked_add(value.len()).ok_or_else(|| invalid("YBN write offset overflows"))?;
  bytes
    .get_mut(offset..end)
    .ok_or_else(|| invalid("YBN write range is out of bounds"))?
    .copy_from_slice(value);
  Ok(())
}
pub(in crate::core::format::ybn) fn ensure_len(
  bytes: &mut Vec<u8>,
  length: usize,
) {
  if bytes.len() < length {
    bytes.resize(length, 0);
  }
}
pub(in crate::core::format::ybn) fn reserve(
  bytes: &mut Vec<u8>,
  length: usize,
) -> io::Result<usize> {
  let padding = (16 - bytes.len() % 16) % 16;
  bytes.resize(bytes.len() + padding, 0);
  let offset = bytes.len();
  let end = offset.checked_add(length).ok_or_else(|| invalid("YBN output size overflows"))?;
  bytes.resize(end, 0);
  Ok(offset)
}
pub(in crate::core::format::ybn) fn append(
  bytes: &mut Vec<u8>,
  value: &[u8],
) -> io::Result<usize> {
  let offset = reserve(bytes, value.len())?;
  bytes[offset..offset + value.len()].copy_from_slice(value);
  Ok(offset)
}
pub(in crate::core::format::ybn) fn address(offset: usize) -> io::Result<u64> {
  let offset = u32::try_from(offset).map_err(|_| invalid("YBN resource exceeds address space"))?;
  Ok(BASE + offset as u64)
}
pub(in crate::core::format::ybn) fn pointer_if_nonempty(
  offset: usize,
  length: usize,
) -> io::Result<u64> {
  if length == 0 { Ok(0) } else { address(offset) }
}
pub(in crate::core::format::ybn) fn page_count(flags: u32) -> usize {
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
pub(in crate::core::format::ybn) fn identity() -> [f32; 16] {
  [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}
pub(in crate::core::format::ybn) fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
