use std::collections::BTreeSet;

use serde_json::json;

use super::*;
use crate::core::format::ymap::{
  compare::lod_light_pairs::LightSide,
  model::{YmapDistantLodLight, YmapLodLight},
};

fn lod(hash: &str) -> YmapLodLight {
  YmapLodLight {
    hash: hash.into(),
    ..YmapLodLight::default()
  }
}

fn distant(x: f32) -> YmapDistantLodLight {
  YmapDistantLodLight {
    position: format!("{x} 0 0"),
    rgbi: "0".into(),
  }
}

fn pair(count: usize) -> (Ymap, Ymap) {
  let lods = Ymap {
    lod_lights: (0..count).map(|index| lod(&index.to_string())).collect(),
    ..base()
  };
  let distants = Ymap {
    distant_lod_lights: YmapDistantLodLights {
      items: (0..count).map(|index| distant(index as f32)).collect(),
      ..YmapDistantLodLights::default()
    },
    ..base()
  };
  (lods, distants)
}

fn without<T: Clone>(
  items: &[T],
  removed: &[usize],
) -> Vec<T> {
  items
    .iter()
    .enumerate()
    .filter(|(index, _)| !removed.contains(index))
    .map(|(_, item)| item.clone())
    .collect()
}

#[test]
fn names_the_other_ymap_of_a_pair() {
  assert_eq!(LightSide::of("VW_LODLIGHTS_SMALL037.ymap"), Some(LightSide::LodLights));
  assert_eq!(LightSide::of("vw_distlodlights_small037.ymap"), Some(LightSide::DistantLodLights));
  assert_eq!(LightSide::of("hei_ch1_11.ymap"), None);
  assert_eq!(
    LightSide::partner_name("VW_LodLights_small037.ymap").as_deref(),
    Some("vw_distlodlights_small037.ymap")
  );
  assert_eq!(
    LightSide::partner_name("apa_distlodlights_medium034.ymap").as_deref(),
    Some("apa_lodlights_medium034.ymap")
  );
  assert_eq!(
    LightSide::partner_name("distlodlights_large000.ymap").as_deref(),
    Some("lodlights_large000.ymap")
  );
  assert_eq!(
    LightSide::partner_name("lodlights_large000.ymap").as_deref(),
    Some("distlodlights_large000.ymap")
  );
  assert_eq!(LightSide::partner_name("hei_ch1_11.ymap"), None);
  assert_eq!(LightSide::partner_name("hei_carrier_lodlights.ymap"), None);
  assert_eq!(LightSide::partner_name("xdistlodlights_large000.ymap"), None);
}

#[test]
fn removals_of_either_side_are_implied_on_the_other() {
  let (lods, distants) = pair(6);
  // As tw-builder-map does: more LOD lights removed than distant ones.
  let modded_lods = Ymap {
    lod_lights: without(&lods.lod_lights, &[1, 2, 4]),
    ..lods.clone()
  };
  let mut modded_distants = distants.clone();
  modded_distants.distant_lod_lights.items = without(&distants.distant_lod_lights.items, &[1]);
  let tolerances = MatchTolerances::default();

  let lod_removed = LightSide::LodLights.removed_between(&lods, &modded_lods, &tolerances);
  let distant_removed =
    LightSide::DistantLodLights.removed_between(&distants, &modded_distants, &tolerances);
  assert_eq!(lod_removed, BTreeSet::from([1, 2, 4]));
  assert_eq!(distant_removed, BTreeSet::from([1]));

  let mut lod_diff = diff(&lods, &modded_lods);
  assert_eq!(LightSide::LodLights.imply_removals(&mut lod_diff, &distant_removed), 0);
  assert_eq!(lod_diff, diff(&lods, &modded_lods));

  let mut distant_diff = diff(&distants, &modded_distants);
  assert_eq!(LightSide::DistantLodLights.imply_removals(&mut distant_diff, &lod_removed), 2);
  assert_eq!(
    serde_json::to_value(&distant_diff).unwrap(),
    json!({
      "distant_lod_lights": { "items": [
        { "Removed": { "index": 1 } },
        { "Removed": { "index": 2, "implied": true } },
        { "Removed": { "index": 4, "implied": true } },
      ] },
    })
  );
  // Implied removals are not read back as removals the source shows.
  assert_eq!(LightSide::DistantLodLights.removed(&distant_diff), distant_removed);

  let mut applied = distants.clone();
  applied.apply(&distant_diff).unwrap();
  assert_eq!(
    applied.distant_lod_lights.items,
    without(&distants.distant_lod_lights.items, &[1, 2, 4])
  );
}

#[test]
fn an_implied_removal_wins_over_a_change_of_the_same_light() {
  let (_, distants) = pair(3);
  let mut modded = distants.clone();
  modded.distant_lod_lights.items[1].position = "50 0 0".into();
  modded.distant_lod_lights.items[2].rgbi = "9".into();
  let mut distant_diff = diff(&distants, &modded);
  LightSide::DistantLodLights.imply_removals(&mut distant_diff, &BTreeSet::from([1, 2]));
  assert_eq!(
    serde_json::to_value(&distant_diff).unwrap(),
    json!({
      "distant_lod_lights": { "items": [
        { "Removed": { "index": 1, "implied": true } },
        { "Removed": { "index": 2, "implied": true } },
      ] },
    })
  );
}

#[test]
fn implied_removals_alone_make_a_diff() {
  let mut lod_diff = YmapDiffFields::default();
  LightSide::LodLights.imply_removals(&mut lod_diff, &BTreeSet::new());
  assert_eq!(lod_diff, YmapDiffFields::default());
  let mut distant_diff = YmapDiffFields::default();
  LightSide::DistantLodLights.imply_removals(&mut distant_diff, &BTreeSet::new());
  assert_eq!(distant_diff, YmapDiffFields::default());

  LightSide::LodLights.imply_removals(&mut lod_diff, &BTreeSet::from([0]));
  assert_eq!(
    serde_json::to_value(&lod_diff).unwrap(),
    json!({ "lod_lights": [{ "Removed": { "index": 0, "implied": true } }] })
  );
}
