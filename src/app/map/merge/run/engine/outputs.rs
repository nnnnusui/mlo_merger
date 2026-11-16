//! Deciding which planned YMAPs are written.

use super::*;

impl<'a> MergeRun<'a> {
  /// Restores the entities' GUIDs and decides which planned YMAPs are written: the merged ones
  /// that differ from what they replace, and the kept ones whose parent references or child
  /// counts were repaired.
  pub(super) fn decide_outputs(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let mut repaired = 0usize;
    for output in self.planned.values_mut() {
      for entity in output.entities_mut() {
        entity.guid = self.references.original_guid(entity.guid)?;
      }
      if let Some(entities) = &output.clone_entities {
        let changed = entities.iter().zip(&output.original.entities).any(|(after, before)| {
          after.parent_index != before.parent_index
            || after.flags != before.flags
            || after.lod_level != before.lod_level
            || after.num_children != before.num_children
        });
        if changed {
          output.rebuild = true;
          repaired += 1;
        }
      }
      if output.rebuild && output.copy_target.is_none() {
        let name = output.name.to_ascii_lowercase();
        // An added YMAP is compared with its source, decoded without the hash names its diff was
        // taken with, so a difference is checked on the encoded files.
        output.rebuild = self.merge.replacing.contains(&name)
          || (output.differs_from_original()
            && !(self.merge.added.contains(&name) && output.encodes_like_original()?));
      }
    }
    tracing::info!(
      "Repaired parent references; promoted {repaired} clone/vanilla child YMAPs for rebuild"
    );
    Ok(())
  }
}
