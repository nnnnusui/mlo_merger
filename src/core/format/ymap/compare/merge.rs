//! Combines the differences of several sources from one vanilla YMAP into the one diff that
//! merging applies.
//!
//! Array changes combine as [`MergedItems`] describes: a removal wins over every other change of
//! the vanilla item, including another source's replacement of it; otherwise the first source to
//! modify or replace a vanilla item wins, and additions keep their place after the vanilla item
//! they follow, in source order. Occlude
//! models modified by several sources take each field from the first source changing it and
//! every source's triangle removals and additions, so cut-outs of every source survive. `content_flags`
//! are OR-ed into vanilla's, the parent and IMAP link keep the first change, and added physics
//! dictionaries and prop instances skip names already present. The name, flags, extents, block
//! and the distant LOD light counts keep their vanilla values.

use std::collections::HashSet;

use super::{
  ItemDiff, PositionDiff, YmapDiffFields, YmapDistantLodLightsDiff, YmapInstancedDataDiff,
  YmapOccludeModelDiff, YmapOccludeModelFieldDiff, entities,
  merged_items::{MergedItems, merged, merged_with},
};
use crate::core::{
  config::blacklist::BlacklistConfig,
  format::ymap::{
    model::{Ymap, YmapOccludeModel},
    reference_hash,
  },
};

/// One diff of `vanilla` making the changes of every diff in `sources`, taken from `vanilla` in
/// merge order; added occlude models that `blacklist` lists are dropped.
pub fn merge_diffs(
  vanilla: &Ymap,
  sources: &[YmapDiffFields],
  blacklist: Option<&BlacklistConfig>,
) -> YmapDiffFields {
  for source in sources {
    log_ignored(source);
  }
  let content_flags =
    sources.iter().filter_map(|diff| diff.content_flags).fold(vanilla.content_flags, |a, b| a | b);
  let instanced_data = sources.iter().filter_map(|diff| diff.instanced_data.as_ref());
  YmapDiffFields {
    content_flags: (content_flags != vanilla.content_flags).then_some(content_flags),
    parent: first_reference("parent", sources.iter().map(|diff| &diff.parent)),
    entities: non_empty(entities::merge(
      &vanilla.entities,
      &lists(sources.iter().map(|diff| &diff.entities)),
    )),
    box_occluders: merge_items(
      "box_occluders",
      vanilla.box_occluders.len(),
      sources.iter().map(|diff| diff.box_occluders.as_ref()),
      |_| true,
    ),
    occlude_models: merge_occlude_models(
      &vanilla.occlude_models,
      sources.iter().map(|diff| diff.occlude_models.as_ref()),
      |model| {
        let listed =
          blacklist.is_some_and(|list| list.is_occlude_model_blacklisted(&model.bmin, &model.bmax));
        if listed {
          tracing::info!(
            "      [Blacklisted] Occlude Model: bmin={:?}, bmax={:?}",
            model.bmin,
            model.bmax
          );
        }
        !listed
      },
    ),
    lod_lights: merge_items(
      "lod_lights",
      vanilla.lod_lights.len(),
      sources.iter().map(|diff| diff.lod_lights.as_ref()),
      |_| true,
    ),
    distant_lod_lights: merge_items(
      "distant_lod_lights",
      vanilla.distant_lod_lights.items.len(),
      sources.iter().map(|diff| diff.distant_lod_lights.as_ref().and_then(|it| it.items.as_ref())),
      |_| true,
    )
    .map(|items| YmapDistantLodLightsDiff {
      items: Some(items),
      ..Default::default()
    }),
    physics_dictionaries: added_references(
      &vanilla.physics_dictionaries,
      sources.iter().filter_map(|diff| diff.physics_dictionaries.as_deref()).flatten(),
    ),
    instanced_data: {
      let data = YmapInstancedDataDiff {
        imap_link: first_reference("imap_link", instanced_data.clone().map(|diff| &diff.imap_link)),
        prop_instance_list: {
          let props = &vanilla.instanced_data.prop_instance_list;
          let mut merged = merged(
            "prop_instance_list",
            &lists(instanced_data.clone().map(|diff| &diff.prop_instance_list)),
          );
          let mut seen = (0..props.len())
            .filter(|index| !merged.drops_vanilla(*index))
            .map(|index| reference_hash(&props[index]))
            .collect::<HashSet<_>>();
          merged.retain_new(|name| seen.insert(reference_hash(name)));
          non_empty(merged.into_diffs(props.len()))
        },
        grass_instance_list: merge_items(
          "grass_instance_list",
          vanilla.instanced_data.grass_instance_list.len(),
          instanced_data.map(|diff| diff.grass_instance_list.as_ref()),
          |_| true,
        ),
      };
      (data != YmapInstancedDataDiff::default()).then_some(data)
    },
    time_cycle_modifiers: merge_items(
      "time_cycle_modifiers",
      vanilla.time_cycle_modifiers.len(),
      sources.iter().map(|diff| diff.time_cycle_modifiers.as_ref()),
      |_| true,
    ),
    car_generators: merge_items(
      "car_generators",
      vanilla.car_generators.len(),
      sources.iter().map(|diff| diff.car_generators.as_ref()),
      |_| true,
    ),
    ..Default::default()
  }
}

