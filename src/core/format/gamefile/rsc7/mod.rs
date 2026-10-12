//! RSC7 envelopes, page layouts and virtual-address access.

const RSC7_MAGIC: u32 = u32::from_le_bytes(*b"RSC7");
const RSC7_HEADER_SIZE: usize = 16;

mod helpers;
mod model;
mod read;
mod write;

pub use model::Rsc7Resource;

#[cfg(test)]
mod tests;
