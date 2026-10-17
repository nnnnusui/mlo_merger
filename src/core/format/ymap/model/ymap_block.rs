use structdiff::{Difference, StructDiff};

/// Block metadata in YMAP
#[derive(
  Clone,
  Debug,
  Default,
  Difference,
  PartialEq,
  serde::Serialize,
  serde::Deserialize,
  struct_diff::Diff,
)]
#[difference(expose)]
#[diff(attr(cfg_attr(feature = "typescript", derive(specta::Type))))]
pub struct YmapBlock {
  pub version: u32,
  pub flags: u32,
  pub name: String,
  pub exported_by: String,
  pub owner: String,
  pub time: String,
}
