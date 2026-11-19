//! Source diffs, and the LOD light removals they imply for YMAPs their resources do not ship.
//!
//! A light removed from either YMAP of a LOD light pair is removed from both (see
//! [`lod_light_pairs`](crate::core::format::ymap::compare::lod_light_pairs)). The source cache adds
//! those removals to the diffs of resources shipping both YMAPs, and writes what else keeps a
//! source's pair in step to its LOD light pair document (`YmapImplied`): the lights it adds to one
//! YMAP only, which are left out of its diff, and, for a resource shipping only one YMAP, the
//! removals of the vanilla other one, merged into it as a diff of their own as far as the
//! source's overwrite keeps the removals they follow from.

use super::*;
use crate::core::format::ymap::compare::lod_light_pairs::LightSide;

/// The vanilla YMAP a source is diffed against: its path, model and entity parents.
pub(super) type Baseline<'a> = (&'a Path, &'a Ymap, &'a EntityParents);

/// Removals of LOD lights that resources make in one YMAP of a pair and that merging makes in
/// the vanilla other one, keyed by that one's hash: each resource with its diff.
pub(super) type ImpliedDiffs = HashMap<u32, Vec<(String, YmapDiffFields)>>;

impl MergeYmap {
  /// The differences of `mod_info`'s YMAP `source` from `baseline` with its overwrite applied:
  /// those of its source-cache diff document, or else taken here, then with the LOD light
  /// removals of the other YMAP of its pair when the resource ships that too.
  pub(super) fn source_diff(
    &self,
    mod_info: &ModYmapReference,
    source: &OriginalMap,
    (vanilla_ymap_path, vanilla_ymap, vanilla_parents): Baseline<'_>,
    sources: &mut SourceMaps,
    references: &mut ParentReferences,
    registered_maps: &HashSet<(String, u32)>,
  ) -> Result<YmapDiffFields, Box<dyn std::error::Error>> {
    let ymap_name = mod_info.ymap_name.as_str();
    let tolerances = crate::core::config::matching::current();
    let (mut ymap_diff, fingerprints) = match &mod_info.diff_path {
      Some(diff_path) => {
        let document = YmapComparisonDiff::read_current(
          diff_path,
          &mod_info.mod_ymap_path,
          vanilla_ymap_path,
          &tolerances,
        )?
        .ok_or_else(|| {
          format!(
            "{} is missing or stale for {}; rebuild the source cache",
            diff_path.display(),
            mod_info.mod_ymap_path.display()
          )
        })?;
        let fingerprints = (
          OverwriteFingerprint::from(&document.target),
          OverwriteFingerprint::from(&document.base),
        );
        let mut diffs = document.diffs;
        if let Some(implied) = self.implied_document(mod_info, &document.target, &document.base)?
          && let Some(skip) = &implied.skip
        {
          crate::app::map::overwrite::skip_diffs(&mut diffs, skip)?;
        }
        (diffs, Some(fingerprints))
      }
      // Sources outside a source cache are compared here, against the latest vanilla.
      None => {
        let ctx = YmapDiffContext {
          tolerances,
          vanilla_parents: vanilla_parents.clone(),
          modded_parents: parents_of(sources, references, Some(&mod_info.mod_name), &source.model)?,
          record_moves: false,
        };
        let mut diff = vanilla_ymap.diff(&source.model, &ctx).unwrap_or_default();
        if let Some((side, partner_hash)) = pair_of(sources, ymap_name)?
          && registered_maps.contains(&(mod_info.mod_name.clone(), partner_hash))
          && let (Some(vanilla), Some(modded)) =
            (sources.get(None, partner_hash)?, sources.get(Some(&mod_info.mod_name), partner_hash)?)
        {
          let removed = side.other().removed_between(&vanilla.model, &modded.model, &tolerances);
          side.imply_removals(&mut diff, &removed);
        }
        (diff, None)
      }
    };
    if let Some(overwrite) = self.overwrites.get(&mod_info.mod_ymap_path) {
      let (source, vanilla) = match fingerprints {
        Some(fingerprints) => fingerprints,
        None => (
          OverwriteFingerprint::read(&mod_info.mod_ymap_path)?,
          OverwriteFingerprint::read(vanilla_ymap_path)?,
        ),
      };
      let label = format!("{} ({})", ymap_name, mod_info.mod_name);
      overwrite.apply(&mut ymap_diff, &source, &vanilla, &label)?;
    }
    Ok(ymap_diff)
  }

