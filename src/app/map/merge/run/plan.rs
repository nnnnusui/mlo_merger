use super::*;

/// A YMAP the merge writes or keeps.
pub(super) struct PlannedMap {
  pub(super) name: String,
  /// The YMAP as the merge leaves it; its entities carry the merge's identities until the
  /// outputs are decided.
  pub(super) model: Ymap,
  /// Whether it is kept as a source or vanilla ships it, so that it is written only when the
  /// merge repairs its entities.
  pub(super) kept: bool,
  pub(super) original: Rc<OriginalMap>,
  pub(super) copy_target: Option<ModYmapReference>,
  pub(super) rebuild: bool,
}

impl PlannedMap {
  /// A YMAP merged from diffs, `model` holding its merged entities.
  pub(super) fn merged(
    name: String,
    model: Ymap,
    original: Rc<OriginalMap>,
  ) -> Self {
    Self {
      name,
      model,
      kept: false,
      original,
      copy_target: None,
      rebuild: true,
    }
  }

  /// `original` kept as it is, as `model` with `entities`, copied from `copy_target` unless the
  /// merge repairs it.
  pub(super) fn kept(
    name: String,
    mut model: Ymap,
    entities: Vec<YmapEntity>,
    original: Rc<OriginalMap>,
    copy_target: Option<ModYmapReference>,
  ) -> Self {
    model.entities = entities;
    Self {
      name,
      model,
      kept: true,
      original,
      copy_target,
      rebuild: false,
    }
  }

  /// Compares runtime entity order and final fields without internal normalized entity keys.
  pub(super) fn differs_from_original(&self) -> bool {
    let mut before = self.original.model.clone();
    let mut after = self.model.clone();
    before.entities.clear();
    after.entities.clear();
    if before != after {
      return true;
    }
    self.runtime_entities() != runtime_entities(self.original.entities.iter())
  }

  /// Whether this map encodes to the same bytes as the original: models that differ only in
  /// names one side spells as `hash_XXXXXXXX` and the other resolved, as when one was decoded
  /// with hash names and the other without, are the same file.
  pub(super) fn encodes_like_original(&self) -> std::io::Result<bool> {
    let original =
      runtime_entities(self.original.entities.iter()).into_iter().cloned().collect::<Vec<_>>();
    Ok(
      crate::core::format::ymap::binary::write_ymap(&self.model, &self.runtime_entity_values())?
        == crate::core::format::ymap::binary::write_ymap(&self.original.model, &original)?,
    )
  }

  /// The entities in the order the game reads them.
  pub(super) fn runtime_entities(&self) -> Vec<&YmapEntity> {
    runtime_entities(self.model.entities.iter())
  }

  /// [`Self::runtime_entities`], cloned for writing.
  pub(super) fn runtime_entity_values(&self) -> Vec<YmapEntity> {
    self.runtime_entities().into_iter().cloned().collect()
  }

  pub(super) fn guids(&self) -> Vec<u32> {
    self.runtime_entities().into_iter().map(|entity| entity.guid).collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn final_ymap_edit_detection_ignores_internal_keys_but_keeps_repairs() {
    let sample =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap");
    let original = Rc::new(OriginalMap::load(&sample).unwrap());
    let mut output =
      PlannedMap::merged("parent.ymap".into(), original.model.clone(), Rc::clone(&original));
    output.model.entities = original.entities.clone();
    assert!(!output.differs_from_original());
    output.model.flags ^= 1;
    assert!(output.differs_from_original());
    output.model.flags ^= 1;
    output.model.entities.swap(0, 1);
    assert!(output.differs_from_original());
    output.model.entities.swap(0, 1);
    assert!(!output.differs_from_original());
    output.model.entities[0].parent_index += 1;
    assert!(output.differs_from_original());
    output.model.entities = original.entities.clone();
    output.model.entities[0].flags ^= 8;
    assert!(output.differs_from_original());
  }
}
