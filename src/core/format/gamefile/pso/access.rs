//! [`StructReader`] over PSO structures and their embedded schemas.

use super::*;
use crate::core::format::gamefile::struct_reader::{Scalar, StructReader};

/// One structure instance inside a PSO, read through its embedded schema.
///
/// Fields are looked up by name (or `hash_XXXXXXXX`), so callers stay independent of the
/// per-file layout that PSO schemas allow.
#[derive(Clone, Copy)]
pub struct PsoStruct<'a> {
  resource: &'a PsoResource,
  names: &'a HashNames<'a>,
  schema: &'a Schema,
  kind: u32,
  block: usize,
  offset: usize,
  address: usize,
  depth: usize,
}

impl PsoResource {
  /// Returns the root structure, resolving hashes through `names` (see [`Self::names`]).
  pub fn root_struct<'a>(
    &'a self,
    names: &'a HashNames<'a>,
  ) -> io::Result<PsoStruct<'a>> {
    let kind = self
      .blocks
      .get(self.root.checked_sub(1).ok_or_else(|| invalid("PSO has no root block"))?)
      .ok_or_else(|| invalid("PSO root block is out of range"))?
      .name;
    PsoStruct::new(self, names, self.root, 0, kind, 0)
  }
}

impl<'a> PsoStruct<'a> {
  fn new(
    resource: &'a PsoResource,
    names: &'a HashNames<'a>,
    block: usize,
    offset: usize,
    kind: u32,
    depth: usize,
  ) -> io::Result<Self> {
    if depth > 64 {
      return Err(invalid("PSO reference nesting exceeds 64 levels"));
    }
    let schema = resource
      .structures
      .get(&kind)
      .ok_or_else(|| invalid("PSO embedded structure schema missing"))?;
    let address = resource.address(block, offset, schema.size)?;
    Ok(Self {
      resource,
      names,
      schema,
      kind,
      block,
      offset,
      address,
      depth,
    })
  }

  fn nested(
    &self,
    block: usize,
    offset: usize,
    kind: u32,
  ) -> io::Result<PsoStruct<'a>> {
    Self::new(self.resource, self.names, block, offset, kind, self.depth + 1)
  }

  fn field(
    &self,
    name: &str,
  ) -> io::Result<Option<&'a Field>> {
    let hash = xml_hash(name)?;
    Ok(self.schema.fields.iter().find(|field| field.name == hash && field.name != 0x100))
  }

  fn location(
    &self,
    field: &Field,
  ) -> io::Result<usize> {
    self.address.checked_add(field.offset).ok_or_else(|| invalid("PSO field offset overflow"))
  }

  fn array(
    &self,
    field: &Field,
    name: &str,
  ) -> io::Result<(&'a Field, usize, usize, usize)> {
    if field.kind != 13 {
      return Err(self.context(name, invalid("PSO field is not an array")));
    }
    self
      .resource
      .array_layout(field, self.schema, (self.block, self.offset))
      .map_err(|error| self.context(name, error))
  }

  fn read_scalar(
    &self,
    field: &Field,
  ) -> io::Result<Scalar> {
    let location = self.location(field)?;
    self.resource.scalar_at(field, location, Some(self.schema), self.names)
  }

  fn block_name(
    &self,
    block: usize,
  ) -> io::Result<u32> {
    Ok(
      self
        .resource
        .blocks
        .get(block - 1)
        .ok_or_else(|| invalid("PSO structure pointer out of range"))?
        .name,
    )
  }
}

