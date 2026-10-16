//! Pairs entities by GUID and records how a modded YMAP's entities differ from vanilla.

use serde::{Deserialize, Serialize};
use structdiff::StructDiff;

use super::{PositionDiff, RotationDiff, YmapComparisonData};
use crate::{
  common::{position::PositionStructDiffEnum, rotation::RotationStructDiffEnum},
  core::format::ymap::model::{Ymap, YmapEntity, ymap_entity::YmapEntityStructDiffEnum},
};

/// How one entity of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapEntityDiff {
  /// Present only in the modded YMAP, at `index` of its entities.
  Added {
    index: usize,
    entity: YmapEntity,
  },
  /// Present only in vanilla, at `index` of its entities.
  Removed {
    index: usize,
    guid: u32,
  },
  /// Paired by GUID and differing beyond the tolerance.
  Modified {
    guid: u32,
    vanilla_index: usize,
    modded_index: usize,
    changes: YmapEntityFieldDiff,
  },
}

/// Changed entity fields, each holding its new value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapEntityFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub entity_type: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub archetype_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub flags: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub guid: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub position: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub rotation: Option<RotationDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub scale_x_y: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub scale_z: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub parent_index: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub lod_dist: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub child_lod_dist: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub lod_level: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub num_children: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub priority_level: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ambient_occlusion_multiplier: Option<u8>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub artificial_ambient_occlusion: Option<u8>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub tint_value: Option<u32>,
}

/// Collects entity differences, pairing entities that share a GUID in entity order.
///
/// Entities sharing a GUID are paired one to one in order; when the counts differ, the
/// unpaired ones are reported as removed or added. Float fields that differ by less than
/// `tolerance` are not differences.
pub fn entity_diffs(
  vanilla: &Ymap,
  vanilla_data: &YmapComparisonData,
  modded: &Ymap,
  modded_data: &YmapComparisonData,
  tolerance: f32,
) -> Vec<YmapEntityDiff> {
  let mut diffs = Vec::new();
  let mut seen = std::collections::HashSet::new();
  let guids = vanilla.entities.iter().chain(&modded.entities).map(|entity| entity.guid);
  for guid in guids.filter(|guid| seen.insert(*guid)) {
    let vanilla_indices =
      vanilla_data.entity_indices_by_guid.get(&guid).map_or(&[][..], Vec::as_slice);
    let modded_indices =
      modded_data.entity_indices_by_guid.get(&guid).map_or(&[][..], Vec::as_slice);
    for (&vanilla_index, &modded_index) in vanilla_indices.iter().zip(modded_indices) {
      let changes = YmapEntityFieldDiff::between(
        &vanilla.entities[vanilla_index],
        &modded.entities[modded_index],
        tolerance,
      );
      if changes != YmapEntityFieldDiff::default() {
        diffs.push(YmapEntityDiff::Modified {
          guid,
          vanilla_index,
          modded_index,
          changes,
        });
      }
    }
    let paired = vanilla_indices.len().min(modded_indices.len());
    diffs.extend(vanilla_indices[paired..].iter().map(|&index| YmapEntityDiff::Removed {
      index,
      guid,
    }));
    diffs.extend(modded_indices[paired..].iter().map(|&index| YmapEntityDiff::Added {
      index,
      entity: modded.entities[index].clone(),
    }));
  }
  diffs
}

impl YmapEntityFieldDiff {
  /// Fields of `after` changed from `before`, ignoring float changes below `tolerance`.
  pub fn between(
    before: &YmapEntity,
    after: &YmapEntity,
    tolerance: f32,
  ) -> Self {
    let changed = |from: f32, to: f32| ((from - to).abs() >= tolerance).then_some(to);
    let mut fields = Self::default();
    for diff in before.diff(after) {
      use YmapEntityStructDiffEnum as Field;
      match diff {
        Field::entity_type(value) => fields.entity_type = Some(value),
        Field::archetype_name(value) => fields.archetype_name = Some(value),
        Field::flags(value) => fields.flags = Some(value),
        Field::guid(value) => fields.guid = Some(value),
        Field::position(axes) => {
          let axes = axes
            .into_iter()
            .filter(|axis| match *axis {
              PositionStructDiffEnum::x(to) => changed(before.position.x, to).is_some(),
              PositionStructDiffEnum::y(to) => changed(before.position.y, to).is_some(),
              PositionStructDiffEnum::z(to) => changed(before.position.z, to).is_some(),
            })
            .collect::<Vec<_>>();
          fields.position = (!axes.is_empty()).then(|| PositionDiff::from_struct_diffs(axes));
        }
        Field::rotation(axes) => {
          let axes = axes
            .into_iter()
            .filter(|axis| match *axis {
              RotationStructDiffEnum::x(to) => changed(before.rotation.x, to).is_some(),
              RotationStructDiffEnum::y(to) => changed(before.rotation.y, to).is_some(),
              RotationStructDiffEnum::z(to) => changed(before.rotation.z, to).is_some(),
              RotationStructDiffEnum::w(to) => changed(before.rotation.w, to).is_some(),
            })
            .collect::<Vec<_>>();
          fields.rotation = (!axes.is_empty()).then(|| RotationDiff::from_struct_diffs(axes));
        }
        Field::scale_x_y(to) => fields.scale_x_y = changed(before.scale_x_y, to),
        Field::scale_z(to) => fields.scale_z = changed(before.scale_z, to),
        Field::parent_index(value) => fields.parent_index = Some(value),
        Field::lod_dist(to) => fields.lod_dist = changed(before.lod_dist, to),
        Field::child_lod_dist(to) => fields.child_lod_dist = changed(before.child_lod_dist, to),
        Field::lod_level(value) => fields.lod_level = Some(value),
        Field::num_children(value) => fields.num_children = Some(value),
        Field::priority_level(value) => fields.priority_level = Some(value),
        Field::ambient_occlusion_multiplier(value) => {
          fields.ambient_occlusion_multiplier = Some(value)
        }
        Field::artificial_ambient_occlusion(value) => {
          fields.artificial_ambient_occlusion = Some(value)
        }
        Field::tint_value(value) => fields.tint_value = Some(value),
      }
    }
    fields
  }
}
