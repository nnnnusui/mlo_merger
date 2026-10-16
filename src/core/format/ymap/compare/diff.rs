//! JSON documents describing how a source YMAP differs from vanilla.
//!
//! Every field is optional and omitted when unchanged, so the documents list only changes.

use serde::{Deserialize, Serialize};
use structdiff::StructDiff;

use super::{
  YmapBoxOccluderDiff, YmapCarGeneratorDiff, YmapComparisonData, YmapDistantLodLightsDiff,
  YmapEntityDiff, YmapInstancedDataDiff, YmapItemDiff, YmapLodLightDiff, YmapOccludeModelDiff,
  YmapTimeCycleModifierDiff, apply_entity_diffs, box_occluder_diffs, car_generator_diffs,
  distant_lod_lights_diff, entity_diffs, field::set_changed, instanced_data_diff, lod_light_diffs,
  occlude_model_diffs, time_cycle_modifier_diffs,
};
use crate::{
  common::{
    Result,
    position::{Position, PositionStructDiffEnum},
    rotation::{Rotation, RotationStructDiffEnum},
  },
  core::{
    config::matching::MatchTolerances,
    format::ymap::model::{
      Ymap, YmapBlock, YmapStructDiffEnum, ymap_block::YmapBlockStructDiffEnum,
    },
  },
};

/// Differences of one source YMAP from the vanilla YMAP of the same name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonDiff {
  /// The source YMAP the differences were taken from.
  pub source: YmapComparisonFile,
  /// The vanilla YMAP used as the baseline.
  pub vanilla: YmapComparisonFile,
  pub diffs: YmapDiffFields,
}

/// A compared YMAP file and its fingerprint when the diff was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonFile {
  pub path: String,
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

/// Changed YMAP fields, each holding its new value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapDiffFields {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub parent: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub flags: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub content_flags: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub streaming_extents: Option<ExtentsDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub entities_extents: Option<ExtentsDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub block: Option<YmapBlockDiff>,
  /// Entity differences, paired by GUID.
  // `default` comes last so specta marks the field optional despite `Vec::is_empty`.
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub entities: Vec<YmapEntityDiff>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub box_occluders: Vec<YmapBoxOccluderDiff>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub occlude_models: Vec<YmapOccludeModelDiff>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub lod_lights: Vec<YmapLodLightDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub distant_lod_lights: Option<YmapDistantLodLightsDiff>,
  /// Physics dictionaries the source adds; removals are not recorded.
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub physics_dictionaries: Vec<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub instanced_data: Option<YmapInstancedDataDiff>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub time_cycle_modifiers: Vec<YmapTimeCycleModifierDiff>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub car_generators: Vec<YmapCarGeneratorDiff>,
}

/// Changed corners of an extents box.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ExtentsDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub min: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub max: Option<PositionDiff>,
}

/// Changed position axes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PositionDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub x: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub y: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub z: Option<f32>,
}

/// Changed quaternion components.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct RotationDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub x: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub y: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub z: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub w: Option<f32>,
}

/// Changed block metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapBlockDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub version: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub flags: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub exported_by: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub owner: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub time: Option<String>,
}

/// Physics dictionaries `modded` lists that `vanilla` does not, in `modded` order.
///
/// Their order carries no meaning (CodeWalker only checks membership), and removals are not
/// recorded.
pub fn added_physics_dictionaries(
  vanilla: &Ymap,
  modded: &Ymap,
) -> Vec<String> {
  modded
    .physics_dictionaries
    .iter()
    .filter(|name| !vanilla.physics_dictionaries.contains(name))
    .cloned()
    .collect()
}

