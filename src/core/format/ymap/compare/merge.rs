//! Combines the differences of several sources from one vanilla YMAP into the one diff that
//! merging applies.
//!
//! Array changes combine as [`MergedItems`] describes: a removal wins over every other change of
//! the vanilla item, including another source's replacement of it; otherwise the first source to
//! modify or replace a vanilla item wins, and additions keep their place after the vanilla item
//! they follow, in source order. Occlude
//! models modified by several sources take each field from the first source changing it and
//! every source's triangle removals and additions, so cut-outs of every source survive. `content_flags`
//! are OR-ed into vanilla's, other fields keep the first change, each extent axis and block field
//! on its own, and added physics dictionaries and prop instances skip names already present.
//!
//! Every change of `sources` is combined; [`leave_out`] removes changes from one source's diff
//! beforehand. The merge stage leaves those listed in [`IgnoredChange::MERGE`] out of every
//! resource's diff, and those in [`IgnoredChange::DERIVED`] out of a vanilla overwrite's too.

use std::collections::HashSet;

use super::{
  ItemDiff, PositionDiff, YmapBlockDiff, YmapDiffFields, YmapDistantLodLightsDiff,
  YmapInstancedDataDiff, YmapOccludeModelDiff, YmapOccludeModelFieldDiff, YmapPropInstanceDiff,
  entities,
  merged_items::{MergedItems, merged, merged_with},
};
use crate::core::format::ymap::{
  model::{Ymap, YmapOccludeModel},
  reference_hash,
};

/// A kind of change that [`leave_out`] removes from a source's diff, keeping vanilla's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoredChange {
  Name,
  Flags,
  StreamingExtents,
  EntitiesExtents,
  Block,
  /// `numStreetLights` and `category` of the distant LOD lights.
  DistantLodLightCounts,
  /// Removals and replacements of vanilla physics dictionaries; replacing names are added.
  PhysicsDictionaryRemovals,
  /// Entities' `numChildren`, which the merge counts from the final parent references. An
  /// entity modification making no other change is dropped with it.
  EntityChildCounts,
}

impl IgnoredChange {
  /// What the merge stage leaves out of resources' diffs: metadata a resource's YMAP restates
  /// from the version it was made from, and dictionary removals, since CodeWalker only checks
  /// their membership.
  pub const MERGE: &[Self] = &[
    Self::Name,
    Self::Flags,
    Self::StreamingExtents,
    Self::EntitiesExtents,
    Self::Block,
    Self::DistantLodLightCounts,
    Self::PhysicsDictionaryRemovals,
    Self::EntityChildCounts,
  ];

  /// What the merge stage leaves out of every diff, a vanilla overwrite's included: values it
  /// derives from the merged YMAPs.
  pub const DERIVED: &[Self] = &[Self::EntityChildCounts];

  /// The field this change belongs to, as logs name it.
  fn field(self) -> &'static str {
    match self {
      Self::Name => "name",
      Self::Flags => "flags",
      Self::StreamingExtents => "streamingExtents",
      Self::EntitiesExtents => "entitiesExtents",
      Self::Block => "block",
      Self::DistantLodLightCounts => "distantLodLights counts",
      Self::PhysicsDictionaryRemovals => "physicsDictionaries removals",
      Self::EntityChildCounts => "entities numChildren",
    }
  }

  /// Whether `diff` makes this change.
  fn is_made_by(
    self,
    diff: &YmapDiffFields,
  ) -> bool {
    match self {
      Self::Name => diff.name.is_some(),
      Self::Flags => diff.flags.is_some(),
      Self::StreamingExtents => {
        diff.streaming_extents_min.is_some() || diff.streaming_extents_max.is_some()
      }
      Self::EntitiesExtents => {
        diff.entities_extents_min.is_some() || diff.entities_extents_max.is_some()
      }
      Self::Block => diff.block.is_some(),
      Self::DistantLodLightCounts => diff
        .distant_lod_lights
        .as_ref()
        .is_some_and(|lights| lights.num_street_lights.is_some() || lights.category.is_some()),
      Self::PhysicsDictionaryRemovals => diff
        .physics_dictionaries
        .iter()
        .flatten()
        .any(|diff| matches!(diff, ItemDiff::Removed { .. } | ItemDiff::Replaced { .. })),
      Self::EntityChildCounts => diff.entities.iter().flatten().any(|diff| {
        matches!(diff, entities::YmapEntityDiff::Modified { changes, .. } if changes.num_children.is_some())
      }),
    }
  }
}

