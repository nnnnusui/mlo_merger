use std::collections::HashSet;

use crate::core::format::{gamefile::meta_resource::jenk_hash, ymap::model::Ymap};

#[derive(Default)]
/// Vanilla-relative parent and physics-dictionary changes.
pub(super) struct YmapMetadataDiff {
  parent: Option<String>,
  dictionaries: ReferenceListDiff,
}

impl YmapMetadataDiff {
  /// Extracts changes while treating names and explicit hashes identically.
  pub(super) fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Self {
    Self {
      parent: (reference_hash(&vanilla.parent) != reference_hash(&modded.parent))
        .then(|| modded.parent.clone()),
      dictionaries: ReferenceListDiff::extract_from(
        &vanilla.physics_dictionaries,
        &modded.physics_dictionaries,
      ),
    }
  }

  /// Combines dictionary deltas and keeps the first conflicting parent change.
  pub(super) fn merge(
    &mut self,
    other: Self,
  ) {
    merge_reference(&mut self.parent, other.parent, "parent");
    self.dictionaries.merge(other.dictionaries);
  }

  /// Applies the combined changes to a clone of the vanilla map.
  pub(super) fn apply_to(
    self,
    output: &mut Ymap,
  ) {
    if let Some(parent) = self.parent {
      output.parent = parent;
    }
    self.dictionaries.apply_to(&mut output.physics_dictionaries);
  }
}

/// Resolves a named or explicitly hashed reference, including null references.
pub(super) fn reference_hash(value: &str) -> u32 {
  let value = value.trim();
  if value.is_empty() {
    return 0;
  }
  if let Some(hash) = value.strip_prefix("hash_")
    && let Ok(hash) = u32::from_str_radix(hash, 16)
  {
    return hash;
  }
  jenk_hash(&value.to_ascii_lowercase())
}

/// Combines a scalar reference change using first-wins conflict handling.
pub(super) fn merge_reference(
  current: &mut Option<String>,
  incoming: Option<String>,
  field: &str,
) {
  if let Some(incoming) = incoming {
    match current {
      Some(current) if reference_hash(current) != reference_hash(&incoming) => {
        log::warn!("    Conflicting {field}: keeping {current}, ignoring {incoming}");
      }
      Some(_) => {}
      None => *current = Some(incoming),
    }
  }
}

#[derive(Default)]
/// Additions and removals to a list of hash-based references.
pub(super) struct ReferenceListDiff {
  removed: HashSet<u32>,
  added: Vec<String>,
}

impl ReferenceListDiff {
  /// Computes vanilla-relative set changes without losing reference spellings.
  pub(super) fn extract_from(
    vanilla: &[String],
    modded: &[String],
  ) -> Self {
    let original = vanilla.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
    let modified = modded.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
    Self {
      removed: original.difference(&modified).copied().collect(),
      added: modded
        .iter()
        .filter(|name| !original.contains(&reference_hash(name)))
        .cloned()
        .collect(),
    }
  }

  /// Accumulates removals and ordered additions from another mod.
  pub(super) fn merge(
    &mut self,
    other: Self,
  ) {
    self.removed.extend(other.removed);
    self.added.extend(other.added);
  }

  /// Removes deleted references and appends deduplicated additions.
  pub(super) fn apply_to(
    self,
    output: &mut Vec<String>,
  ) {
    output.retain(|name| !self.removed.contains(&reference_hash(name)));
    let mut seen = output.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
    output.extend(self.added.into_iter().filter(|name| seen.insert(reference_hash(name))));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn names_and_hashes_are_the_same_reference() {
    assert_eq!(
      reference_hash("Parent_Map"),
      reference_hash(&format!("hash_{:08X}", jenk_hash("parent_map")))
    );
    assert_eq!(reference_hash(""), reference_hash("hash_00000000"));
    let original = vec!["physics".into()];
    let modified = vec![format!("hash_{:08X}", jenk_hash("physics"))];
    let mut output = original.clone();
    ReferenceListDiff::extract_from(&original, &modified).apply_to(&mut output);
    assert_eq!(output, original);
  }

  #[test]
  fn dictionary_deltas_preserve_removals_and_deduplicate_additions() {
    let original = vec!["keep".into(), "remove".into()];
    let mut first = ReferenceListDiff::extract_from(&original, &["keep".into(), "added".into()]);
    first.merge(ReferenceListDiff::extract_from(
      &original,
      &["keep".into(), "remove".into(), "added".into(), "second".into()],
    ));
    let mut output = original;
    first.apply_to(&mut output);
    assert_eq!(output, ["keep", "added", "second"]);
  }

  #[test]
  fn parent_conflicts_keep_first_change() {
    let mut parent = None;
    merge_reference(&mut parent, Some("first".into()), "parent");
    merge_reference(&mut parent, Some("second".into()), "parent");
    assert_eq!(parent.as_deref(), Some("first"));
  }
}
