//! Changes carried over to newer generations of vanilla YMAPs, merged as the resources' diffs.
//!
//! The source cache writes them as `YmapPropagatedDiff` documents, each holding what one
//! resource's YMAPs change in one vanilla YMAP the resource does not ship; see
//! [`crate::core::source_cache`]'s `propagate` step.

use super::*;

/// The carried-over changes of each vanilla YMAP, keyed by its hash: each resource with the
/// document and its diff.
pub(super) type PropagatedDiffs = HashMap<u32, Vec<(String, PathBuf, YmapDiffFields)>>;

impl MergeYmap {
  /// Reads [`Self::propagated`], each document checked against the latest vanilla YMAP it changes.
  pub(super) fn propagated_diffs(
    &self,
    vanilla_map_paths: &HashMap<u32, PathBuf>,
  ) -> Result<PropagatedDiffs, Box<dyn std::error::Error>> {
    let mut diffs = PropagatedDiffs::new();
    for (resource, file_name, path) in &self.propagated {
      let hash = reference_hash(file_name.trim_end_matches(".ymap"));
      let Some(vanilla) = vanilla_map_paths.get(&hash) else {
        tracing::warn!(
          "{} changes {file_name}, which is not a latest vanilla YMAP",
          path.display()
        );
        continue;
      };
      let document = YmapPropagatedDiff::read_current(path, vanilla)?.ok_or_else(|| {
        format!(
          "{} is missing or stale for {}; rebuild the source cache",
          path.display(),
          vanilla.display()
        )
      })?;
      diffs.entry(hash).or_default().push((resource.clone(), path.clone(), document.diffs));
    }
    Ok(diffs)
  }
}
