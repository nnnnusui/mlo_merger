//! Instanced data differences.
//!
//! Grass batches are paired, then their instances compared, in world space: instance positions
//! are packed relative to their batch bounds, which editors change (CodeWalker
//! `YmapGrassInstanceBatch`). Positions match when they differ on every axis by at most
//! `tolerance.ymap_grass_position` or one packed step of either batch (`size / 65536`), so
//! re-quantisation into new bounds is not a change. Instances first match those with equal
//! attributes, then the rest by position alone, reporting their attribute changes; `pad` bytes
//! are ignored, since re-exporting tools rewrite them.
//!
//! Batches sharing an archetype and render settings can pair. Pairs are chosen by how many
//! instances match across them, then, for repainted batches sharing no instance, by how much
//! their bounds overlap in XY. Instances are compared only within a pair, so a batch split in two
//! is a modified batch plus an added one. Prop instances and the IMAP link compare by name hash.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
  NoFieldDiff, YmapItemDiff,
  field::{Pairing, changed, pair_by_keys, set_changed},
  item_diff::{ItemChange, apply_changes},
};
use crate::{
  common::{Result, position::Position},
  core::{
    config::matching::MatchTolerances,
    format::ymap::{
      diff::reference_hash,
      model::{BoundingBox, GrassInstance, GrassInstanceBatch, Ymap, YmapInstancedData},
    },
  },
};

/// CodeWalker's `BatchVertMultiplier`, scaling packed positions to the batch bounds.
const BATCH_VERT_MULTIPLIER: f64 = 0.00001525878;

/// Changed instanced data.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapInstancedDataDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub imap_link: Option<String>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub prop_instances: Vec<YmapPropInstanceDiff>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub grass_batches: Vec<YmapGrassBatchDiff>,
}

/// How one prop instance name of a modded YMAP differs from vanilla.
pub type YmapPropInstanceDiff = YmapItemDiff<String, NoFieldDiff>;

/// How one grass batch of a modded YMAP differs from vanilla.
pub type YmapGrassBatchDiff = YmapItemDiff<GrassInstanceBatch, YmapGrassBatchFieldDiff>;

/// Changes of a paired grass batch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapGrassBatchFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub batch_aabb: Option<BoundingBox>,
  /// Indices into the vanilla batch's instances that the modded batch lacks.
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub removed_instances: Vec<usize>,
  /// Instances only the modded batch has.
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub added_instances: Vec<YmapAddedGrassInstance>,
  /// Instances at the same place whose attributes changed.
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub modified_instances: Vec<YmapModifiedGrassInstance>,
}

/// A modded grass instance vanilla lacks, packed relative to its batch bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapAddedGrassInstance {
  /// Index of the instance within the modded batch.
  pub index: usize,
  pub world_position: Position,
  pub instance: GrassInstance,
}

/// A grass instance kept in place with changed attributes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapModifiedGrassInstance {
  /// Index of the instance within the vanilla batch.
  pub vanilla_index: usize,
  /// Index of the instance within the modded batch.
  pub modded_index: usize,
  pub world_position: Position,
  pub changes: YmapGrassInstanceFieldDiff,
}

/// Changed grass instance attributes, each holding its new value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapGrassInstanceFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub normal_x: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub normal_y: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub color: Option<Vec<u32>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub scale: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ao: Option<u32>,
}

/// Collects `instanced_data` differences, or `None` when there are none.
pub fn instanced_data_diff(
  vanilla: &Ymap,
  modded: &Ymap,
  tolerances: &MatchTolerances,
) -> Option<YmapInstancedDataDiff> {
  let (before, after) = (&vanilla.instanced_data, &modded.instanced_data);
  let names = |list: &[String]| list.iter().map(|name| Some(reference_hash(name))).collect();
  let props = pair_by_keys(names(&before.prop_instance_list), names(&after.prop_instance_list));
  let diff = YmapInstancedDataDiff {
    imap_link: (reference_hash(&before.imap_link) != reference_hash(&after.imap_link))
      .then(|| after.imap_link.clone()),
    prop_instances: YmapItemDiff::unpaired(props, &after.prop_instance_list),
    grass_batches: grass_batch_diffs(
      &before.grass_instance_list,
      &after.grass_instance_list,
      tolerances.ymap_grass_position,
    ),
  };
  (diff != YmapInstancedDataDiff::default()).then_some(diff)
}

