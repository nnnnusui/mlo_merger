//! Combining several resources' versions of a YMAP that no vanilla YMAP is named after.
//!
//! Each version is compared with the empty YMAP, so everything in it is an addition, and merging
//! them would place what they share once per version. [`thin_additions`] leaves out the
//! additions of later sources that an earlier source already makes:
//!
//! - An entity is the same as one an earlier source adds when it has the same non-zero GUID,
//!   whether or not they differ otherwise, or the same type, archetype and LOD level at the same
//!   position, within the `ymap` matching tolerance, whatever its rotation, scale and GUID. The
//!   earlier one is kept. A left-out entity with another non-zero GUID is listed in
//!   [`Thinned::aliases`], so that children pointing at it, in its YMAP or another, point at the
//!   kept one; one the kept entity cannot stand in for, as it has no GUID, is kept.
//! - A box occluder, an occlude model, a grass batch, a time-cycle modifier or a car generator
//!   is left out when it equals one an earlier source adds within the matching tolerances
//!   ([`struct_diff::Diff::same_as`]).
//! - LOD lights and distant LOD lights are kept: they pair up by index across the two YMAPs of
//!   a LOD light pair, which one YMAP alone cannot keep in step. Physics dictionaries and prop
//!   instances skip names already present when merged anyway.
//!
//! [`union_extents`] gives the merged YMAP extents covering every version's.

use std::collections::HashSet;

use struct_diff::Diff;

use super::{ItemDiff, PositionDiff, YmapDiffFields, YmapEntityDiff, instanced_data};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::{model::YmapEntity, reference_hash},
};

/// What [`thin_additions`] left out.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Thinned {
  /// How many additions it left out.
  pub left_out: usize,
  /// For each entity left out in favour of one with another GUID, its GUID and the kept one's.
  pub aliases: Vec<(u32, u32)>,
}

/// Leaves out the additions of each of `sources`, in merge order, that an earlier source makes
/// too; see the module docs.
pub fn thin_additions(
  sources: &mut [YmapDiffFields],
  tolerances: &MatchTolerances,
) -> Thinned {
  let mut thinned = thin_entities(sources, tolerances);
  let mut left_out = 0;
  left_out += thin_items(
    "box_occluders",
    sources,
    |diff| Some(&mut diff.box_occluders),
    |a, b| a.same_as(b, tolerances),
  );
  left_out += thin_items(
    "occlude_models",
    sources,
    |diff| Some(&mut diff.occlude_models),
    |a, b| a.same_as(b, tolerances),
  );
  left_out += thin_items(
    "time_cycle_modifiers",
    sources,
    |diff| Some(&mut diff.time_cycle_modifiers),
    |a, b| a.same_as(b, tolerances),
  );
  left_out += thin_items(
    "car_generators",
    sources,
    |diff| Some(&mut diff.car_generators),
    |a, b| a.same_as(b, tolerances),
  );
  left_out += thin_items(
    "grass_instance_list",
    sources,
    |diff| diff.instanced_data.as_mut().map(|data| &mut data.grass_instance_list),
    |a, b| {
      instanced_data::grass_batches::diff(
        std::slice::from_ref(a),
        std::slice::from_ref(b),
        &tolerances.ymap_grass_position,
      )
      .is_none()
    },
  );
  thinned.left_out += left_out;
  thinned
}

/// Whether `entity` is placed as `other` is: the same type, archetype and LOD level, at the
/// same position within `tolerance`.
fn placed_alike(
  entity: &YmapEntity,
  other: &YmapEntity,
  tolerance: f32,
) -> bool {
  entity.lod_level == other.lod_level
    && reference_hash(&entity.entity_type) == reference_hash(&other.entity_type)
    && reference_hash(&entity.archetype_name) == reference_hash(&other.archetype_name)
    && entity.position.same_as(&other.position, &tolerance)
}

