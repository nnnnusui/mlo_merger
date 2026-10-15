use std::collections::HashMap;

/// Hash-to-name lookup for one resource, layering its embedded names over a borrowed shared
/// dictionary without copying it.
///
/// Lookup order is shared, then embedded, then built-in names, matching the precedence of
/// extending a single map with built-in, embedded and shared names in that order.
pub struct HashNames<'a> {
  shared: &'a HashMap<u32, String>,
  embedded: HashMap<u32, String>,
  known: &'static HashMap<u32, String>,
}

impl<'a> HashNames<'a> {
  pub(crate) fn new(
    shared: &'a HashMap<u32, String>,
    embedded: HashMap<u32, String>,
    known: &'static HashMap<u32, String>,
  ) -> Self {
    Self {
      shared,
      embedded,
      known,
    }
  }

  pub fn get(
    &self,
    hash: &u32,
  ) -> Option<&String> {
    self.shared.get(hash).or_else(|| self.embedded.get(hash)).or_else(|| self.known.get(hash))
  }
}
