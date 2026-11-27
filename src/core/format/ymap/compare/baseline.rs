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
//! - A modification or replacement of an item the resource keeps as its baseline has it is left
//!   out. One the resource changed makes only those changes to the latest item.
//!
//! A removal of a baseline item the latest lacks is not among the differences from the latest
//! to begin with. Items are paired between versions as the differences pair them, by GUID for
//! entities, so a reordering between versions changes nothing.
//!
//! Entities, box occluders, time cycle modifiers and car generators are handled, and the YMAP's
//! `parent` and `contentFlags`; the other differences are kept as they are.

use std::collections::{HashMap, HashSet};

use struct_diff::{
  Diff,
  vec::{ItemDiff, Pair},
};

use super::{
  YmapDiffFields,
  entities::{self, YmapEntityDiff},
  entity_parent::YmapDiffContext,
};
use crate::core::format::ymap::model::{Ymap, YmapEntity};

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
}

/// How many changes `diffs` makes: one per array item and per other changed field, by which the
/// vanilla version a source differs least from is told.
pub fn change_count(diffs: &YmapDiffFields) -> usize {
  fn items<T>(items: &Option<Vec<T>>) -> usize {
    items.as_ref().map_or(0, Vec::len)
  }
  let fields = [
    diffs.parent.is_some(),
    diffs.content_flags.is_some(),
    diffs.distant_lod_lights.is_some(),
    diffs.instanced_data.is_some(),
  ];
  items(&diffs.entities)
    + items(&diffs.box_occluders)
    + items(&diffs.occlude_models)
    + items(&diffs.lod_lights)
    + items(&diffs.physics_dictionaries)
    + items(&diffs.time_cycle_modifiers)
    + items(&diffs.car_generators)
    + fields.into_iter().filter(|changed| *changed).count()
}

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
  let Some(list) = diffs else {
    return;
  };
  let source_baseline = by_later(&T::pair(baseline, source, ctx).pairs);
  let latest_pairs = T::pair(baseline, latest, ctx).pairs;
  let latest_baseline = by_later(&latest_pairs);
  let kept_by_latest = latest_pairs.iter().map(|&(baseline, _)| baseline).collect::<HashSet<_>>();
  list.retain_mut(|diff| match diff {
    ItemDiff::Removed {
      base_index,
      implied,
      reason,
    } => *implied || reason.is_some() || latest_baseline.contains_key(base_index),
    ItemDiff::Added {
      target_index: Some(index),
      ..
    } => source_baseline.get(index).is_none_or(|baseline| kept_by_latest.contains(baseline)),
    ItemDiff::Added {
      ..
    } => true,
    ItemDiff::Replaced {
      target_index: Some(index),
      ..
    } => source_baseline
      .get(index)
      .is_none_or(|&baseline_index| baseline[baseline_index].diff(&source[*index], ctx).is_some()),
    ItemDiff::Replaced {
      ..
    } => true,
    ItemDiff::Modified {
      base_index,
      target_index,
      changes,
      reason: None,
    } => {
      let Some(&baseline_index) = source_baseline.get(target_index) else {
        return true;
      };
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
    ItemDiff::Modified {
      ..
    } => true,
  });
  if list.is_empty() {
    *diffs = None;
  }
}
