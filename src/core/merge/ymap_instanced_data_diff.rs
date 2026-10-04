use std::collections::{HashMap, HashSet};

use crate::core::format::ymap::model::{GrassInstance, GrassInstanceBatch, YmapInstancedData};

use super::ymap_metadata_diff::{ReferenceListDiff, merge_reference, reference_hash};

#[derive(Default)]
/// Vanilla-relative instance changes with batch-local packed coordinates.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct YmapInstancedDataDiff {
  imap_link: Option<String>,
  props: ReferenceListDiff,
  removed_batches: HashSet<BatchKey>,
  batches: Vec<BatchDiff>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Metadata defining the coordinate system and rendering settings of a batch.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct BatchKey {
  archetype: u32,
  bounds: [u32; 8],
  scale: [u32; 3],
  lod: [u32; 3],
  terrain: u32,
}

impl BatchKey {
  /// Identifies a batch independently of its name/hash spelling and instances.
  pub(super) fn from_batch(batch: &GrassInstanceBatch) -> Self {
    let min = &batch.batch_aabb.min;
    let max = &batch.batch_aabb.max;
    Self {
      archetype: reference_hash(&batch.archetype_name),
      bounds: [min.x, min.y, min.z, min.w, max.x, max.y, max.z, max.w].map(float_key),
      scale: [batch.scale_range.x, batch.scale_range.y, batch.scale_range.z].map(float_key),
      lod: [batch.lod_dist, batch.lod_fade_start_dist, batch.lod_inst_fade_range].map(float_key),
      terrain: batch.orient_to_terrain,
    }
  }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BatchDiff {
  key: BatchKey,
  template: GrassInstanceBatch,
  removed: HashSet<Vec<u32>>,
  changed: Vec<GrassInstance>,
}

fn float_key(value: f32) -> u32 {
  if value == 0.0 { 0 } else { value.to_bits() }
}

fn position_key(instance: &GrassInstance) -> Vec<u32> {
  instance.position.iter().copied().map(float_key).collect()
}

impl YmapInstancedDataDiff {
  /// Extracts link, prop-reference, batch, and instance changes from vanilla.
  pub(super) fn extract_from(
    vanilla: &YmapInstancedData,
    modded: &YmapInstancedData,
  ) -> Self {
    let original = vanilla
      .grass_instance_list
      .iter()
      .map(|batch| (BatchKey::from_batch(batch), batch))
      .collect::<HashMap<_, _>>();
    let modified =
      modded.grass_instance_list.iter().map(BatchKey::from_batch).collect::<HashSet<_>>();
    let removed_batches =
      original.keys().filter(|key| !modified.contains(*key)).cloned().collect::<HashSet<_>>();
    let mut batches = Vec::new();
    for batch in &modded.grass_instance_list {
      let key = BatchKey::from_batch(batch);
      let before = original.get(&key).map(|batch| batch.instances.as_slice()).unwrap_or_default();
      let original_instances =
        before.iter().map(|instance| (position_key(instance), instance)).collect::<HashMap<_, _>>();
      let modified_positions = batch.instances.iter().map(position_key).collect::<HashSet<_>>();
      let removed = original_instances
        .keys()
        .filter(|key| !modified_positions.contains(*key))
        .cloned()
        .collect::<HashSet<_>>();
      let changed = batch
        .instances
        .iter()
        .filter(|instance| {
          original_instances
            .get(&position_key(instance))
            .is_none_or(|before| **before != **instance)
        })
        .cloned()
        .collect::<Vec<_>>();
      if !original.contains_key(&key) || !removed.is_empty() || !changed.is_empty() {
        let mut template = batch.clone();
        template.instances.clear();
        batches.push(BatchDiff {
          key,
          template,
          removed,
          changed,
        });
      }
    }
    log::info!(
      "    Found {} grass batch changes and {} removed batches",
      batches.len(),
      removed_batches.len()
    );
    Self {
      imap_link: (reference_hash(&vanilla.imap_link) != reference_hash(&modded.imap_link))
        .then(|| modded.imap_link.clone()),
      props: ReferenceListDiff::extract_from(
        &vanilla.prop_instance_list,
        &modded.prop_instance_list,
      ),
      removed_batches,
      batches,
    }
  }

  /// Accumulates changes from another mod without restoring deleted instances.
  pub(super) fn merge(
    &mut self,
    other: Self,
  ) {
    merge_reference(&mut self.imap_link, other.imap_link, "ImapLink");
    self.props.merge(other.props);
    self.removed_batches.extend(other.removed_batches);
    self.batches.extend(other.batches);
  }

