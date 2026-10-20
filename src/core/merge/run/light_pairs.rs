//! Source diffs, and the LOD light removals they imply for YMAPs their resources do not ship.
//!
//! A light removed from either YMAP of a LOD light pair is removed from both (see
//! [`lod_light_pairs`](crate::core::format::ymap::compare::lod_light_pairs)). The source cache adds those removals to the diffs of resources shipping
//! both YMAPs; a resource shipping only one leaves the other vanilla, so its removals are merged
//! into the vanilla YMAP of the other as a diff of their own.

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
        let fingerprint =
          |file: &crate::core::format::ymap::compare::YmapComparisonFile| OverwriteFingerprint {
            sha256: file.sha256.clone(),
            size: file.size,
          };
        let fingerprints = (fingerprint(&document.source), fingerprint(&document.vanilla));
        (document.diffs, Some(fingerprints))
      }
      // Sources outside a source cache are compared here, against the latest vanilla.
      None => {
        let ctx = YmapDiffContext {
          tolerances,
          vanilla_parents: vanilla_parents.clone(),
          modded_parents: parents_of(sources, references, Some(&mod_info.mod_name), &source.model)?,
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
          engine::file_fingerprint(&mod_info.mod_ymap_path)?,
          engine::file_fingerprint(vanilla_ymap_path)?,
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
        let mut partner_diff = YmapDiffFields::default();
        side.other().imply_removals(&mut partner_diff, &removed);
        implied.entry(partner_hash).or_default().push((mod_info.mod_name.clone(), partner_diff));
      }
    }
    Ok(implied)
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
