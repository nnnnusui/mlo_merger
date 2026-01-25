/// Distant LOD lights structure of array in YMAP
#[derive(Debug, Clone, PartialEq)]
pub struct YmapDistantLodLightsSoa {
  pub position: Vec<String>,
  pub rgbi: Vec<String>,
  pub num_street_lights: u32,
  pub category: u32,
}