  /// Applies removals before additions and first-wins instance modifications.
  pub(super) fn apply_to(
    self,
    output: &mut YmapInstancedData,
  ) {
    if let Some(link) = self.imap_link {
      output.imap_link = link;
    }
    self.props.apply_to(&mut output.prop_instance_list);
    output
      .grass_instance_list
      .retain(|batch| !self.removed_batches.contains(&BatchKey::from_batch(batch)));
    let mut grouped = Vec::<BatchDiff>::new();
    let mut groups = HashMap::<BatchKey, usize>::new();
    for batch in self.batches {
      if self.removed_batches.contains(&batch.key) {
        continue;
      }
      if let Some(&index) = groups.get(&batch.key) {
        grouped[index].removed.extend(batch.removed);
        grouped[index].changed.extend(batch.changed);
      } else {
        groups.insert(batch.key.clone(), grouped.len());
        grouped.push(batch);
      }
    }
    let mut indices = output
      .grass_instance_list
      .iter()
      .enumerate()
      .map(|(index, batch)| (BatchKey::from_batch(batch), index))
      .collect::<HashMap<_, _>>();
    for diff in grouped {
      let index = *indices.entry(diff.key).or_insert_with(|| {
        let index = output.grass_instance_list.len();
        output.grass_instance_list.push(diff.template);
        index
      });
      let batch = &mut output.grass_instance_list[index];
      batch.instances.retain(|instance| !diff.removed.contains(&position_key(instance)));
      let mut positions = batch
        .instances
        .iter()
        .enumerate()
        .map(|(index, instance)| (position_key(instance), index))
        .collect::<HashMap<_, _>>();
      let mut applied = HashSet::new();
      for instance in diff.changed {
        let key = position_key(&instance);
        if diff.removed.contains(&key) {
          continue;
        }
        if !applied.insert(key.clone()) {
          if batch.instances[positions[&key]] != instance {
            log::warn!(
              "    Conflicting grass instance at {:?}: keeping first change",
              instance.position
            );
          }
          continue;
        }
        if let Some(&index) = positions.get(&key) {
          batch.instances[index] = instance;
        } else {
          positions.insert(key, batch.instances.len());
          batch.instances.push(instance);
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::ymap::model::{BoundingBox, Vector3, Vector4};

  fn sample(positions: &[f32]) -> YmapInstancedData {
    YmapInstancedData {
      grass_instance_list: vec![GrassInstanceBatch {
        batch_aabb: BoundingBox {
          min: Vector4 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
          },
          max: Vector4 {
            x: 10.0,
            y: 10.0,
            z: 10.0,
            w: 0.0,
          },
        },
        scale_range: Vector3 {
          x: 1.0,
          y: 1.0,
          z: 1.0,
        },
        archetype_name: "grass".into(),
        lod_dist: 40.0,
        lod_fade_start_dist: 20.0,
        lod_inst_fade_range: 1.0,
        orient_to_terrain: 1,
        instances: positions
          .iter()
          .map(|position| GrassInstance {
            position: vec![*position, 0.0, 0.0],
            normal_x: 100,
            normal_y: 100,
            color: vec![70, 80, 40],
            scale: 100,
            ao: 255,
            pad: vec![0; 3],
          })
          .collect(),
      }],
      ..YmapInstancedData::default()
    }
  }

  #[test]
  fn merges_independent_grass_deletions_and_additions() {
    let original = sample(&[1.0, 2.0, 3.0]);
    let mut diff = YmapInstancedDataDiff::extract_from(&original, &sample(&[2.0, 3.0, 4.0]));
    diff.merge(YmapInstancedDataDiff::extract_from(&original, &sample(&[1.0, 3.0, 4.0, 5.0])));
    let mut output = original;
    diff.apply_to(&mut output);
    assert_eq!(output, sample(&[3.0, 4.0, 5.0]));
  }

  #[test]
  fn grass_removal_wins_over_conflicting_modification() {
    let original = sample(&[1.0, 2.0]);
    let mut modified = original.clone();
    modified.grass_instance_list[0].instances[0].scale = 150;
    let mut diff = YmapInstancedDataDiff::extract_from(&original, &modified);
    diff.merge(YmapInstancedDataDiff::extract_from(&original, &sample(&[2.0])));
    let mut output = original;
    diff.apply_to(&mut output);
    assert_eq!(output, sample(&[2.0]));
  }

  #[test]
  fn batch_metadata_change_and_empty_data_are_supported() {
    let original = sample(&[1.0]);
    let mut modified = original.clone();
    modified.grass_instance_list[0].batch_aabb.max.x = 20.0;
    modified.imap_link = "imap".into();
    modified.prop_instance_list.push("prop".into());
    let mut output = original.clone();
    YmapInstancedDataDiff::extract_from(&original, &modified).apply_to(&mut output);
    assert_eq!(output, modified);
    YmapInstancedDataDiff::extract_from(&modified, &YmapInstancedData::default())
      .apply_to(&mut output);
    assert_eq!(output, YmapInstancedData::default());
  }

  #[test]
  fn equivalent_batch_hash_and_name_do_not_change_grass() {
    let original = sample(&[1.0]);
    let mut modified = original.clone();
    modified.grass_instance_list[0].archetype_name =
      format!("hash_{:08X}", reference_hash("grass"));
    let mut output = original.clone();
    YmapInstancedDataDiff::extract_from(&original, &modified).apply_to(&mut output);
    assert_eq!(output, original);
  }

  #[test]
  fn conflicting_grass_edits_keep_first_change() {
    let original = sample(&[1.0]);
    let mut first = original.clone();
    first.grass_instance_list[0].instances[0].scale = 150;
    let mut second = original.clone();
    second.grass_instance_list[0].instances[0].scale = 200;
    let mut diff = YmapInstancedDataDiff::extract_from(&original, &first);
    diff.merge(YmapInstancedDataDiff::extract_from(&original, &second));
    let mut output = original;
    diff.apply_to(&mut output);
    assert_eq!(output, first);
  }
}