/// Archetype and render settings shared by batches whose instances can match.
fn settings_key(batch: &GrassInstanceBatch) -> GroupKey {
  (
    reference_hash(&batch.archetype_name),
    [batch.scale_range.x, batch.scale_range.y, batch.scale_range.z].map(f32::to_bits),
    [batch.lod_dist, batch.lod_fade_start_dist, batch.lod_inst_fade_range].map(f32::to_bits),
    batch.orient_to_terrain,
  )
}

/// Instance attributes that must be equal to match, excluding position and `pad`.
fn attributes(instance: &GrassInstance) -> (u32, u32, Vec<u32>, u32, u32) {
  (instance.normal_x, instance.normal_y, instance.color.clone(), instance.scale, instance.ao)
}

/// World position of `instance` (CodeWalker `GetGrassWorldPos`), or `None` if malformed.
pub fn grass_world_position(
  batch: &GrassInstanceBatch,
  instance: &GrassInstance,
) -> Option<Position> {
  unpack_position(&batch.batch_aabb, &instance.position)
}

/// World position of a `packed` position within `bounds`, or `None` if malformed.
fn unpack_position(
  bounds: &BoundingBox,
  packed: &[f32],
) -> Option<Position> {
  let [x, y, z] = packed else {
    return None;
  };
  let (min, max) = (&bounds.min, &bounds.max);
  let axis = |min: f32, max: f32, packed: f32| {
    (f64::from(min)
      + (f64::from(max) - f64::from(min)) * (f64::from(packed) * BATCH_VERT_MULTIPLIER)) as f32
  };
  Some(Position {
    x: axis(min.x, max.x, *x),
    y: axis(min.y, max.y, *y),
    z: axis(min.z, max.z, *z),
  })
}

/// `position` packed relative to `bounds`, the inverse of [`unpack_position`] up to one packed
/// step; axes of zero size pack to 0.
fn pack_position(
  bounds: &BoundingBox,
  position: &Position,
) -> Vec<f32> {
  let (min, max) = (&bounds.min, &bounds.max);
  let axis = |min: f32, max: f32, world: f32| {
    let size = f64::from(max) - f64::from(min);
    if size == 0.0 {
      return 0.0;
    }
    let packed = (f64::from(world) - f64::from(min)) / size / BATCH_VERT_MULTIPLIER;
    packed.round().clamp(0.0, f64::from(u16::MAX)) as f32
  };
  vec![
    axis(min.x, max.x, position.x),
    axis(min.y, max.y, position.y),
    axis(min.z, max.z, position.z),
  ]
}

/// An instance located in world space, remembering where it is stored.
#[derive(Clone)]
struct Placed<'a> {
  batch: usize,
  instance: &'a GrassInstance,
  position: Option<Position>,
  /// One packed position step of the instance's batch on each axis.
  step: [f32; 3],
}

/// Every batch's instances located in world space.
fn place(batches: &[GrassInstanceBatch]) -> Vec<Vec<Placed<'_>>> {
  batches
    .iter()
    .enumerate()
    .map(|(batch_index, batch)| {
      let (min, max) = (&batch.batch_aabb.min, &batch.batch_aabb.max);
      let step = [max.x - min.x, max.y - min.y, max.z - min.z]
        .map(|size| (f64::from(size.abs()) * BATCH_VERT_MULTIPLIER) as f32);
      batch
        .instances
        .iter()
        .map(|instance| Placed {
          batch: batch_index,
          instance,
          position: grass_world_position(batch, instance),
          step,
        })
        .collect()
    })
    .collect()
}

type GroupKey = (u32, [u32; 3], [u32; 3], u32);

