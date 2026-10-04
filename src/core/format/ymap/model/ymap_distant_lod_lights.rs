/// Single distant LOD light entry in YMAP
#[derive(Debug, Clone, PartialEq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct YmapDistantLodLight {
  pub position: String,
  pub rgbi: String,
}

/// Distant LOD lights structure of array in YMAP
#[derive(Debug, Clone, PartialEq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct YmapDistantLodLights {
  pub items: Vec<YmapDistantLodLight>,
  pub num_street_lights: u32,
  pub category: u32,
}