/// Removes the changes `ignored` names from `diff`, logging each one it makes. Replacements of
/// vanilla physics dictionaries become additions after the replaced name.
pub fn leave_out(
  diff: &mut YmapDiffFields,
  ignored: &[IgnoredChange],
) {
  for &change in ignored {
    if !change.is_made_by(diff) {
      continue;
    }
    tracing::info!("    skip changes: <{} />", change.field());
    match change {
      IgnoredChange::Name => diff.name = None,
      IgnoredChange::Flags => diff.flags = None,
      IgnoredChange::StreamingExtents => {
        diff.streaming_extents_min = None;
        diff.streaming_extents_max = None;
      }
      IgnoredChange::EntitiesExtents => {
        diff.entities_extents_min = None;
        diff.entities_extents_max = None;
      }
      IgnoredChange::Block => diff.block = None,
      IgnoredChange::DistantLodLightCounts => {
        if let Some(lights) = &mut diff.distant_lod_lights {
          lights.num_street_lights = None;
          lights.category = None;
          if *lights == YmapDistantLodLightsDiff::default() {
            diff.distant_lod_lights = None;
          }
        }
      }
      IgnoredChange::PhysicsDictionaryRemovals => {
        let kept = diff
          .physics_dictionaries
          .take()
          .into_iter()
          .flatten()
          .filter_map(|diff| match diff {
            ItemDiff::Removed {
              ..
            } => None,
            ItemDiff::Replaced {
              base_index,
              target_index,
              item,
            } => Some(ItemDiff::Added {
              anchor_base_index: Some(base_index),
              target_index,
              item,
            }),
            diff => Some(diff),
          })
          .collect();
        diff.physics_dictionaries = non_empty(kept);
      }
      IgnoredChange::EntityChildCounts => {
        let kept = diff
          .entities
          .take()
          .into_iter()
          .flatten()
          .filter_map(|mut diff| {
            if let entities::YmapEntityDiff::Modified {
              changes,
              parent,
              ..
            } = &mut diff
            {
              changes.num_children = None;
              if parent.is_none() && *changes == Default::default() {
                return None;
              }
            }
            Some(diff)
          })
          .collect();
        diff.entities = non_empty(kept);
      }
    }
  }
}

