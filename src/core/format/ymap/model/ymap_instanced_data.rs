use structdiff::{Difference, StructDiff};

/// Instanced data definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct YmapInstancedData {
  pub imap_link: String,
  pub prop_instance_list: Vec<String>,
  pub grass_instance_list: Vec<String>,
}

impl Default for YmapInstancedData {
  fn default() -> Self {
    Self {
      imap_link: String::new(),
      prop_instance_list: Vec::new(),
      grass_instance_list: Vec::new(),
    }
  }
}
