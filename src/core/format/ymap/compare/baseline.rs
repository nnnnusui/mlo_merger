//! Leaves out of a source YMAP's differences from the latest vanilla version what only tells the
//! vanilla version the source was made from apart from the latest.
//!
//! A resource's YMAP is made from one vanilla version, its baseline, which may be older than the
//! latest. Its differences from the latest then also hold every change vanilla made since, as
//! if the resource reverted them. [`relative_to_baseline`] keeps, of the differences from the
//! latest, those the resource made to its baseline:
//!
//! - A removal of an item the baseline lacks, which vanilla added after it, is left out.
//! - An addition of an item the baseline has, which vanilla removed after it, is left out,
//!   whether or not the resource changed it: there is nothing left to change.
//! - A modification of an item the resource keeps as its baseline has it is left out. One the
//!   resource changed makes only those changes to the latest item.
//! - A replacement, the removal of a latest item and the addition of a source one in its place,
//!   keeps whichever of the two is not left out.
//!
//! A removal of a baseline item the latest lacks is not among the differences from the latest
//! to begin with. Items are paired between versions as the differences pair them, by GUID for
//! entities, so a reordering between versions changes nothing.
//!
//! Every array of the YMAP is handled, as are its `parent` and `contentFlags`. Occlude model
//! triangles relate as the differences compare them, by the vertices they join; grass batches
//! and instances as they pair, in world space. The two YMAPs of a LOD light pair are related
//! each on its own; the removals of either are then made in the other on the latest (see
//! [`super::lod_light_pairs`]), so both keep their lights in step.

use std::collections::{HashMap, HashSet};

use struct_diff::{
  Diff,
  vec::{ItemDiff, Pair},
};

use super::{
  YmapDiffFields, YmapOccludeTrianglesDiff,
  entities::{self, YmapEntityDiff},
  entity_parent::YmapDiffContext,
  instanced_data,
  occlude_models::VertexIndex,
};
use crate::core::format::ymap::{
  model::{Ymap, YmapEntity, YmapOccludeModel},
  reference_hash,
};

/// The YMAPs and contexts [`relative_to_baseline`] relates: the latest vanilla version and the
/// source compared under `latest_ctx`, the baseline and the source under `baseline_ctx`.
pub struct BaselineComparison<'a> {
  pub latest: &'a Ymap,
  pub baseline: &'a Ymap,
  pub source: &'a Ymap,
  pub latest_ctx: &'a YmapDiffContext,
  pub baseline_ctx: &'a YmapDiffContext,
}

