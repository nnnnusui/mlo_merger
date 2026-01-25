use std::io::{Read, Write};

/// Represents a data block of the system segment in a resource file.
pub trait ResourceSystemBlock: ResourceBlock {
  /// Gets the position of the data block.
  fn file_position(&self) -> i64;

  /// Sets the position of the data block.
  /// When set, this should also update the positions of all parts.
  fn set_file_position(&mut self, position: i64);

  /// Gets the length of the data block.
  fn block_length(&self) -> i64;

  /// Gets the length of the data block for Gen9 (GTA V Enhanced).
  /// By default, this returns the same value as block_length.
  fn block_length_gen9(&self) -> i64 {
    self.block_length()
  }

  /// Reads the data block.
  fn read<R: Read>(
    &mut self,
    reader: &mut R,
    parameters: &[&dyn std::any::Any],
  ) -> std::io::Result<()>;

  /// Writes the data block.
  fn write<W: Write>(
    &self,
    writer: &mut W,
    parameters: &[&dyn std::any::Any],
  ) -> std::io::Result<()>;

  /// Returns a list of data blocks that are part of this block.
  /// Each tuple contains the offset and a reference to the block.
  fn get_parts(&self) -> Vec<(i64, &dyn ResourceBlock)> {
    Vec::new()
  }

  /// Returns a mutable list of data blocks that are part of this block.
  fn get_parts_mut(&mut self) -> Vec<(i64, &mut dyn ResourceBlock)> {
    Vec::new()
  }

  /// Returns a list of data blocks that are referenced by this block.
  fn get_references(&self) -> Vec<&dyn ResourceBlock> {
    Vec::new()
  }

  /// Returns a mutable list of data blocks that are referenced by this block.
  fn get_references_mut(&mut self) -> Vec<&mut dyn ResourceBlock> {
    Vec::new()
  }
}

/// Base trait for resource blocks.
pub trait ResourceBlock {
  /// Gets the position of the block in the file.
  fn file_position(&self) -> i64;

  /// Sets the position of the block in the file.
  fn set_file_position(&mut self, position: i64);
}

/// Base implementation for a resource system block with position tracking.
#[derive(Debug, Clone)]
pub struct BaseResourceSystemBlock {
  position: i64,
}

impl Default for BaseResourceSystemBlock {
  fn default() -> Self {
    Self { position: 0 }
  }
}

impl BaseResourceSystemBlock {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn with_position(position: i64) -> Self {
    Self { position }
  }

  /// Gets the current position.
  pub fn position(&self) -> i64 {
    self.position
  }

  /// Sets the position and updates all parts recursively.
  pub fn set_position_with_parts<T: ResourceSystemBlock + ?Sized>(
    &mut self,
    position: i64,
    block: &mut T,
  ) {
    self.position = position;

    // Update positions of all parts
    for (offset, part) in block.get_parts_mut() {
      part.set_file_position(position + offset);
    }
  }
}

impl ResourceBlock for BaseResourceSystemBlock {
  fn file_position(&self) -> i64 {
    self.position
  }

  fn set_file_position(&mut self, position: i64) {
    self.position = position;
  }
}
