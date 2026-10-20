//! LOD lights and their distant LOD lights, kept in step across their two YMAPs.
//!
//! `[<prefix>_]lodlights_<suffix>.ymap` and `[<prefix>_]distlodlights_<suffix>.ymap` describe the
//! same lights: the game reads the `lod_lights` of one and the `distant_lod_lights` of the other
//! as parallel arrays, so the light at one index of either is the light at that index of the
//! other. Every vanilla pair has as many of each, but sources sometimes remove a light from only
//! one of them, after which the lights past it no longer line up.
//!
//! A light removed from either array is therefore removed from both: the removals of each side
//! are added to the other's diff as [`ItemDiff::Removed`] marked `implied`.

use std::collections::BTreeSet;

use struct_diff::Diff;

use super::{ItemDiff, YmapDiffFields, YmapDistantLodLightsDiff};
use crate::core::{config::matching::MatchTolerances, format::ymap::model::Ymap};

/// Which array of a light pair a YMAP holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightSide {
  /// `[<prefix>_]lodlights_<suffix>.ymap`, holding `lod_lights`.
  LodLights,
  /// `[<prefix>_]distlodlights_<suffix>.ymap`, holding `distant_lod_lights`.
  DistantLodLights,
}

const LOD: &str = "lodlights_";
const DISTANT: &str = "distlodlights_";

impl LightSide {
  /// The side the YMAP `file_name` holds, by its name; `None` for other YMAPs.
  pub fn of(file_name: &str) -> Option<Self> {
    Self::split(&file_name.to_ascii_lowercase()).map(|(side, ..)| side)
  }

  /// The name of the YMAP holding the other side of `file_name`'s lights, in lowercase; `None`
  /// for YMAPs of no light pair.
  pub fn partner_name(file_name: &str) -> Option<String> {
    let name = file_name.to_ascii_lowercase();
    let (side, prefix, suffix) = Self::split(&name)?;
    Some(format!("{prefix}{}{suffix}", side.other().token()))
  }

  fn token(self) -> &'static str {
    match self {
      Self::LodLights => LOD,
      Self::DistantLodLights => DISTANT,
    }
  }

  /// The side `name` (lowercase) holds, with what comes before and after its side's token, which
  /// starts the name or follows an underscore.
  fn split(name: &str) -> Option<(Self, &str, &str)> {
    [Self::DistantLodLights, Self::LodLights].into_iter().find_map(|side| {
      let token = side.token();
      name.match_indices(token).find_map(|(at, _)| {
        let prefix = &name[..at];
        (prefix.is_empty() || prefix.ends_with('_'))
          .then(|| (side, prefix, &name[at + token.len()..]))
      })
    })
  }

  /// The side of the other YMAP of the pair.
  pub fn other(self) -> Self {
    match self {
      Self::LodLights => Self::DistantLodLights,
      Self::DistantLodLights => Self::LodLights,
    }
  }

  /// How many lights of this side `ymap` holds.
  pub fn count(
    self,
    ymap: &Ymap,
  ) -> usize {
    match self {
      Self::LodLights => ymap.lod_lights.len(),
      Self::DistantLodLights => ymap.distant_lod_lights.items.len(),
    }
  }

  /// The indices of the vanilla lights of this side that `diff` removes as the source shows it,
  /// leaving out implied removals.
  pub fn removed(
    self,
    diff: &YmapDiffFields,
  ) -> BTreeSet<usize> {
    match self {
      Self::LodLights => shown_removals(diff.lod_lights.as_deref()),
      Self::DistantLodLights => {
        shown_removals(diff.distant_lod_lights.as_ref().and_then(|lights| lights.items.as_deref()))
      }
    }
  }

  /// The indices of the lights of this side that `modded` lacks against `vanilla`, as
  /// [`Self::removed`] reads them from the diff of the two.
  pub fn removed_between(
    self,
    vanilla: &Ymap,
    modded: &Ymap,
    tolerances: &MatchTolerances,
  ) -> BTreeSet<usize> {
    match self {
      Self::LodLights => {
        shown_removals(vanilla.lod_lights.diff(&modded.lod_lights, &()).as_deref())
      }
      Self::DistantLodLights => shown_removals(
        vanilla
          .distant_lod_lights
          .items
          .diff(&modded.distant_lod_lights.items, tolerances)
          .as_deref(),
      ),
    }
  }

  /// Adds to `diff`, of this side, the removal of each light in `partner_removed` that it does
  /// not remove already, marked `implied`; returns how many it added.
  ///
  /// A light the diff replaces or modifies is removed instead, as merging lets a removal win.
  pub fn imply_removals(
    self,
    diff: &mut YmapDiffFields,
    partner_removed: &BTreeSet<usize>,
  ) -> usize {
    match self {
      Self::LodLights => imply(&mut diff.lod_lights, partner_removed),
      Self::DistantLodLights => {
        let lights = diff.distant_lod_lights.get_or_insert_with(YmapDistantLodLightsDiff::default);
        let added = imply(&mut lights.items, partner_removed);
        if *lights == YmapDistantLodLightsDiff::default() {
          diff.distant_lod_lights = None;
        }
        added
      }
    }
  }
}

fn shown_removals<T, C>(diffs: Option<&[ItemDiff<T, C>]>) -> BTreeSet<usize> {
  diffs
    .into_iter()
    .flatten()
    .filter_map(|diff| match diff {
      ItemDiff::Removed {
        index,
        implied: false,
      } => Some(*index),
      _ => None,
    })
    .collect()
}

/// [`LightSide::imply_removals`] for one array's diffs. Removals come first, by index, as diffs
/// list them.
fn imply<T, C>(
  diffs: &mut Option<Vec<ItemDiff<T, C>>>,
  removed: &BTreeSet<usize>,
) -> usize {
  let current = diffs.take().unwrap_or_default();
  let changed = |diff: &ItemDiff<T, C>| match diff {
    ItemDiff::Removed {
      index,
      ..
    }
    | ItemDiff::Replaced {
      index,
      ..
    }
    | ItemDiff::Modified {
      before_index: index,
      ..
    } => Some(*index),
    ItemDiff::Added {
      ..
    } => None,
  };
  let already_removed = current
    .iter()
    .filter(|diff| matches!(diff, ItemDiff::Removed { .. }))
    .filter_map(changed)
    .collect::<BTreeSet<_>>();
  let implied = removed.difference(&already_removed).copied().collect::<BTreeSet<_>>();
  let (mut removals, rest): (Vec<_>, Vec<_>) =
    current.into_iter().partition(|diff| matches!(diff, ItemDiff::Removed { .. }));
  let rest = rest.into_iter().filter(|diff| {
    let dropped = changed(diff).is_some_and(|index| implied.contains(&index));
    if dropped {
      tracing::info!(
        "    Light {} is removed from the other array of its pair; dropping its change",
        changed(diff).unwrap_or_default()
      );
    }
    !dropped
  });
  removals.extend(implied.iter().map(|&index| ItemDiff::Removed {
    index,
    implied: true,
  }));
  removals.sort_by_key(|diff| changed(diff).unwrap_or_default());
  let merged = removals.into_iter().chain(rest).collect::<Vec<_>>();
  *diffs = (!merged.is_empty()).then_some(merged);
  implied.len()
}