/// Thins the added entities of `sources`; see the module docs.
fn thin_entities(
  sources: &mut [YmapDiffFields],
  tolerances: &MatchTolerances,
) -> Thinned {
  let mut guids = HashSet::new();
  let mut earlier = Vec::<YmapEntity>::new();
  let mut thinned = Thinned::default();
  for source in sources {
    let Some(entities) = &mut source.entities else {
      continue;
    };
    let mut kept = Vec::new();
    entities.retain(|diff| {
      let YmapEntityDiff::Added {
        entity,
        ..
      } = diff
      else {
        return true;
      };
      let same = if entity.guid != 0 && guids.contains(&entity.guid) {
        Some(entity.guid)
      } else {
        earlier
          .iter()
          .find(|other| placed_alike(entity, other, tolerances.ymap))
          .map(|other| other.guid)
      };
      let left_out = match same {
        Some(guid) if guid == entity.guid || entity.guid == 0 => true,
        // Children may point at it by its GUID, so it is left out only for one that has one.
        Some(0) => {
          tracing::info!(
            "    entities: entity {} ({}) is placed as one an earlier source adds without a GUID; keeping it",
            entity.guid,
            entity.archetype_name
          );
          false
        }
        Some(guid) => {
          thinned.aliases.push((entity.guid, guid));
          true
        }
        None => false,
      };
      if left_out {
        tracing::info!(
          "    entities: an earlier source adds entity {} ({}) already; leaving it out",
          entity.guid,
          entity.archetype_name
        );
        thinned.left_out += 1;
      } else {
        kept.push(entity.clone());
      }
      !left_out
    });
    if entities.is_empty() {
      source.entities = None;
    }
    guids.extend(kept.iter().map(|entity| entity.guid).filter(|guid| *guid != 0));
    earlier.extend(kept);
  }
  thinned
}

/// Thins the additions to one array of `sources`, which `list` picks out of each diff, leaving
/// out an item when `same` holds for it and an item an earlier source adds.
fn thin_items<T: Clone, C>(
  what: &str,
  sources: &mut [YmapDiffFields],
  mut list: impl FnMut(&mut YmapDiffFields) -> Option<&mut Option<Vec<ItemDiff<T, C>>>>,
  same: impl Fn(&T, &T) -> bool,
) -> usize {
  let mut earlier = Vec::<T>::new();
  let mut left_out = 0;
  for source in sources {
    let Some(diffs) = list(source) else {
      continue;
    };
    let Some(items) = diffs else {
      continue;
    };
    let mut kept = Vec::new();
    items.retain(|diff| {
      let ItemDiff::Added {
        item,
        ..
      } = diff
      else {
        return true;
      };
      if earlier.iter().any(|other| same(other, item)) {
        left_out += 1;
        return false;
      }
      kept.push(item.clone());
      true
    });
    if items.is_empty() {
      *diffs = None;
    }
    earlier.extend(kept);
  }
  if left_out > 0 {
    tracing::info!("    {what}: {left_out} additions an earlier source makes too are left out");
  }
  left_out
}

