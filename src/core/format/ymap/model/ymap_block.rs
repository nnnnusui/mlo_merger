/// Block metadata in YMAP
#[derive(
  Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, struct_diff::Diff,
)]
#[diff(attr(cfg_attr(feature = "typescript", derive(specta::Type))))]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapBlock {
  pub version: u32,
  pub flags: u32,
  pub name: String,
  pub exported_by: String,
  pub owner: String,
  pub time: String,
}