/// Leaves out of `diffs`, the differences of the source from the latest vanilla version, what
/// the source keeps from its baseline; see the module docs.
pub fn relative_to_baseline(
  diffs: &mut YmapDiffFields,
  versions: &BaselineComparison,
) {
  let BaselineComparison {
    latest,
    baseline,
    source,
    baseline_ctx,
    ..
  } = *versions;
  if diffs.parent.is_some() && source.parent == baseline.parent {
    diffs.parent = None;
  }
  if diffs.content_flags.is_some() && source.content_flags == baseline.content_flags {
    diffs.content_flags = None;
  }
  relate_entities(&mut diffs.entities, versions);
  let tolerances = &baseline_ctx.tolerances;
  relate_items(
    &mut diffs.box_occluders,
    (&latest.box_occluders, &baseline.box_occluders, &source.box_occluders),
    tolerances,
  );
  relate_items(
    &mut diffs.time_cycle_modifiers,
    (&latest.time_cycle_modifiers, &baseline.time_cycle_modifiers, &source.time_cycle_modifiers),
    tolerances,
  );
  relate_items(
    &mut diffs.car_generators,
    (&latest.car_generators, &baseline.car_generators, &source.car_generators),
    tolerances,
  );
  relate_names(
    &mut diffs.physics_dictionaries,
    (&latest.physics_dictionaries, &baseline.physics_dictionaries, &source.physics_dictionaries),
  );
  relate_triangles(
    &mut diffs.occlude_models,
    &baseline.occlude_models,
    tolerances.ymap_occlude_vertex_position,
  );
  relate_items(
    &mut diffs.lod_lights,
    (&latest.lod_lights, &baseline.lod_lights, &source.lod_lights),
    &(),
  );
  if let Some(distant) = &mut diffs.distant_lod_lights {
    relate_items(
      &mut distant.items,
      (
        &latest.distant_lod_lights.items,
        &baseline.distant_lod_lights.items,
        &source.distant_lod_lights.items,
      ),
      tolerances,
    );
    if distant.num_street_lights.is_some()
      && source.distant_lod_lights.num_street_lights
        == baseline.distant_lod_lights.num_street_lights
    {
      distant.num_street_lights = None;
    }
    if distant.category.is_some()
      && source.distant_lod_lights.category == baseline.distant_lod_lights.category
    {
      distant.category = None;
    }
    if *distant == Default::default() {
      diffs.distant_lod_lights = None;
    }
  }
  if let Some(instanced) = &mut diffs.instanced_data {
    let (latest, baseline, source) =
      (&latest.instanced_data, &baseline.instanced_data, &source.instanced_data);
    if instanced.imap_link.is_some()
      && reference_hash(&source.imap_link) == reference_hash(&baseline.imap_link)
    {
      instanced.imap_link = None;
    }
    relate_names(
      &mut instanced.prop_instance_list,
      (&latest.prop_instance_list, &baseline.prop_instance_list, &source.prop_instance_list),
    );
    instanced_data::relate_grass_batches(
      &mut instanced.grass_instance_list,
      (&latest.grass_instance_list, &baseline.grass_instance_list, &source.grass_instance_list),
      tolerances.ymap_grass_position,
    );
    if *instanced == Default::default() {
      diffs.instanced_data = None;
    }
  }
}

/// How many changes `diffs` makes: one per array item and per other changed field, by which the
/// vanilla version a source differs least from is told.
///
/// Occlude models count one per triangle, and grass batches one per instance they change.
pub fn change_count(diffs: &YmapDiffFields) -> usize {
  fn items<T>(items: &Option<Vec<T>>) -> usize {
    items.as_ref().map_or(0, Vec::len)
  }
  let fields = [diffs.parent.is_some(), diffs.content_flags.is_some()];
  let triangles =
    diffs.occlude_models.iter().flatten().map(|diff| diff.removed.len() + diff.added.len());
  let distant = diffs.distant_lod_lights.as_ref().map_or(0, |distant| {
    items(&distant.items)
      + usize::from(distant.num_street_lights.is_some())
      + usize::from(distant.category.is_some())
  });
  let instanced = diffs.instanced_data.as_ref().map_or(0, |instanced| {
    usize::from(instanced.imap_link.is_some())
      + items(&instanced.prop_instance_list)
      + instanced
        .grass_instance_list
        .iter()
        .flatten()
        .map(instanced_data::grass_change_count)
        .sum::<usize>()
  });
  items(&diffs.entities)
    + items(&diffs.box_occluders)
    + triangles.sum::<usize>()
    + items(&diffs.lod_lights)
    + distant
    + items(&diffs.physics_dictionaries)
    + instanced
    + items(&diffs.time_cycle_modifiers)
    + items(&diffs.car_generators)
    + fields.into_iter().filter(|changed| *changed).count()
}

/// Paired indices, as `(earlier, later)`.
type Pairs = Vec<(usize, usize)>;

/// Each later index of `pairs` keyed to its earlier one.
fn by_later(pairs: &[(usize, usize)]) -> HashMap<usize, usize> {
  pairs.iter().map(|&(earlier, later)| (later, earlier)).collect()
}

