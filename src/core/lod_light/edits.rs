use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn apply(
  export: &YmapLightDocument,
  edit: &YmapLightEditDocument,
  maps: &mut BTreeMap<String, LoadedMap>,
) -> Result<()> {
  if edit.format != "mlo_lod_light_edits" || edit.schema_version != 1 {
    return Err("Unsupported light edit format/version".into());
  }
  if edit.resource != export.resource || edit.path != export.path || edit.files != export.files {
    return Err(
      "Stale light edit or mismatched resource/file/dependency snapshot; re-export JSON".into(),
    );
  }
  let lights =
    export.lights.iter().map(|light| (light.id.as_str(), light)).collect::<BTreeMap<_, _>>();
  let mut lod = BTreeMap::<String, BTreeSet<usize>>::new();
  let mut distant = BTreeMap::<String, BTreeSet<usize>>::new();
  for operation in &edit.operations {
    let LightOperation::Remove {
      light_id,
    } = operation;
    let light =
      lights.get(light_id.as_str()).ok_or_else(|| format!("Unknown light ID {light_id}"))?;
    if !light.editable {
      return Err(format!("Light {light_id} has unresolved/unsupported correspondence").into());
    }
    distant.entry(light.distant_ref.file_id.clone()).or_default().insert(light.distant_ref.index);
    for view in &light.lod {
      lod.entry(view.reference.file_id.clone()).or_default().insert(view.reference.index);
    }
  }
  for file in &export.files {
    let map = maps.get(&file.path).ok_or("Light dependency missing")?;
    if lod
      .get(&file.file_id)
      .into_iter()
      .flatten()
      .any(|index| *index >= map.model.lod_lights.len())
      || distant
        .get(&file.file_id)
        .into_iter()
        .flatten()
        .any(|index| *index >= map.model.distant_lod_lights.items.len())
    {
      return Err("Invalid light array reference".into());
    }
  }
  let mut street_counts = BTreeMap::new();
  for file in &export.files {
    let Some(removals) = distant.get(&file.file_id) else {
      continue;
    };
    let parent_hash = resource::file_hash(&file.path)?;
    let children = maps
      .values()
      .filter(|map| {
        !map.model.lod_lights.is_empty()
          && crate::core::format::ymap::reference_hash(&map.model.parent) == parent_hash
      })
      .collect::<Vec<_>>();
    let first = children.first().ok_or("Distant removal has no linked child")?;
    let mut street = 0u32;
    for (index, light) in first.model.lod_lights.iter().enumerate() {
      let flag = light.time_and_state_flags.parse::<u32>()? & (1 << 24) != 0;
      for child in &children {
        if child
          .model
          .lod_lights
          .get(index)
          .ok_or("Mismatched shared-child light count")?
          .time_and_state_flags
          .parse::<u32>()?
          & (1 << 24)
          != u32::from(flag) << 24
        {
          return Err("Shared-child streetlight classification differs".into());
        }
      }
      if flag && !removals.contains(&index) {
        street += 1;
      }
    }
    street_counts.insert(file.path.clone(), street);
  }
  for file in &export.files {
    let map = maps.get_mut(&file.path).ok_or("Light dependency missing")?;
    if let Some(removals) = lod.get(&file.file_id) {
      let mut index = 0;
      map.model.lod_lights.retain(|_| {
        let retained = !removals.contains(&index);
        index += 1;
        retained
      });
    }
    if let Some(removals) = distant.get(&file.file_id) {
      let mut index = 0;
      map.model.distant_lod_lights.items.retain(|_| {
        let retained = !removals.contains(&index);
        index += 1;
        retained
      });
      map.model.distant_lod_lights.num_street_lights = street_counts[&file.path];
    }
  }
  Ok(())
}