/// Pairs unmatched `before` instances with unmatched `after` ones whose positions match and
/// whose `key`s are equal, each `before` taking the first such `after` in order.
fn match_positions<K: std::hash::Hash + Eq + Clone>(
  before: &[Placed],
  after: &[Placed],
  before_used: &mut [bool],
  after_used: &mut [bool],
  tolerance: f32,
  key: impl Fn(&GrassInstance) -> K,
) -> Vec<(usize, usize)> {
  // Cells must be at least as large as the widest allowed difference.
  let widest = before.iter().chain(after).flat_map(|placed| placed.step).fold(tolerance, f32::max);
  let cell = |value: f32| (f64::from(value) / f64::from(widest)).floor() as i64;
  let mut grid = HashMap::<_, Vec<usize>>::new();
  for (index, placed) in after.iter().enumerate() {
    if let (false, Some(position)) = (after_used[index], &placed.position) {
      let cells = (cell(position.x), cell(position.y), cell(position.z));
      grid.entry((key(placed.instance), cells)).or_default().push(index);
    }
  }
  let mut pairs = Vec::new();
  for (index, placed) in before.iter().enumerate() {
    let (false, Some(position)) = (before_used[index], &placed.position) else {
      continue;
    };
    let wanted = key(placed.instance);
    let (x, y, z) = (cell(position.x), cell(position.y), cell(position.z));
    let mut best: Option<usize> = None;
    let neighbours = (-1..=1)
      .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (x + dx, y + dy, z + dz))));
    for cells in neighbours {
      let Some(candidates) = grid.get(&(wanted.clone(), cells)) else {
        continue;
      };
      for &candidate in candidates.iter() {
        let other = &after[candidate];
        let close = |axis: usize, from: f32, to: f32| {
          (from - to).abs() <= tolerance.max(placed.step[axis]).max(other.step[axis])
        };
        let to = other.position.as_ref().unwrap();
        if !after_used[candidate]
          && close(0, position.x, to.x)
          && close(1, position.y, to.y)
          && close(2, position.z, to.z)
          && best.is_none_or(|best| candidate < best)
        {
          best = Some(candidate);
        }
      }
    }
    if let Some(candidate) = best {
      before_used[index] = true;
      after_used[candidate] = true;
      pairs.push((index, candidate));
    }
  }
  pairs
}

/// Instance pairs of `before` and `after` that are unchanged, then those that only moved
/// attributes, as `(before index, after index)`.
fn match_instances(
  before: &[Placed],
  after: &[Placed],
  tolerance: f32,
) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
  let (mut before_used, mut after_used) = (vec![false; before.len()], vec![false; after.len()]);
  let unchanged =
    match_positions(before, after, &mut before_used, &mut after_used, tolerance, attributes);
  let modified =
    match_positions(before, after, &mut before_used, &mut after_used, tolerance, |_| ());
  (unchanged, modified)
}

/// The instances of `batches`, in batch order.
fn flatten<'a>(
  batches: &[usize],
  placed: &[Vec<Placed<'a>>],
) -> Vec<Placed<'a>> {
  batches.iter().flat_map(|index| placed[*index].iter().cloned()).collect()
}

/// Overlapping XY area of two batch bounds.
fn overlap_area(
  a: &BoundingBox,
  b: &BoundingBox,
) -> f32 {
  let overlap =
    |a_min: f32, a_max: f32, b_min: f32, b_max: f32| (a_max.min(b_max) - a_min.max(b_min)).max(0.0);
  overlap(a.min.x, a.max.x, b.min.x, b.max.x) * overlap(a.min.y, a.max.y, b.min.y, b.max.y)
}

/// Greedily pairs unpaired batches by descending `score`, skipping scores that are not positive.
fn pair_by_score(
  candidates: Vec<(usize, usize, f32)>,
  vanilla_paired: &mut [bool],
  modded_paired: &mut [bool],
  pairs: &mut Vec<(usize, usize)>,
) {
  let mut candidates =
    candidates.into_iter().filter(|(_, _, score)| *score > 0.0).collect::<Vec<_>>();
  candidates.sort_by(|a, b| b.2.total_cmp(&a.2).then((a.0, a.1).cmp(&(b.0, b.1))));
  for (vanilla_index, modded_index, _) in candidates {
    if !vanilla_paired[vanilla_index] && !modded_paired[modded_index] {
      vanilla_paired[vanilla_index] = true;
      modded_paired[modded_index] = true;
      pairs.push((vanilla_index, modded_index));
    }
  }
}