/// Logs the changes of `diff` that merging leaves at their vanilla values.
fn log_ignored(diff: &YmapDiffFields) {
  let ignored = [
    ("name", diff.name.is_some()),
    ("flags", diff.flags.is_some()),
    ("streamingExtentsMin", diff.streaming_extents_min.is_some()),
    ("streamingExtentsMax", diff.streaming_extents_max.is_some()),
    ("entitiesExtentsMin", diff.entities_extents_min.is_some()),
    ("entitiesExtentsMax", diff.entities_extents_max.is_some()),
    ("block", diff.block.is_some()),
    (
      "distantLodLights counts",
      diff
        .distant_lod_lights
        .as_ref()
        .is_some_and(|lights| lights.num_street_lights.is_some() || lights.category.is_some()),
    ),
  ];
  for (field, _) in ignored.iter().filter(|(_, changed)| *changed) {
    tracing::info!("    skip changes: <{field} />");
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

/// The item diffs of `sources` against `vanilla_len` items combined, keeping only additions that
/// `keep` accepts.
fn merge_items<'a, T: Clone + 'a, C: Clone + 'a>(
  what: &str,
  vanilla_len: usize,
  sources: impl Iterator<Item = Option<&'a Vec<ItemDiff<T, C>>>>,
  keep: impl FnMut(&T) -> bool,
) -> Option<Vec<ItemDiff<T, C>>> {
  let sources = sources.flatten().map(Vec::as_slice).collect::<Vec<_>>();
  let mut merged: MergedItems<T, C> = merged(what, &sources);
  merged.retain_new(keep);
  non_empty(merged.into_diffs(vanilla_len))
}

/// [`merge_items`] for occlude models, combining the changes of a model that several sources
/// modify; see the module docs.
fn merge_occlude_models<'a>(
  vanilla: &[YmapOccludeModel],
  sources: impl Iterator<Item = Option<&'a Vec<YmapOccludeModelDiff>>>,
  keep: impl FnMut(&YmapOccludeModel) -> bool,
) -> Option<Vec<YmapOccludeModelDiff>> {
  let sources = sources.flatten().map(Vec::as_slice).collect::<Vec<_>>();
  let mut merged = merged_with(&sources, |index, earlier, later| {
    tracing::info!("    occlude_models: combining changes of vanilla model {index}");
    combine_occlude_model(&vanilla[index], earlier, later);
  });
  merged.retain_new(keep);
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

/// The `added` references whose hash neither `existing` nor an earlier addition has.
fn added_references<'a>(
  existing: &[String],
  added: impl Iterator<Item = &'a String>,
) -> Option<Vec<String>> {
  let mut seen = existing.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
  non_empty(added.filter(|name| seen.insert(reference_hash(name))).cloned().collect())
}