  /// The removals each resource shipping only one YMAP of a LOD light pair makes in the vanilla
  /// other one, from the removals of its own; see the module docs.
  pub(super) fn implied_diffs(
    &self,
    modded_ymaps_map: &BTreeMap<String, Vec<ModYmapReference>>,
    vanilla_map_paths: &HashMap<u32, PathBuf>,
    sources: &mut SourceMaps,
    references: &mut ParentReferences,
    registered_maps: &HashSet<(String, u32)>,
  ) -> Result<ImpliedDiffs, Box<dyn std::error::Error>> {
    let mut implied = ImpliedDiffs::new();
    for (ymap_name, mod_refs) in modded_ymaps_map {
      let Some((side, partner_hash)) = pair_of(sources, ymap_name)? else {
        continue;
      };
      let hash = reference_hash(ymap_name.trim_end_matches(".ymap"));
      let (Some(vanilla_ymap_path), Some(original)) =
        (vanilla_map_paths.get(&hash), sources.get(None, hash)?)
      else {
        continue;
      };
      let vanilla_parents = parents_of(sources, references, None, &original.model)?;
      for mod_info in mod_refs {
        if registered_maps.contains(&(mod_info.mod_name.clone(), partner_hash)) {
          continue;
        }
        let Some(source) = sources.get(Some(&mod_info.mod_name), hash)? else {
          continue;
        };
        let diff = self.source_diff(
          mod_info,
          &source,
          (vanilla_ymap_path, &original.model, &vanilla_parents),
          sources,
          references,
          registered_maps,
        )?;
        let removed = side.removed(&diff);
        if removed.is_empty() {
          continue;
        }
        // A source of the source cache takes the removals its LOD light pair document implies.
        let partner_diff = match &mod_info.diff_path {
          Some(diff_path) => {
            let document: YmapComparisonDiff = crate::common::json::read_json(diff_path)?;
            let Some(partner) = self
              .implied_document(mod_info, &document.target, &document.base)?
              .and_then(|implied| implied.partner)
            else {
              continue;
            };
            keep_removals(partner.diffs, side.other(), &removed)
          }
          None => {
            let mut partner_diff = YmapDiffFields::default();
            side.other().imply_removals(&mut partner_diff, &removed);
            partner_diff
          }
        };
        implied.entry(partner_hash).or_default().push((mod_info.mod_name.clone(), partner_diff));
      }
    }
    Ok(implied)
  }
}

impl MergeYmap {
  /// The LOD light pair document of `mod_info`'s source, when it has one taken from the files
  /// `target` and `base` its diff document was.
  fn implied_document(
    &self,
    mod_info: &ModYmapReference,
    target: &crate::core::format::ymap::compare::YmapComparisonFile,
    base: &crate::core::format::ymap::compare::YmapComparisonFile,
  ) -> Result<Option<crate::app::map::source_cache::YmapImplied>, Box<dyn std::error::Error>> {
    let Some(path) = self.implied.get(&mod_info.mod_ymap_path) else {
      return Ok(None);
    };
    let implied: crate::app::map::source_cache::YmapImplied =
      crate::common::json::read_json(path)?;
    Ok((implied.target == *target && implied.base == *base).then_some(implied))
  }
}

/// The removals of the lights of `side` that `diffs` makes and `kept` names, marked `implied`.
fn keep_removals(
  diffs: YmapDiffFields,
  side: LightSide,
  kept: &std::collections::BTreeSet<usize>,
) -> YmapDiffFields {
  let removed = match side {
    LightSide::LodLights => diffs.lod_lights.iter().flatten().filter_map(removed_index).collect(),
    LightSide::DistantLodLights => diffs
      .distant_lod_lights
      .iter()
      .flat_map(|lights| lights.items.iter().flatten())
      .filter_map(removed_index)
      .collect::<std::collections::BTreeSet<_>>(),
  };
  let mut kept_diffs = YmapDiffFields::default();
  side.imply_removals(&mut kept_diffs, &removed.intersection(kept).copied().collect());
  kept_diffs
}

fn removed_index<T, C>(diff: &crate::core::format::ymap::compare::ItemDiff<T, C>) -> Option<usize> {
  match diff {
    crate::core::format::ymap::compare::ItemDiff::Removed {
      base_index,
      ..
    } => Some(*base_index),
    _ => None,
  }
}

/// For `ymap_name`, one YMAP of a LOD light pair, its side and the hash of the other one; `None`
/// for other YMAPs and for pairs whose vanilla YMAPs do not hold as many lights of each side,
/// the source cache leaves as they are too.
fn pair_of(
  sources: &mut SourceMaps,
  ymap_name: &str,
) -> Result<Option<(LightSide, u32)>, Box<dyn std::error::Error>> {
  let (Some(side), Some(partner)) = (LightSide::of(ymap_name), LightSide::partner_name(ymap_name))
  else {
    return Ok(None);
  };
  let hash = reference_hash(ymap_name.trim_end_matches(".ymap"));
  let partner_hash = reference_hash(partner.trim_end_matches(".ymap"));
  let (Some(vanilla), Some(partner_vanilla)) =
    (sources.get(None, hash)?, sources.get(None, partner_hash)?)
  else {
    return Ok(None);
  };
  let paired = side.count(&vanilla.model) == side.other().count(&partner_vanilla.model);
  Ok(paired.then_some((side, partner_hash)))
}