impl YmapDiffFields {
  /// Every difference of `modded` from `vanilla`, each YMAP given with its comparison data.
  pub fn between(
    (vanilla, vanilla_data): (&Ymap, &YmapComparisonData),
    (modded, modded_data): (&Ymap, &YmapComparisonData),
    tolerances: &MatchTolerances,
  ) -> Self {
    Self {
      entities: entity_diffs(vanilla, vanilla_data, modded, modded_data, tolerances.ymap),
      box_occluders: box_occluder_diffs(vanilla, modded, tolerances),
      occlude_models: occlude_model_diffs(vanilla, modded, tolerances),
      lod_lights: lod_light_diffs(vanilla, modded),
      distant_lod_lights: distant_lod_lights_diff(vanilla, modded, tolerances),
      physics_dictionaries: added_physics_dictionaries(vanilla, modded),
      instanced_data: instanced_data_diff(vanilla, modded, tolerances),
      time_cycle_modifiers: time_cycle_modifier_diffs(vanilla, modded, tolerances),
      car_generators: car_generator_diffs(vanilla, modded, tolerances),
      ..Self::from_struct_diffs(vanilla.diff(modded))
    }
  }

  /// The modded YMAP rebuilt from `vanilla` and these differences.
  ///
  /// Differences within the tolerances of [`Self::between`] are not recorded, so the result
  /// equals the modded YMAP only up to them. The diffs also leave out where unchanged items
  /// moved, where added occlude model triangles sit and which physics dictionaries were
  /// removed: unchanged items keep their vanilla order, added triangles are appended and every
  /// vanilla physics dictionary is kept.
  pub fn apply_to(
    &self,
    vanilla: &Ymap,
  ) -> Result<Ymap> {
    let Self {
      name,
      parent,
      flags,
      content_flags,
      streaming_extents,
      entities_extents,
      block,
      entities,
      box_occluders,
      occlude_models,
      lod_lights,
      distant_lod_lights,
      physics_dictionaries,
      instanced_data,
      time_cycle_modifiers,
      car_generators,
    } = self;
    let mut ymap = vanilla.clone();
    set_changed(&mut ymap.name, name);
    set_changed(&mut ymap.parent, parent);
    set_changed(&mut ymap.flags, flags);
    set_changed(&mut ymap.content_flags, content_flags);
    if let Some(extents) = streaming_extents {
      extents.apply_to(&mut ymap.streaming_extents_min, &mut ymap.streaming_extents_max);
    }
    if let Some(extents) = entities_extents {
      extents.apply_to(&mut ymap.entities_extents_min, &mut ymap.entities_extents_max);
    }
    if let Some(block) = block {
      block.apply_to(&mut ymap.block);
    }
    ymap.entities = apply_entity_diffs(&ymap.entities, entities)?;
    ymap.box_occluders =
      YmapItemDiff::apply("box_occluders", &ymap.box_occluders, box_occluders, |item, changes| {
        changes.apply_to(item)
      })?;
    ymap.occlude_models = YmapItemDiff::try_apply(
      "occlude_models",
      &ymap.occlude_models,
      occlude_models,
      |item, changes| changes.apply_to(item),
    )?;
    ymap.lod_lights =
      YmapItemDiff::apply("lod_lights", &ymap.lod_lights, lod_lights, |item, changes| {
        changes.apply_to(item)
      })?;
    if let Some(distant_lod_lights) = distant_lod_lights {
      distant_lod_lights.apply_to(&mut ymap.distant_lod_lights)?;
    }
    for name in physics_dictionaries {
      if !ymap.physics_dictionaries.contains(name) {
        ymap.physics_dictionaries.push(name.clone());
      }
    }
    if let Some(instanced_data) = instanced_data {
      instanced_data.apply_to(&mut ymap.instanced_data)?;
    }
    ymap.time_cycle_modifiers = YmapItemDiff::apply(
      "time_cycle_modifiers",
      &ymap.time_cycle_modifiers,
      time_cycle_modifiers,
      |item, changes| changes.apply_to(item),
    )?;
    ymap.car_generators = YmapItemDiff::apply(
      "car_generators",
      &ymap.car_generators,
      car_generators,
      |item, changes| changes.apply_to(item),
    )?;
    Ok(ymap)
  }