fn grass_batch_diffs(
  vanilla: &[GrassInstanceBatch],
  modded: &[GrassInstanceBatch],
  tolerance: f32,
) -> Vec<YmapGrassBatchDiff> {
  let (vanilla_placed, modded_placed) = (place(vanilla), place(modded));
  let groups = |batches: &[GrassInstanceBatch]| {
    let mut groups = HashMap::<GroupKey, Vec<usize>>::new();
    for (index, batch) in batches.iter().enumerate() {
      groups.entry(settings_key(batch)).or_default().push(index);
    }
    groups
  };
  let (vanilla_groups, modded_groups) = (groups(vanilla), groups(modded));
  let mut vanilla_paired = vec![false; vanilla.len()];
  let mut modded_paired = vec![false; modded.len()];
  let mut pairs = Vec::new();
  let mut group_keys =
    vanilla_groups.keys().filter(|key| modded_groups.contains_key(*key)).collect::<Vec<_>>();
  group_keys.sort();
  for key in group_keys {
    let (vanilla_batches, modded_batches) = (&vanilla_groups[key], &modded_groups[key]);
    // Count instances matching across each batch pair over the whole group.
    let (before, after) =
      (flatten(vanilla_batches, &vanilla_placed), flatten(modded_batches, &modded_placed));
    let (unchanged, modified) = match_instances(&before, &after, tolerance);
    let mut shared = HashMap::<(usize, usize), f32>::new();
    for (from, to) in unchanged.into_iter().chain(modified) {
      *shared.entry((before[from].batch, after[to].batch)).or_default() += 1.0;
    }
    pair_by_score(
      shared.into_iter().map(|((a, b), score)| (a, b, score)).collect(),
      &mut vanilla_paired,
      &mut modded_paired,
      &mut pairs,
    );
    // Repainted batches share no instance; pair them by overlapping bounds.
    let overlaps = vanilla_batches
      .iter()
      .flat_map(|a| modded_batches.iter().map(move |b| (*a, *b)))
      .map(|(a, b)| (a, b, overlap_area(&vanilla[a].batch_aabb, &modded[b].batch_aabb)))
      .collect();
    pair_by_score(overlaps, &mut vanilla_paired, &mut modded_paired, &mut pairs);
  }
  pairs.sort();
  let pairing = Pairing {
    pairs,
    removed: (0..vanilla.len()).filter(|index| !vanilla_paired[*index]).collect(),
    added: (0..modded.len()).filter(|index| !modded_paired[*index]).collect(),
  };
  YmapItemDiff::collect(pairing, modded, |vanilla_index, modded_index| {
    YmapGrassBatchFieldDiff::between(
      (&vanilla[vanilla_index], &vanilla_placed[vanilla_index]),
      (&modded[modded_index], &modded_placed[modded_index]),
      tolerance,
    )
  })
}

impl YmapInstancedDataDiff {
  /// Overwrites the changed fields of `data` and rebuilds its prop instances and grass batches.
  pub fn apply_to(
    &self,
    data: &mut YmapInstancedData,
  ) -> Result<()> {
    let Self {
      imap_link,
      prop_instances,
      grass_batches,
    } = self;
    set_changed(&mut data.imap_link, imap_link);
    data.prop_instance_list = YmapItemDiff::apply(
      "prop instances",
      &data.prop_instance_list,
      prop_instances,
      |item, changes| changes.apply_to(item),
    )?;
    data.grass_instance_list = YmapItemDiff::try_apply(
      "grass batches",
      &data.grass_instance_list,
      grass_batches,
      |batch, changes| changes.apply_to(batch),
    )?;
    Ok(())
  }
}