/// One diff of `vanilla` making the changes of every diff in `sources`, taken from `vanilla` in
/// merge order.
pub fn merge_diffs(
  vanilla: &Ymap,
  sources: &[YmapDiffFields],
) -> YmapDiffFields {
  let content_flags =
    sources.iter().filter_map(|diff| diff.content_flags).fold(vanilla.content_flags, |a, b| a | b);
  let distant_lod_lights = sources.iter().filter_map(|diff| diff.distant_lod_lights.as_ref());
  let instanced_data = sources.iter().filter_map(|diff| diff.instanced_data.as_ref());
  YmapDiffFields {
    name: first_change("name", sources.iter().map(|diff| &diff.name)),
    flags: first_change("flags", sources.iter().map(|diff| &diff.flags)),
    content_flags: (content_flags != vanilla.content_flags).then_some(content_flags),
    parent: first_reference("parent", sources.iter().map(|diff| &diff.parent)),
    streaming_extents_min: first_position(sources.iter().map(|diff| &diff.streaming_extents_min)),
    streaming_extents_max: first_position(sources.iter().map(|diff| &diff.streaming_extents_max)),
    entities_extents_min: first_position(sources.iter().map(|diff| &diff.entities_extents_min)),
    entities_extents_max: first_position(sources.iter().map(|diff| &diff.entities_extents_max)),
    block: first_block(sources.iter().filter_map(|diff| diff.block.as_ref())),
    entities: non_empty(entities::merge(
      &vanilla.entities,
      &lists(sources.iter().map(|diff| &diff.entities)),
    )),
    box_occluders: merge_items(
      "box_occluders",
      vanilla.box_occluders.len(),
      sources.iter().map(|diff| diff.box_occluders.as_ref()),
    ),
    occlude_models: merge_occlude_models(
      &vanilla.occlude_models,
      sources.iter().map(|diff| diff.occlude_models.as_ref()),
    ),
    lod_lights: merge_items(
      "lod_lights",
      vanilla.lod_lights.len(),
      sources.iter().map(|diff| diff.lod_lights.as_ref()),
    ),
    distant_lod_lights: {
      let lights = YmapDistantLodLightsDiff {
        items: merge_items(
          "distant_lod_lights",
          vanilla.distant_lod_lights.items.len(),
          distant_lod_lights.clone().map(|diff| diff.items.as_ref()),
        ),
        num_street_lights: first_change(
          "numStreetLights",
          distant_lod_lights.clone().map(|diff| &diff.num_street_lights),
        ),
        category: first_change("category", distant_lod_lights.map(|diff| &diff.category)),
      };
      (lights != YmapDistantLodLightsDiff::default()).then_some(lights)
    },
    physics_dictionaries: merge_references(
      "physics_dictionaries",
      &vanilla.physics_dictionaries,
      &lists(sources.iter().map(|diff| &diff.physics_dictionaries)),
    ),
    instanced_data: {
      let data = YmapInstancedDataDiff {
        imap_link: first_reference("imap_link", instanced_data.clone().map(|diff| &diff.imap_link)),
        prop_instance_list: merge_references(
          "prop_instance_list",
          &vanilla.instanced_data.prop_instance_list,
          &lists(instanced_data.clone().map(|diff| &diff.prop_instance_list)),
        ),
        grass_instance_list: merge_items(
          "grass_instance_list",
          vanilla.instanced_data.grass_instance_list.len(),
          instanced_data.map(|diff| diff.grass_instance_list.as_ref()),
        ),
      };
      (data != YmapInstancedDataDiff::default()).then_some(data)
    },
    time_cycle_modifiers: merge_items(
      "time_cycle_modifiers",
      vanilla.time_cycle_modifiers.len(),
      sources.iter().map(|diff| diff.time_cycle_modifiers.as_ref()),
    ),
    car_generators: merge_items(
      "car_generators",
      vanilla.car_generators.len(),
      sources.iter().map(|diff| diff.car_generators.as_ref()),
    ),
  }
}

/// The lists of `sources`, skipping those without changes.
fn lists<'a, T>(sources: impl Iterator<Item = &'a Option<Vec<T>>>) -> Vec<&'a [T]>
where
  T: 'a,
{
  sources.filter_map(Option::as_deref).collect()
}

fn non_empty<T>(items: Vec<T>) -> Option<Vec<T>> {
  (!items.is_empty()).then_some(items)
}

/// The item diffs of `sources` against `vanilla_len` items combined.
fn merge_items<'a, T: Clone + 'a, C: Clone + 'a>(
  what: &str,
  vanilla_len: usize,
  sources: impl Iterator<Item = Option<&'a Vec<ItemDiff<T, C>>>>,
) -> Option<Vec<ItemDiff<T, C>>> {
  let sources = sources.flatten().map(Vec::as_slice).collect::<Vec<_>>();
  let merged: MergedItems<T, C> = merged(what, &sources);
  non_empty(merged.into_diffs(vanilla_len))
}

/// [`merge_items`] for occlude models, combining the changes of a model that several sources
/// modify; see the module docs.
fn merge_occlude_models<'a>(
  vanilla: &[YmapOccludeModel],
  sources: impl Iterator<Item = Option<&'a Vec<YmapOccludeModelDiff>>>,
) -> Option<Vec<YmapOccludeModelDiff>> {
  let sources = sources.flatten().map(Vec::as_slice).collect::<Vec<_>>();
  let merged = merged_with(&sources, |index, earlier, later| {
    tracing::info!("    occlude_models: combining changes of vanilla model {index}");
    combine_occlude_model(&vanilla[index], earlier, later);
  });
  non_empty(merged.into_diffs(vanilla.len()))
}

