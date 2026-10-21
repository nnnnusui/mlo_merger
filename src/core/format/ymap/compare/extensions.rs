//! Entity extensions, the `#[diff(with)]` strategy of `YmapEntity::extensions`.
//!
//! An entity has at most one extension in practice, and merging keeps one source's changes of
//! an entity, so its extensions are compared as a whole: when any number in them differs by the
//! tolerance or more, or anything else differs, the change is all of the source's extensions.
//! Re-exporting tools move light positions and planes by less than the tolerance; those
//! extensions stay unchanged.

use struct_diff::ApplyError;

use crate::{
  common::position::Position,
  core::format::ymap::model::{
    YmapCapsuleBound, YmapDoor, YmapExtension, YmapLightAttr, YmapLightEffect,
    YmapSpawnPointOverride, YmapVerletClothCustomBounds,
  },
};

pub fn diff(
  before: &Vec<YmapExtension>,
  after: &Vec<YmapExtension>,
  tolerance: &f32,
) -> Option<Vec<YmapExtension>> {
  (!before.close_to(after, *tolerance)).then(|| after.clone())
}

pub fn apply(
  target: &mut Vec<YmapExtension>,
  change: &Vec<YmapExtension>,
) -> Result<(), ApplyError> {
  target.clone_from(change);
  Ok(())
}

/// Equality with numbers differing by less than a tolerance.
trait CloseTo {
  fn close_to(
    &self,
    other: &Self,
    tolerance: f32,
  ) -> bool;
}

impl CloseTo for f32 {
  fn close_to(
    &self,
    other: &Self,
    tolerance: f32,
  ) -> bool {
    self == other || (self - other).abs() < tolerance
  }
}

macro_rules! exact {
  ($($type:ty),*) => {$(
    impl CloseTo for $type {
      fn close_to(&self, other: &Self, _: f32) -> bool {
        self == other
      }
    }
  )*};
}
exact!(bool, u8, i16, u32, String);

impl<T: CloseTo, const N: usize> CloseTo for [T; N] {
  fn close_to(
    &self,
    other: &Self,
    tolerance: f32,
  ) -> bool {
    self.iter().zip(other).all(|(a, b)| a.close_to(b, tolerance))
  }
}

impl<T: CloseTo> CloseTo for Vec<T> {
  fn close_to(
    &self,
    other: &Self,
    tolerance: f32,
  ) -> bool {
    self.len() == other.len() && self.iter().zip(other).all(|(a, b)| a.close_to(b, tolerance))
  }
}

/// Compares the listed fields of a structure.
macro_rules! fields {
  ($type:ty { $($field:ident),* $(,)? }) => {
    impl CloseTo for $type {
      fn close_to(&self, other: &Self, tolerance: f32) -> bool {
        $(self.$field.close_to(&other.$field, tolerance))&&*
      }
    }
  };
}

fields!(Position {
  x,
  y,
  z
});
fields!(YmapLightEffect {
  name,
  offset_position,
  instances
});
fields!(YmapLightAttr {
  posn,
  colour,
  flashiness,
  intensity,
  flags,
  bone_tag,
  light_type,
  group_id,
  time_flags,
  falloff,
  falloff_exponent,
  culling_plane,
  shadow_blur,
  padding_1,
  padding_2,
  padding_3,
  vol_intensity,
  vol_size_scale,
  vol_outer_colour,
  light_hash,
  vol_outer_intensity,
  corona_size,
  vol_outer_exponent,
  light_fade_distance,
  shadow_fade_distance,
  specular_fade_distance,
  volumetric_fade_distance,
  shadow_near_clip,
  corona_intensity,
  corona_z_bias,
  direction,
  tangent,
  cone_inner_angle,
  cone_outer_angle,
  extents,
  projected_texture_key,
});
fields!(YmapSpawnPointOverride {
  name,
  offset_position,
  scenario_type,
  i_time_start_override,
  i_time_end_override,
  group,
  model_set,
  availability_in_mp_sp,
  flags,
  radius,
  time_till_ped_leaves,
});
fields!(YmapDoor {
  name,
  offset_position,
  enable_limit_angle,
  starts_locked,
  can_break,
  limit_angle,
  door_target_ratio,
  audio_hash,
});
fields!(YmapVerletClothCustomBounds {
  name,
  collision_data
});
fields!(YmapCapsuleBound {
  owner_name,
  rotation,
  position,
  normal,
  capsule_radius,
  capsule_len,
  capsule_half_height,
  capsule_half_width,
  flags,
});

impl CloseTo for YmapExtension {
  fn close_to(
    &self,
    other: &Self,
    tolerance: f32,
  ) -> bool {
    match (self, other) {
      (Self::LightEffect(a), Self::LightEffect(b)) => a.close_to(b, tolerance),
      (Self::SpawnPointOverride(a), Self::SpawnPointOverride(b)) => a.close_to(b, tolerance),
      (Self::Door(a), Self::Door(b)) => a.close_to(b, tolerance),
      (Self::VerletClothCustomBounds(a), Self::VerletClothCustomBounds(b)) => {
        a.close_to(b, tolerance)
      }
      _ => false,
    }
  }
}
