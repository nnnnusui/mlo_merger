/// Block metadata in YMAP
#[derive(Debug, Clone, PartialEq)]
pub struct YmapBlock {
  pub version: u32,
  pub flags: u32,
  pub name: String,
  pub exported_by: String,
  pub owner: String,
  pub time: String,
}
