use std::collections::HashMap;

use super::StreamFileRef;
use crate::core::format::ymap::model::Ymap;

/// A declared parent YMAP.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub enum YmapParentRef {
  /// Found in the child's resource or in vanilla.
  Resolved(StreamFileRef),
  /// The parent's reference hash, matching no YMAP in the child's resource or vanilla.
  Unresolved(u32),
}

/// Per-YMAP data derived once for comparisons.
#[derive(Debug, Clone, PartialEq)]
pub struct YmapComparisonData {
  /// The declared parent YMAP, or `None` when the YMAP declares no parent.
  pub parent_ref: Option<YmapParentRef>,
  /// Indices into `Ymap::entities` keyed by GUID, in entity order.
  pub entity_indices_by_guid: HashMap<u32, Vec<usize>>,
}

impl YmapComparisonData {
  pub fn new(
    ymap: &Ymap,
    parent_ref: Option<YmapParentRef>,
  ) -> Self {
    let mut entity_indices_by_guid = HashMap::<u32, Vec<usize>>::new();
    for (index, entity) in ymap.entities.iter().enumerate() {
      entity_indices_by_guid.entry(entity.guid).or_default().push(index);
    }
    Self {
      entity_indices_by_guid,
      parent_ref,
    }
  }

  /// Whether exactly one entity in the YMAP carries `guid`.
  pub fn is_unique_guid(
    &self,
    guid: u32,
  ) -> bool {
    self.entity_indices_by_guid.get(&guid).is_some_and(|indices| indices.len() == 1)
  }
}