/// Sets the extents of the first of `sources` to the smallest covering those of every source,
/// and clears the others', so merging keeps them; each source's extents are its changes from
/// the empty YMAP's, all zero.
pub fn union_extents(sources: &mut [YmapDiffFields]) {
  let union = |pick: fn(&YmapDiffFields) -> &Option<PositionDiff>, min: bool| {
    let axis = |value: fn(&PositionDiff) -> Option<f32>| {
      let values = sources.iter().map(|diff| pick(diff).as_ref().and_then(value).unwrap_or(0.0));
      let value = if min {
        values.fold(f32::INFINITY, f32::min)
      } else {
        values.fold(f32::NEG_INFINITY, f32::max)
      };
      (value != 0.0).then_some(value)
    };
    let position = PositionDiff {
      x: axis(|position| position.x),
      y: axis(|position| position.y),
      z: axis(|position| position.z),
    };
    (position != PositionDiff::default()).then_some(position)
  };
  if sources.len() < 2 {
    return;
  }
  let streaming_min = union(|diff| &diff.streaming_extents_min, true);
  let streaming_max = union(|diff| &diff.streaming_extents_max, false);
  let entities_min = union(|diff| &diff.entities_extents_min, true);
  let entities_max = union(|diff| &diff.entities_extents_max, false);
  for source in sources.iter_mut() {
    source.streaming_extents_min = None;
    source.streaming_extents_max = None;
    source.entities_extents_min = None;
    source.entities_extents_max = None;
  }
  let first = &mut sources[0];
  first.streaming_extents_min = streaming_min;
  first.streaming_extents_max = streaming_max;
  first.entities_extents_min = entities_min;
  first.entities_extents_max = entities_max;
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::ymap::model::YmapBoxOccluder;

  fn occluder(i_cos_z: i32) -> YmapBoxOccluder {
    YmapBoxOccluder {
      i_center_x: 4,
      i_center_y: 8,
      i_center_z: 0,
      i_cos_z,
      i_length: 4,
      i_width: 4,
      i_height: 4,
      i_sin_z: 0,
    }
  }

  fn adding(items: Vec<YmapBoxOccluder>) -> YmapDiffFields {
    YmapDiffFields {
      box_occluders: Some(
        items
          .into_iter()
          .map(|item| ItemDiff::Added {
            after: None,
            item,
          })
          .collect(),
      ),
      ..Default::default()
    }
  }

  #[test]
  fn additions_equal_within_the_tolerances_to_an_earlier_sources_are_left_out() {
    let tolerances = MatchTolerances::default();
    // Within the box occluder rotation tolerance of the first source's, beyond it, and one the
    // third source has twice, which is kept twice.
    let mut sources = vec![
      adding(vec![occluder(100)]),
      adding(vec![occluder(101), occluder(110)]),
      adding(vec![occluder(100), occluder(120), occluder(120)]),
    ];
    assert_eq!(thin_additions(&mut sources, &tolerances).left_out, 2);
    assert_eq!(sources[1], adding(vec![occluder(110)]));
    assert_eq!(sources[2], adding(vec![occluder(120), occluder(120)]));

    // A source left with no additions has none.
    let mut sources = vec![adding(vec![occluder(100)]), adding(vec![occluder(100)])];
    thin_additions(&mut sources, &tolerances);
    assert_eq!(sources[1], YmapDiffFields::default());
  }

  #[test]
  fn entities_placed_alike_are_left_out_for_the_earlier_ones() {
    let sample: crate::core::format::ymap::model::Ymap =
      crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap");
    let entity = |guid: u32, x: f32, lod_level: &str| {
      let mut entity = sample.entities[0].clone();
      entity.guid = guid;
      entity.position.x = x;
      entity.lod_level = lod_level.into();
      entity
    };
    let adding = |entities: Vec<YmapEntity>| YmapDiffFields {
      entities: Some(
        entities
          .into_iter()
          .map(|entity| YmapEntityDiff::Added {
            after: None,
            entity,
            parent: super::super::EntityParent::None,
          })
          .collect(),
      ),
      ..Default::default()
    };
    let hd = "LODTYPES_DEPTH_HD";
    let mut sources = vec![
      adding(vec![entity(1, 0.0, hd), entity(0, 10.0, hd)]),
      // Placed as the first's within the tolerance, as another LOD level, elsewhere, and placed
      // as an entity without a GUID, which cannot stand in for it.
      adding(vec![
        entity(2, 0.0005, hd),
        entity(3, 0.0, "LODTYPES_DEPTH_LOD"),
        entity(4, 5.0, hd),
        entity(5, 10.0, hd),
      ]),
    ];
    let thinned = thin_additions(&mut sources, &MatchTolerances::default());
    assert_eq!(
      thinned,
      Thinned {
        left_out: 1,
        aliases: vec![(2, 1)],
      }
    );
    assert_eq!(
      sources[1],
      adding(vec![entity(3, 0.0, "LODTYPES_DEPTH_LOD"), entity(4, 5.0, hd), entity(5, 10.0, hd)])
    );
  }

  #[test]
  fn merged_extents_cover_every_sources() {
    let extents = |min: f32, max: f32| YmapDiffFields {
      streaming_extents_min: Some(PositionDiff {
        x: Some(min),
        y: None,
        z: None,
      }),
      streaming_extents_max: Some(PositionDiff {
        x: Some(max),
        y: Some(max),
        z: None,
      }),
      ..Default::default()
    };
    let mut sources = vec![extents(-10.0, 5.0), extents(-2.0, 20.0)];
    union_extents(&mut sources);
    assert_eq!(sources[0], extents(-10.0, 20.0));
    assert_eq!(sources[1], YmapDiffFields::default());
  }
}