impl YmapGrassBatchFieldDiff {
  /// Applies the changes to `batch`.
  ///
  /// When the bounds change, the vanilla instances are repacked into the new bounds so they keep
  /// their world positions; added instances are already packed relative to the new bounds.
  pub fn apply_to(
    &self,
    batch: &mut GrassInstanceBatch,
  ) -> Result<()> {
    let Self {
      batch_aabb,
      removed_instances,
      added_instances,
      modified_instances,
    } = self;
    if let Some(bounds) = batch_aabb {
      for instance in &mut batch.instances {
        if let Some(position) = unpack_position(&batch.batch_aabb, &instance.position) {
          instance.position = pack_position(bounds, &position);
        }
      }
      batch.batch_aabb = bounds.clone();
    }
    let removed = removed_instances.iter().map(|&index| ItemChange::Removed {
      index,
    });
    let added = added_instances.iter().map(|added| ItemChange::Added {
      index: added.index,
      item: &added.instance,
    });
    let modified = modified_instances.iter().map(|modified| ItemChange::Modified {
      vanilla_index: modified.vanilla_index,
      modded_index: modified.modded_index,
      changes: &modified.changes,
    });
    batch.instances = apply_changes(
      "grass instances",
      &batch.instances,
      removed.chain(added).chain(modified),
      |instance, changes| {
        changes.apply_to(instance);
        Ok(())
      },
    )?;
    Ok(())
  }

  /// Bounds and instance changes of a paired batch, comparing instances in world space.
  fn between(
    (before, before_placed): (&GrassInstanceBatch, &[Placed]),
    (after, after_placed): (&GrassInstanceBatch, &[Placed]),
    tolerance: f32,
  ) -> Self {
    let (unchanged, modified) = match_instances(before_placed, after_placed, tolerance);
    let mut before_kept = vec![false; before_placed.len()];
    let mut after_kept = vec![false; after_placed.len()];
    for &(from, to) in unchanged.iter().chain(&modified) {
      before_kept[from] = true;
      after_kept[to] = true;
    }
    let mut modified_instances = modified
      .into_iter()
      .map(|(from, to)| YmapModifiedGrassInstance {
        vanilla_index: from,
        modded_index: to,
        world_position: after_placed[to].position.clone().unwrap_or_default(),
        changes: YmapGrassInstanceFieldDiff::between(
          before_placed[from].instance,
          after_placed[to].instance,
        ),
      })
      .collect::<Vec<_>>();
    modified_instances.sort_by_key(|it| it.vanilla_index);
    Self {
      batch_aabb: changed(&before.batch_aabb, &after.batch_aabb),
      removed_instances: (0..before_placed.len()).filter(|index| !before_kept[*index]).collect(),
      added_instances: (0..after_placed.len())
        .filter(|index| !after_kept[*index])
        .map(|index| YmapAddedGrassInstance {
          index,
          world_position: after_placed[index].position.clone().unwrap_or_default(),
          instance: after.instances[index].clone(),
        })
        .collect(),
      modified_instances,
    }
  }
}

impl YmapGrassInstanceFieldDiff {
  /// Overwrites the changed attributes of `instance`.
  pub fn apply_to(
    &self,
    instance: &mut GrassInstance,
  ) {
    let Self {
      normal_x,
      normal_y,
      color,
      scale,
      ao,
    } = self;
    set_changed(&mut instance.normal_x, normal_x);
    set_changed(&mut instance.normal_y, normal_y);
    set_changed(&mut instance.color, color);
    set_changed(&mut instance.scale, scale);
    set_changed(&mut instance.ao, ao);
  }

  /// Attributes of `after` changed from `before`, excluding position and `pad`.
  pub fn between(
    before: &GrassInstance,
    after: &GrassInstance,
  ) -> Self {
    Self {
      normal_x: changed(&before.normal_x, &after.normal_x),
      normal_y: changed(&before.normal_y, &after.normal_y),
      color: changed(&before.color, &after.color),
      scale: changed(&before.scale, &after.scale),
      ao: changed(&before.ao, &after.ao),
    }
  }
}
