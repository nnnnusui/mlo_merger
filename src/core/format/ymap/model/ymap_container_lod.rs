/// Container LOD definition in YMAP (`rage__fwContainerLodDef`).
///
/// CodeWalker reads no relationship from these and drops them on save; what `parent_index`
/// refers to is unknown.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct YmapContainerLod {
  pub name: String,
  pub parent_index: u32,
}
