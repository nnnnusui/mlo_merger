use super::{
  io::Fingerprint,
  resource::{ResourceFile, file_hash},
  *,
};
use crate::core::format::ymap::binary::ymap_to_light_model_with_entities;
use crate::core::format::ymap::{
  diff::reference_hash,
  model::{Ymap, YmapEntity, YmapLodLight},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct LoadedMap {
  pub model: Ymap,
  pub entities: Vec<YmapEntity>,
}

pub(crate) fn read_map(input: &ResourceFile) -> Result<LoadedMap> {
  let bytes = input.read()?;
  let (model, entities) = ymap_to_light_model_with_entities(&bytes, &HashMap::new())?;
  Ok(LoadedMap {
    model,
    entities,
  })
}

pub(super) fn closure(
  target: &str,
  files: &BTreeMap<String, ResourceFile>,
  metadata: &BTreeMap<String, super::metadata::LightMetadata>,
) -> Result<BTreeSet<String>> {
  if !files.contains_key(target) {
    return Err(format!("YMAP is not in the selected resource: {target}").into());
  }
  let names = files
    .keys()
    .map(|path| Ok((file_hash(path)?, path.clone())))
    .collect::<Result<BTreeMap<_, _>>>()?;
  let mut selected = BTreeSet::from([target.to_owned()]);
  let mut pending = vec![target.to_owned()];
  while let Some(path) = pending.pop() {
    if let Some(parent) = metadata
      .get(&path)
      .filter(|item| item.has_lod)
      .and_then(|item| names.get(&item.parent))
      .filter(|parent| metadata.get(*parent).is_some_and(|item| item.has_distant))
      && selected.insert(parent.clone())
    {
      pending.push(parent.clone());
    }
    let hash = file_hash(&path)?;
    for (child, item) in metadata {
      if metadata[&path].has_distant
        && item.has_lod
        && item.parent == hash
        && selected.insert(child.clone())
      {
        pending.push(child.clone());
      }
    }
  }
  Ok(selected)
}

pub(crate) fn document(
  resource: &str,
  target: &str,
  maps: &BTreeMap<String, LoadedMap>,
  fingerprints: &BTreeMap<String, Fingerprint>,
) -> Result<YmapLightDocument> {
  let files = maps
    .keys()
    .enumerate()
    .map(|(index, path)| {
      let fingerprint = &fingerprints[path];
      LightFile {
        file_id: format!("f{index}"),
        path: path.clone(),
        sha256: fingerprint.sha256.clone(),
        size: fingerprint.size,
      }
    })
    .collect::<Vec<_>>();
  let ids = files
    .iter()
    .map(|file| (file.path.as_str(), file.file_id.as_str()))
    .collect::<BTreeMap<_, _>>();
  let names = maps
    .keys()
    .map(|path| Ok((file_hash(path)?, path.as_str())))
    .collect::<Result<BTreeMap<_, _>>>()?;
  let mut output = YmapLightDocument {
    format: "mlo_ymap_lights".into(),
    schema_version: 1,
    resource: resource.into(),
    path: target.into(),
    files: files.clone(),
    lights: Vec::new(),
    diagnostics: Vec::new(),
  };
  for (path, map) in maps {
    if !map.model.lod_lights.is_empty() && !names.contains_key(&reference_hash(&map.model.parent)) {
      output.diagnostics.push(LightDiagnostic {
        path: path.clone(),
        message: "LOD parent is missing from this resource; no fallback is permitted".into(),
      });
    }
    let children = maps
      .iter()
      .filter(|(_, child)| {
        !child.model.lod_lights.is_empty()
          && reference_hash(&child.model.parent) == file_hash(path).unwrap_or(0)
      })
      .collect::<Vec<_>>();
    let count = map.model.distant_lod_lights.items.len();
    let aligned = !children.is_empty()
      && children.iter().all(|(_, child)| child.model.lod_lights.len() == count);
    if count > 0 && !aligned {
      output.diagnostics.push(LightDiagnostic {
        path: path.clone(),
        message: "Missing LOD child or mismatched LOD/distant array counts".into(),
      });
    }
    for (index, distant) in map.model.distant_lod_lights.items.iter().enumerate() {
      let position = parse_position(&distant.position)?;
      let mut light = LightView {
        id: format!("{}:{index}", ids[path.as_str()]),
        distant_ref: LightRef {
          file_id: ids[path.as_str()].into(),
          index,
        },
        position,
        rgbi: distant.rgbi.parse()?,
        lod: Vec::new(),
        editable: aligned,
      };
      if aligned {
        let mut street = None;
        for (child_path, child) in &children {
          match view(
            &child.model.lod_lights[index],
            position,
            LightRef {
              file_id: ids[child_path.as_str()].into(),
              index,
            },
          ) {
            Ok(view) => {
              let flag = view.time_and_state_flags & (1 << 24) != 0;
              if street.is_some_and(|previous| previous != flag) {
                light.editable = false;
              }
              street = Some(flag);
              light.lod.push(view);
            }
            Err(error) => {
              light.editable = false;
              output.diagnostics.push(LightDiagnostic {
                path: (*child_path).clone(),
                message: format!("light {index}: {error}"),
              });
            }
          }
        }
      }
      output.lights.push(light);
    }
  }
  Ok(output)
}

pub(crate) fn parse_position(text: &str) -> Result<[f64; 3]> {
  let coordinates =
    text.split_whitespace().map(str::parse::<f64>).collect::<std::result::Result<Vec<_>, _>>()?;
  let coordinates: [f64; 3] =
    coordinates.try_into().map_err(|_| "Distant position must have three coordinates")?;
  if !coordinates.iter().all(|value| value.is_finite()) {
    return Err("Nonfinite distant light position".into());
  }
  Ok(coordinates)
}

fn view(
  light: &YmapLodLight,
  position: [f64; 3],
  reference: LightRef,
) -> Result<LodLightView> {
  let flags = light.time_and_state_flags.parse::<u32>()?;
  let direction =
    [f64::from(light.direction.x), f64::from(light.direction.y), f64::from(light.direction.z)];
  let outer = u8::try_from(light.cone_outer_angle_or_cap_ext)?;
  let inner = u8::try_from(light.cone_inner_angle)?;
  if !light.falloff_exponent.is_finite()
    || !light.corona_intensity.is_finite()
    || light.corona_intensity < 0.0
    || light.corona_intensity > 255.0
    || light.corona_intensity.fract() != 0.0
  {
    return Err("Invalid light exponent/corona byte".into());
  }
  let light_type = (flags >> 26) & 7;
  let (obb, aabb) =
    geometry::influence(position, direction, f64::from(light.falloff), light_type, outer)?;
  Ok(LodLightView {
    reference,
    direction,
    hash: light.hash.parse()?,
    time_and_state_flags: flags,
    light_type,
    falloff: f64::from(light.falloff),
    falloff_exponent: f64::from(light.falloff_exponent),
    cone_inner_angle: inner,
    cone_outer_angle_or_cap_ext: outer,
    corona_intensity: light.corona_intensity as u8,
    obb,
    aabb,
  })
}