impl StructReader for PsoStruct<'_> {
  /// The resolved structure name, e.g. `CEntityDef`.
  fn type_name(&self) -> String {
    resolve(self.kind, self.names)
  }

  /// Reads a scalar field, or `None` when the schema has no such field.
  fn scalar(
    &self,
    name: &str,
  ) -> io::Result<Option<Scalar>> {
    let Some(field) = self.field(name)? else {
      return Ok(None);
    };
    self.read_scalar(field).map(Some).map_err(|error| self.context(name, error))
  }

  /// Reads a float vector field (`xy`, `xyz` or `xyzw`), or `None` when it is absent.
  fn vector(
    &self,
    name: &str,
  ) -> io::Result<Option<Vec<f32>>> {
    let Some(field) = self.field(name)? else {
      return Ok(None);
    };
    let count = match field.kind {
      8 => 2,
      9 | 20 | 21 => 3,
      10 => 4,
      kind => {
        return Err(self.context(name, invalid(&format!("PSO field type {kind} is not a vector"))));
      }
    };
    let location = self.location(field)?;
    (0..count)
      .map(|index| u32_at(&self.resource.data, location + index * 4).map(f32::from_bits))
      .collect::<io::Result<Vec<_>>>()
      .map(Some)
  }

  /// Reads an embedded or pointed-to structure field; `None` when absent or a null pointer.
  fn child(
    &self,
    name: &str,
  ) -> io::Result<Option<Self>> {
    let Some(field) = self.field(name)? else {
      return Ok(None);
    };
    if field.kind != 12 {
      return Err(self.context(name, invalid("PSO field is not a structure")));
    }
    match field.subtype {
      0 => self.nested(self.block, self.offset + field.offset, field.reference).map(Some),
      3 | 4 => {
        let (target, position) = self.resource.pointer(self.location(field)?)?;
        if target == 0 {
          return Ok(None);
        }
        let kind = if field.reference == 0 { self.block_name(target)? } else { field.reference };
        self.nested(target, position, kind).map(Some)
      }
      _ => Err(self.context(name, invalid("unsupported PSO structure subtype"))),
    }
  }

  /// Reads a structure array field; absent fields read as empty.
  fn children(
    &self,
    name: &str,
  ) -> io::Result<Vec<Self>> {
    let Some(field) = self.field(name)? else {
      return Ok(Vec::new());
    };
    let (item, target, position, count) = self.array(field, name)?;
    if count == 0 {
      return Ok(Vec::new());
    }
    if item.kind != 12 {
      return Err(self.context(name, invalid("PSO array does not hold structures")));
    }
    (0..count)
      .map(|element| match item.subtype {
        0 => {
          let size = self
            .resource
            .structures
            .get(&item.reference)
            .ok_or_else(|| invalid("PSO array structure missing"))?
            .size;
          self.nested(target, position + element * size, item.reference)
        }
        3 => {
          let pointer = self.resource.address(target, position + element * 8, 8)?;
          let (target, position) = self.resource.pointer(pointer)?;
          if target == 0 {
            return Err(self.context(name, invalid("PSO structure array holds a null pointer")));
          }
          self.nested(target, position, self.block_name(target)?)
        }
        _ => Err(self.context(name, invalid("unsupported PSO structure array subtype"))),
      })
      .collect()
  }

  /// Reads a primitive or hash array field; absent fields read as empty.
  fn values(
    &self,
    name: &str,
  ) -> io::Result<Vec<Scalar>> {
    let Some(field) = self.field(name)? else {
      return Ok(Vec::new());
    };
    let (item, target, position, count) = self.array(field, name)?;
    if count == 0 {
      return Ok(Vec::new());
    }
    let width = match item.kind {
      0..=2 => 1,
      3 | 4 => 2,
      5..=7 => 4,
      11 if matches!(item.subtype, 7 | 8) => 4,
      kind => {
        return Err(
          self.context(name, invalid(&format!("unsupported PSO array element type {kind}"))),
        );
      }
    };
    (0..count)
      .map(|element| {
        let address = self.resource.address(target, position + element * width, width)?;
        self.resource.scalar_at(item, address, None, self.names)
      })
      .collect()
  }
}

impl PsoResource {
  /// Reads one scalar of `field`'s kind at `address`; `schema` resolves named flag lists.
  fn scalar_at(
    &self,
    field: &Field,
    address: usize,
    schema: Option<&Schema>,
    names: &HashNames,
  ) -> io::Result<Scalar> {
    let byte = || slice(&self.data, address, 1).map(|bytes| bytes[0]);
    Ok(match field.kind {
      0 => Scalar::Bool(byte()? != 0),
      1 => Scalar::Int(byte()? as i8 as i64),
      2 => Scalar::Int(byte()? as i64),
      3 | 30 => Scalar::Int(u16_at(&self.data, address)? as i16 as i64),
      4 => Scalar::Int(u16_at(&self.data, address)? as i64),
      5 => Scalar::Int(u32_at(&self.data, address)? as i32 as i64),
      6 => match field.subtype {
        0 => Scalar::Int(u32_at(&self.data, address)? as i32 as i64),
        1 => Scalar::Int(u32_at(&self.data, address)? as i64),
        _ => return Err(invalid("unsupported PSO integer subtype")),
      },
      7 => Scalar::Float(f32::from_bits(u32_at(&self.data, address)?)),
      11 => Scalar::Text(self.string(field, address, names)?),
      14 => {
        let number = match field.subtype {
          0 => u32_at(&self.data, address)? as i32,
          2 => byte()? as i32,
          _ => return Err(invalid("unsupported PSO enum subtype")),
        };
        let enum_values =
          self.enums.get(&field.reference).ok_or_else(|| invalid("PSO enum schema missing"))?;
        Scalar::Text(resolve(
          enum_values
            .iter()
            .find(|(_, value)| *value == number)
            .map(|(name, _)| *name)
            .unwrap_or(0),
          names,
        ))
      }
      15 => {
        let number = match field.subtype {
          0 => u32_at(&self.data, address)?,
          1 => u16_at(&self.data, address)? as u32,
          2 => byte()? as u32,
          _ => return Err(invalid("unsupported PSO flags subtype")),
        };
        let flags = schema
          .and_then(|schema| schema.fields.get((field.reference & 0xfff) as usize))
          .filter(|info| info.name == 0x100)
          .and_then(|info| self.enums.get(&info.reference));
        match flags {
          Some(flags) => Scalar::Text(
            flags
              .iter()
              .filter(|(_, bit)| *bit >= 0 && *bit < 32 && number & (1 << *bit) != 0)
              .map(|(name, _)| resolve(*name, names))
              .collect::<Vec<_>>()
              .join(" "),
          ),
          None => Scalar::Int(number as i64),
        }
      }
      32 => {
        let value =
          u32_at(&self.data, address)? as u64 | ((u32_at(&self.data, address + 4)? as u64) << 32);
        Scalar::Int(
          i64::try_from(value).map_err(|_| invalid("PSO 64-bit value exceeds i64 range"))?,
        )
      }
      kind => return Err(invalid(&format!("PSO field type {kind} is not a scalar"))),
    })
  }
}