  /// Collects structdiff field differences of a YMAP, leaving entities empty.
  pub fn from_struct_diffs(diffs: Vec<YmapStructDiffEnum>) -> Self {
    let mut fields = Self::default();
    for diff in diffs {
      match diff {
        YmapStructDiffEnum::name(value) => fields.name = Some(value),
        YmapStructDiffEnum::parent(value) => fields.parent = Some(value),
        YmapStructDiffEnum::flags(value) => fields.flags = Some(value),
        YmapStructDiffEnum::content_flags(value) => fields.content_flags = Some(value),
        YmapStructDiffEnum::streaming_extents_min(axes) => {
          fields.streaming_extents.get_or_insert_default().min =
            Some(PositionDiff::from_struct_diffs(axes));
        }
        YmapStructDiffEnum::streaming_extents_max(axes) => {
          fields.streaming_extents.get_or_insert_default().max =
            Some(PositionDiff::from_struct_diffs(axes));
        }
        YmapStructDiffEnum::entities_extents_min(axes) => {
          fields.entities_extents.get_or_insert_default().min =
            Some(PositionDiff::from_struct_diffs(axes));
        }
        YmapStructDiffEnum::entities_extents_max(axes) => {
          fields.entities_extents.get_or_insert_default().max =
            Some(PositionDiff::from_struct_diffs(axes));
        }
        YmapStructDiffEnum::block(block) => {
          fields.block = Some(YmapBlockDiff::from_struct_diffs(block))
        }
      }
    }
    fields
  }
}

impl ExtentsDiff {
  /// Overwrites the changed axes of the `min` and `max` corners.
  pub fn apply_to(
    &self,
    min: &mut Position,
    max: &mut Position,
  ) {
    let Self {
      min: min_diff,
      max: max_diff,
    } = self;
    if let Some(diff) = min_diff {
      diff.apply_to(min);
    }
    if let Some(diff) = max_diff {
      diff.apply_to(max);
    }
  }
}

impl PositionDiff {
  pub fn from_struct_diffs(diffs: Vec<PositionStructDiffEnum>) -> Self {
    let mut axes = Self::default();
    for diff in diffs {
      match diff {
        PositionStructDiffEnum::x(value) => axes.x = Some(value),
        PositionStructDiffEnum::y(value) => axes.y = Some(value),
        PositionStructDiffEnum::z(value) => axes.z = Some(value),
      }
    }
    axes
  }
}

impl RotationDiff {
  /// Overwrites the changed components of `rotation`.
  pub fn apply_to(
    &self,
    rotation: &mut Rotation,
  ) {
    let Self {
      x,
      y,
      z,
      w,
    } = self;
    set_changed(&mut rotation.x, x);
    set_changed(&mut rotation.y, y);
    set_changed(&mut rotation.z, z);
    set_changed(&mut rotation.w, w);
  }

  pub fn from_struct_diffs(diffs: Vec<RotationStructDiffEnum>) -> Self {
    let mut axes = Self::default();
    for diff in diffs {
      match diff {
        RotationStructDiffEnum::x(value) => axes.x = Some(value),
        RotationStructDiffEnum::y(value) => axes.y = Some(value),
        RotationStructDiffEnum::z(value) => axes.z = Some(value),
        RotationStructDiffEnum::w(value) => axes.w = Some(value),
      }
    }
    axes
  }
}

impl YmapBlockDiff {
  /// Overwrites the changed fields of `block`.
  pub fn apply_to(
    &self,
    block: &mut YmapBlock,
  ) {
    let Self {
      version,
      flags,
      name,
      exported_by,
      owner,
      time,
    } = self;
    set_changed(&mut block.version, version);
    set_changed(&mut block.flags, flags);
    set_changed(&mut block.name, name);
    set_changed(&mut block.exported_by, exported_by);
    set_changed(&mut block.owner, owner);
    set_changed(&mut block.time, time);
  }

  pub fn from_struct_diffs(diffs: Vec<YmapBlockStructDiffEnum>) -> Self {
    let mut block = Self::default();
    for diff in diffs {
      match diff {
        YmapBlockStructDiffEnum::version(value) => block.version = Some(value),
        YmapBlockStructDiffEnum::flags(value) => block.flags = Some(value),
        YmapBlockStructDiffEnum::name(value) => block.name = Some(value),
        YmapBlockStructDiffEnum::exported_by(value) => block.exported_by = Some(value),
        YmapBlockStructDiffEnum::owner(value) => block.owner = Some(value),
        YmapBlockStructDiffEnum::time(value) => block.time = Some(value),
      }
    }
    block
  }
}