/// Folds the `later` changes of `vanilla` into the `earlier` ones: each field keeps its earlier
/// change, and triangle changes combine as [`MergedItems`] describes.
fn combine_occlude_model(
  vanilla: &YmapOccludeModel,
  earlier: &mut YmapOccludeModelFieldDiff,
  later: &YmapOccludeModelFieldDiff,
) {
  combine_position(&mut earlier.bmin, &later.bmin);
  combine_position(&mut earlier.bmax, &later.bmax);
  earlier.flags = earlier.flags.or(later.flags);
  let triangles = [&earlier.triangles, &later.triangles]
    .into_iter()
    .filter_map(Option::as_deref)
    .collect::<Vec<_>>();
  let triangles = merged("triangles", &triangles);
  earlier.triangles = non_empty(triangles.into_diffs(vanilla.triangles.len()));
}

/// The first change of `changes`, warning about later ones to another value.
fn first_change<'a, T: PartialEq + std::fmt::Debug + Clone + 'a>(
  field: &str,
  changes: impl Iterator<Item = &'a Option<T>>,
) -> Option<T> {
  let mut first: Option<&T> = None;
  for change in changes.flatten() {
    match first {
      Some(kept) if kept != change => {
        tracing::warn!("    Conflicting {field}: keeping {kept:?}, ignoring {change:?}");
      }
      Some(_) => {}
      None => first = Some(change),
    }
  }
  first.cloned()
}

/// Each axis of the first change of `changes` changing it.
fn first_position<'a>(
  changes: impl Iterator<Item = &'a Option<PositionDiff>>
) -> Option<PositionDiff> {
  let mut combined = None;
  for change in changes {
    combine_position(&mut combined, change);
  }
  combined
}

/// Each field of the first change of `changes` changing it.
fn first_block<'a>(changes: impl Iterator<Item = &'a YmapBlockDiff>) -> Option<YmapBlockDiff> {
  changes.cloned().reduce(|earlier, later| YmapBlockDiff {
    version: earlier.version.or(later.version),
    flags: earlier.flags.or(later.flags),
    name: earlier.name.or(later.name),
    exported_by: earlier.exported_by.or(later.exported_by),
    owner: earlier.owner.or(later.owner),
    time: earlier.time.or(later.time),
  })
}

/// Fills the axes `earlier` leaves unchanged with those `later` changes.
fn combine_position(
  earlier: &mut Option<PositionDiff>,
  later: &Option<PositionDiff>,
) {
  let Some(later) = later else {
    return;
  };
  let earlier = earlier.get_or_insert_with(PositionDiff::default);
  earlier.x = earlier.x.or(later.x);
  earlier.y = earlier.y.or(later.y);
  earlier.z = earlier.z.or(later.z);
}

/// The first changed reference of `changes`, warning about later ones naming another reference.
fn first_reference<'a>(
  field: &str,
  changes: impl Iterator<Item = &'a Option<String>>,
) -> Option<String> {
  let mut first: Option<&String> = None;
  for change in changes.flatten() {
    match first {
      Some(kept) if reference_hash(kept) != reference_hash(change) => {
        tracing::warn!("    Conflicting {field}: keeping {kept}, ignoring {change}");
      }
      Some(_) => {}
      None => first = Some(change),
    }
  }
  first.cloned()
}

/// The name changes of `sources` to `vanilla` names combined as [`MergedItems`] describes,
/// skipping added names whose hash a kept or earlier added name has.
fn merge_references(
  what: &str,
  vanilla: &[String],
  sources: &[&[YmapPropInstanceDiff]],
) -> Option<Vec<YmapPropInstanceDiff>> {
  let mut merged = merged(what, sources);
  let mut seen = (0..vanilla.len())
    .filter(|index| !merged.drops_vanilla(*index))
    .map(|index| reference_hash(&vanilla[index]))
    .collect::<HashSet<_>>();
  merged.retain_new(|name| seen.insert(reference_hash(name)));
  non_empty(merged.into_diffs(vanilla.len()))
}
