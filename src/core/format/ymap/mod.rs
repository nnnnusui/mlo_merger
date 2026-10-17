pub mod binary;
pub mod compare;
pub mod model;
mod reference;
pub mod relation;

pub(crate) use reference::reference_hash;

#[cfg(test)]
mod test;
