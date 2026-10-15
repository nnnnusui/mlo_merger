mod entity_parent;
mod ymap_parent;

pub(crate) use entity_parent::EntityParentRef;
pub(crate) use entity_parent::{
  entity_parent_ref, is_local_parent, runtime_entities, runtime_entity_indices,
};
pub use ymap_parent::YmapParent;
