use super::*;

pub(super) struct PlannedMap {
  pub(super) name: String,
  pub(super) model: Ymap,
  pub(super) clone_entities: Option<Vec<crate::core::format::ymap::model::YmapEntity>>,
  pub(super) original: Rc<OriginalMap>,
  pub(super) copy_target: Option<ModYmapReference>,
  pub(super) rebuild: bool,
}

impl PlannedMap {
  /// Compares runtime entity order and final fields without internal normalized entity keys.
  pub(super) fn differs_from_original(&self) -> bool {
    let mut before = self.original.model.clone();
    let mut after = self.model.clone();
    before.entity_map.clear();
    after.entity_map.clear();
    if before != after {
      return true;
    }
    let entities = match &self.clone_entities {
      Some(entities) => runtime_entities(entities.iter()),
      None => runtime_entities(self.model.entity_map.values()),
    };
    entities != runtime_entities(self.original.entities.iter())
  }

  pub(super) fn entity_at(
    &self,
    index: usize,
  ) -> Option<&crate::core::format::ymap::model::YmapEntity> {
    match &self.clone_entities {
      Some(entities) => entities.get(index),
      None => self.model.entity_map.get_index(index).map(|(_, entity)| entity),
    }
  }

  pub(super) fn guids(&self) -> Vec<u32> {
    let entities = match &self.clone_entities {
      Some(entities) => runtime_entities(entities.iter()),
      None => runtime_entities(self.model.entity_map.values()),
    };
    entities.into_iter().map(|entity| entity.guid).collect()
  }

  pub(super) fn entities_mut(&mut self) -> Vec<&mut crate::core::format::ymap::model::YmapEntity> {
    match &mut self.clone_entities {
      Some(entities) => entities.iter_mut().collect(),
      None => self.model.entity_map.values_mut().collect(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn final_ymap_edit_detection_ignores_internal_keys_but_keeps_repairs() {
    let sample =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap.xml");
    let original = Rc::new(OriginalMap::load(&sample).unwrap());
    let mut output = PlannedMap {
      name: "parent.ymap.xml".into(),
      model: original.model.clone(),
      clone_entities: None,
      original: Rc::clone(&original),
      copy_target: None,
      rebuild: true,
    };
    output.model.entity_map = original
      .entities
      .iter()
      .enumerate()
      .map(|(index, entity)| (index as u32 + 1, entity.clone()))
      .collect();
    assert!(!output.differs_from_original());
    output.model.flags ^= 1;
    assert!(output.differs_from_original());
    output.model.flags ^= 1;
    output.model.entity_map.swap_indices(0, 1);
    assert!(output.differs_from_original());
    output.model.entity_map.swap_indices(0, 1);
    assert!(!output.differs_from_original());
    output.clone_entities = Some(original.entities.clone());
    assert!(!output.differs_from_original());
    output.clone_entities.as_mut().unwrap()[0].parent_index += 1;
    assert!(output.differs_from_original());
    output.clone_entities = Some(original.entities.clone());
    output.clone_entities.as_mut().unwrap()[0].flags ^= 8;
    assert!(output.differs_from_original());
  }
}