fn relate_entities(
  diffs: &mut Option<Vec<YmapEntityDiff>>,
  versions: &BaselineComparison,
) {
  let Some(list) = diffs else {
    return;
  };
  let BaselineComparison {
    latest,
    baseline,
    source,
    baseline_ctx,
    ..
  } = *versions;
  let (latest_entities, baseline_entities, source_entities) =
    (&latest.entities, &baseline.entities, &source.entities);
  let aliases = &baseline_ctx.guid_aliases;
  let source_baseline =
    by_later(&entities::guid_pairs(baseline_entities, source_entities, aliases));
  let latest_pairs = entities::guid_pairs(baseline_entities, latest_entities, &Default::default());
  let latest_baseline = by_later(&latest_pairs);
  let kept_by_latest = latest_pairs.iter().map(|&(baseline, _)| baseline).collect::<HashSet<_>>();
  // The source entity as its baseline pairs it, keeping the baseline's GUID it was rounded from.
  let source_entity = |index: usize, baseline_index: usize| {
    let entity = &source_entities[index];
    match aliases.contains_key(&entity.guid) {
      true => YmapEntity {
        guid: baseline_entities[baseline_index].guid,
        ..entity.clone()
      },
      false => entity.clone(),
    }
  };
  let tolerances = &baseline_ctx.tolerances;
  list.retain_mut(|diff| match diff {
    YmapEntityDiff::Removed {
      base_index,
      reason,
    } => reason.is_some() || latest_baseline.contains_key(base_index),
    YmapEntityDiff::Added {
      target_index: Some(index),
      ..
    } => source_baseline.get(index).is_none_or(|baseline| kept_by_latest.contains(baseline)),
    YmapEntityDiff::Added {
      ..
    } => true,
    YmapEntityDiff::Modified {
      base_index,
      target_index,
      changes,
      parent,
      reason: None,
    } => {
      let Some(&baseline_index) = source_baseline.get(target_index) else {
        return true;
      };
      let source = source_entity(*target_index, baseline_index);
      let intent = baseline_entities[baseline_index].diff(&source, tolerances);
      let parent_changed =
        entities::parent_of(&baseline_ctx.modded_parents, source_entities, *target_index)
          != entities::parent_of(&baseline_ctx.vanilla_parents, baseline_entities, baseline_index);
      if !parent_changed {
        *parent = None;
      }
      let latest_entity = &latest_entities[*base_index];
      let mut made = latest_entity.clone();
      let relative = match &intent {
        None => None,
        Some(intent) => match made.apply(intent) {
          Ok(()) => latest_entity.diff(&made, tolerances),
          // Kept as taken from the latest version when the changes do not apply to it.
          Err(_) => return true,
        },
      };
      *changes = relative.unwrap_or_default();
      *changes != Default::default() || parent.is_some()
    }
    YmapEntityDiff::Modified {
      ..
    } => true,
  });
  if list.is_empty() {
    *diffs = None;
  }
}

fn relate_items<T: Pair + Clone>(
  diffs: &mut Option<Vec<ItemDiff<T, T::Output>>>,
  (latest, baseline, source): (&[T], &[T], &[T]),
  ctx: &T::Context,
) where
  T::Output: PartialEq,
{
  let pairs = (T::pair(baseline, source, ctx).pairs, T::pair(baseline, latest, ctx).pairs);
  relate_paired(diffs, pairs, |diff, baseline_index| match diff {
    ItemDiff::Modified {
      base_index,
      target_index,
      changes,
      ..
    } => {
      let Some(intent) = baseline[baseline_index].diff(&source[*target_index], ctx) else {
        return false;
      };
      let mut made = latest[*base_index].clone();
      if made.apply(&intent).is_err() {
        return true;
      }
      match latest[*base_index].diff(&made, ctx) {
        Some(relative) => {
          *changes = relative;
          true
        }
        None => false,
      }
    }
    _ => true,
  });
}

/// [`relate_items`] for names compared by hash as a multiset, which are only added or removed.
fn relate_names<C>(
  diffs: &mut Option<Vec<ItemDiff<String, C>>>,
  (latest, baseline, source): (&[String], &[String], &[String]),
) {
  let names =
    |list: &[String]| list.iter().map(|name| Some(reference_hash(name))).collect::<Vec<_>>();
  let pairs =
    |later: &[String]| struct_diff::vec::pair_by_keys(names(baseline), names(later)).pairs;
  relate_paired(diffs, (pairs(source), pairs(latest)), |_, _| true);
}

/// Leaves out of `diffs`, taken from the latest version to the source, the removals of latest
/// items the baseline lacks and the additions of baseline items the latest lacks, given the
/// pairs of baseline items with source ones and with latest ones. A replacement, the removal of a
/// latest item and the addition of a source one in its place, keeps the part that is not left
/// out. A modification of a source item paired with a baseline item is kept when `paired`
/// returns true, which may change it.
pub(super) fn relate_paired<T, C>(
  diffs: &mut Option<Vec<ItemDiff<T, C>>>,
  (source_pairs, latest_pairs): (Pairs, Pairs),
  mut paired: impl FnMut(&mut ItemDiff<T, C>, usize) -> bool,
) {
  let Some(list) = diffs else {
    return;
  };
  let source_baseline = by_later(&source_pairs);
  let latest_baseline = by_later(&latest_pairs);
  let kept_by_latest = latest_pairs.iter().map(|&(baseline, _)| baseline).collect::<HashSet<_>>();
  let kept_removal = |index: &usize| latest_baseline.contains_key(index);
  let kept_addition = |index: &usize| {
    source_baseline.get(index).is_none_or(|baseline| kept_by_latest.contains(baseline))
  };
  for diff in list.iter_mut() {
    let ItemDiff::Replaced {
      base_index,
      target_index: Some(target_index),
      ..
    } = diff
    else {
      continue;
    };
    let (base_index, target_index) = (*base_index, *target_index);
    match (kept_removal(&base_index), kept_addition(&target_index)) {
      (true, false) => {
        *diff = ItemDiff::Removed {
          base_index,
          implied: false,
          reason: None,
        };
      }
      (false, true) => {
        let ItemDiff::Replaced {
          item,
          ..
        } = std::mem::replace(
          diff,
          ItemDiff::Removed {
            base_index,
            implied: false,
            reason: None,
          },
        )
        else {
          unreachable!()
        };
        *diff = ItemDiff::Added {
          anchor_base_index: Some(base_index),
          target_index: Some(target_index),
          item,
        };
      }
      _ => {}
    }
  }
  list.retain_mut(|diff| match diff {
    ItemDiff::Removed {
      base_index,
      implied,
      reason,
    } => *implied || reason.is_some() || kept_removal(base_index),
    ItemDiff::Added {
      target_index: Some(index),
      ..
    } => kept_addition(index),
    ItemDiff::Replaced {
      base_index,
      target_index: Some(index),
      ..
    } => kept_removal(base_index) || kept_addition(index),
    ItemDiff::Modified {
      target_index: index,
      reason: None,
      ..
    } => match source_baseline.get(&*index).copied() {
      Some(baseline_index) => paired(diff, baseline_index),
      None => true,
    },
    _ => true,
  });
  if list.is_empty() {
    *diffs = None;
  }
}

/// Leaves out of `diffs`, the occlude triangles changed from the latest version to the source,
/// the removals of triangles `baseline` lacks and the additions of those it has, keyed by the
/// vertices they join within `tolerance`.
fn relate_triangles(
  diffs: &mut Option<Vec<YmapOccludeTrianglesDiff>>,
  baseline: &[YmapOccludeModel],
  tolerance: f32,
) {
  let Some(list) = diffs else {
    return;
  };
  let mut index = VertexIndex::new(tolerance);
  let mut held = HashSet::new();
  for model in baseline {
    for triangle in &model.triangles {
      held.insert((model.flags, index.key(triangle)));
    }
  }
  for diff in list.iter_mut() {
    let flags = diff.flags;
    diff.removed.retain(|triangle| held.contains(&(flags, index.key(triangle))));
    diff.added.retain(|triangle| !held.contains(&(flags, index.key(triangle))));
  }
  list.retain(|diff| !diff.removed.is_empty() || !diff.added.is_empty());
  if list.is_empty() {
    *diffs = None;
  }
}
