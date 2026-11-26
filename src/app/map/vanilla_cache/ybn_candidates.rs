//! The loaded YBNs that may hold the polygons of each YBN the game does not load.
//!
//! A YBN's number indexes a spatial piece of its area, and a DLC rebuilding an area splits its
//! collision again, into other pieces and files of other names, so no file of a newer
//! generation is that of an older one. A YBN the game does not load therefore has as
//! candidates the loaded YBNs of its kind of collision (its marker, such as `hi@`) whose box
//! overlaps its own on the ground plane: of the archives standing for the path its archive
//! stands for, or, for one first shipped after the game build, of any archive.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rayon::prelude::*;

use super::activity::{DlcActivity, Unloaded};
use super::generations::{SourceYmap, ybn_marker};
use super::*;
use crate::core::format::ybn::read_ybn_bound_tree;

/// A YBN the game does not load: its box and the loaded YBNs that may hold its polygons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnCandidates {
  pub box_min: [f32; 3],
  pub box_max: [f32; 3],
  /// Most covering first.
  pub files: Vec<YbnCandidate>,
}

/// A loaded YBN whose box overlaps that of a YBN the game does not load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnCandidate {
  /// Its lowercase file name.
  pub file: String,
  pub box_min: [f32; 3],
  pub box_max: [f32; 3],
  /// The share of the other YBN's box it overlaps on the ground plane, from 0 to 1.
  pub covers: f32,
}

type Box3 = ([f32; 3], [f32; 3]);

/// The candidates of each YBN of `ybns` that is `inactive`, of which `newer` were first shipped
/// after the game build; see the module docs.
pub(super) fn candidates(
  ybns: &BTreeMap<String, SourceYmap>,
  activity: Option<&DlcActivity>,
  inactive: &BTreeMap<String, String>,
  newer: &BTreeSet<String>,
) -> Result<BTreeMap<String, YbnCandidates>> {
  candidates_with(ybns.keys(), activity, inactive, newer, |name| {
    let bytes = fs::read(&ybns[name].path).map_err(|error| format!("{name}: {error}"))?;
    let root = read_ybn_bound_tree(&bytes).map_err(|error| format!("{name}: {error}"))?;
    Ok(root_box(&root.common))
  })
}

/// [`candidates`] of the YBNs `names`, whose boxes `read_box` reads.
fn candidates_with<'a>(
  names: impl Iterator<Item = &'a String>,
  activity: Option<&DlcActivity>,
  inactive: &BTreeMap<String, String>,
  newer: &BTreeSet<String>,
  read_box: impl Fn(&str) -> std::result::Result<Box3, String> + Sync,
) -> Result<BTreeMap<String, YbnCandidates>> {
  let names = names.collect::<Vec<_>>();
  // The archive of each latest YBN, and the path it stands for.
  let mut archives = HashMap::<String, &str>::new();
  for (path, file) in activity.into_iter().flat_map(|activity| &activity.files) {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if file.latest && name.ends_with(".ybn") {
      archives.insert(name, &file.archive);
    }
  }
  let archive_of = |path: &str| activity.and_then(|activity| activity.archives.get(path));
  // The path an archive's YBNs stand for: the path it is loaded in place of, or its own.
  let stands_for = |path: &str| {
    let archive = archive_of(path)?;
    Some(match &archive.unloaded {
      Some(Unloaded::Replaced {
        path,
        ..
      })
      | Some(Unloaded::MountedOverInvalidated {
        path,
        ..
      }) => path.clone(),
      _ => archive.stands_for.clone().unwrap_or_else(|| path.to_owned()),
    })
  };
  let mut loaded_by_path = HashMap::<String, Vec<&String>>::new();
  let mut loaded = Vec::new();
  for &name in names.iter().filter(|name| !inactive.contains_key(**name)) {
    loaded.push(name);
    if let Some(path) = archives.get(name).and_then(|archive| stands_for(archive)) {
      loaded_by_path.entry(path).or_default().push(name);
    }
  }

  // The loaded YBNs each YBN not loaded is compared with.
  let mut compared = BTreeMap::<&String, Vec<&String>>::new();
  for &name in names.iter().filter(|name| inactive.contains_key(**name)) {
    let pool = if newer.contains(name) {
      loaded.clone()
    } else {
      archives
        .get(name)
        .and_then(|archive| stands_for(archive))
        .and_then(|path| loaded_by_path.get(&path).cloned())
        .unwrap_or_default()
    };
    let marker = ybn_marker(name).0;
    compared.insert(name, pool.into_iter().filter(|other| ybn_marker(other).0 == marker).collect());
  }
  let needed = compared
    .iter()
    .flat_map(|(name, others)| std::iter::once(*name).chain(others.iter().copied()))
    .collect::<BTreeSet<_>>();
  let boxes = needed
    .par_iter()
    .map(|name| Ok(((*name).clone(), read_box(name)?)))
    .collect::<std::result::Result<HashMap<_, _>, String>>()?;

  Ok(
    compared
      .into_iter()
      .map(|(name, others)| {
        let own = boxes[name];
        let mut candidates = others
          .into_iter()
          .filter_map(|other| {
            let other_box = boxes[other];
            Some(YbnCandidate {
              file: other.clone(),
              box_min: other_box.0,
              box_max: other_box.1,
              covers: covers(&own, &other_box)?,
            })
          })
          .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.covers.total_cmp(&a.covers).then_with(|| a.file.cmp(&b.file)));
        (
          name.clone(),
          YbnCandidates {
            box_min: own.0,
            box_max: own.1,
            files: candidates,
          },
        )
      })
      .collect(),
  )
}

/// The `BoxMin` and `BoxMax` of a bound's common record.
fn root_box(common: &[u8]) -> Box3 {
  let vector = |offset: usize| -> [f32; 3] {
    std::array::from_fn(|axis| {
      let at = offset + axis * 4;
      common
        .get(at..at + 4)
        .map_or(0.0, |bytes| f32::from_le_bytes(bytes.try_into().expect("four bytes")))
    })
  };
  (vector(48), vector(32))
}

/// The share of `own`'s ground-plane box `other` overlaps, when they overlap: 1 for an `own`
/// without area that `other` touches.
fn covers(
  own: &Box3,
  other: &Box3,
) -> Option<f32> {
  let mut shared = 1.0;
  let mut area = 1.0;
  for axis in 0..2 {
    let low = own.0[axis].max(other.0[axis]);
    let high = own.1[axis].min(other.1[axis]);
    if low > high {
      return None;
    }
    shared *= high - low;
    area *= own.1[axis] - own.0[axis];
  }
  if area <= 0.0 {
    return Some(1.0);
  }
  (shared > 0.0).then(|| (shared / area).min(1.0))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn covers_measures_the_ground_plane_overlap() {
    let own = ([0.0, 0.0, 0.0], [10.0, 10.0, 5.0]);
    assert_eq!(covers(&own, &([5.0, 0.0, 50.0], [20.0, 10.0, 60.0])), Some(0.5));
    // Boxes that only touch do not overlap.
    assert_eq!(covers(&own, &([10.0, 0.0, 0.0], [20.0, 10.0, 5.0])), None);
    assert_eq!(covers(&own, &([11.0, 0.0, 0.0], [20.0, 10.0, 5.0])), None);
    let flat = ([2.0, 2.0, 0.0], [2.0, 8.0, 0.0]);
    assert_eq!(covers(&flat, &own), Some(1.0));
  }

  #[test]
  fn candidates_are_the_overlapping_ybns_of_the_archives_standing_for_the_same_path() {
    let activity: DlcActivity = serde_json::from_value(serde_json::json!({
      "format_version": 1, "vanilla_manifest_sha256": "", "version": "0001-pack",
      "packs": [], "configured_additive": [],
      "archives": {
        "platform:/area.rpf": {"on_demand": false, "ymaps": [],
          "unloaded": {"kind": "invalidated", "by": {"pack": "pack", "change_set": "CS"}},
          "ybns": ["area_0.ybn", "area_1.ybn", "hi@area_0.ybn"]},
        "dlcpack:/area.rpf": {"pack": "pack", "on_demand": false, "ymaps": [],
          "stands_for": "platform:/area.rpf",
          "ybns": ["new_area_0.ybn", "new_area_1.ybn", "new_area_part.ybn", "hi@new_area_0.ybn"]},
        "platform:/neighbour.rpf": {"on_demand": false, "ymaps": [], "ybns": ["neighbour_0.ybn"]},
      },
      "files": {},
    }))
    .unwrap();
    let mut activity = activity;
    for archive in ["platform:/area.rpf", "dlcpack:/area.rpf", "platform:/neighbour.rpf"] {
      for name in activity.archives[archive].ybns.clone() {
        activity.files.insert(
          format!("stage/ybn/{name}"),
          serde_json::from_value(serde_json::json!({"archive": archive, "latest": true})).unwrap(),
        );
      }
    }
    // The newer generation splits the area at x = 15 instead of x = 10, and moves a part of it
    // to a file of another name; the neighbour overlaps the area but stands for another path.
    let boxes = HashMap::from([
      ("area_0.ybn", ([0.0, 0.0, 0.0], [10.0, 10.0, 5.0])),
      ("area_1.ybn", ([10.0, 0.0, 0.0], [20.0, 10.0, 5.0])),
      ("hi@area_0.ybn", ([0.0, 0.0, 0.0], [10.0, 10.0, 5.0])),
      ("new_area_0.ybn", ([0.0, 0.0, 0.0], [15.0, 10.0, 5.0])),
      ("new_area_1.ybn", ([15.0, 0.0, 0.0], [20.0, 5.0, 5.0])),
      ("new_area_part.ybn", ([15.0, 5.0, 0.0], [20.0, 10.0, 5.0])),
      ("hi@new_area_0.ybn", ([0.0, 0.0, 0.0], [10.0, 10.0, 5.0])),
      ("neighbour_0.ybn", ([5.0, 5.0, 0.0], [12.0, 12.0, 5.0])),
      ("later_0.ybn", ([14.0, 0.0, 0.0], [16.0, 2.0, 5.0])),
    ]);
    let names = boxes.keys().map(|name| name.to_string()).collect::<BTreeSet<_>>();
    let inactive = ["area_0.ybn", "area_1.ybn", "hi@area_0.ybn", "later_0.ybn"]
      .map(|name| (name.to_owned(), String::new()))
      .into_iter()
      .collect::<BTreeMap<_, _>>();
    let newer = BTreeSet::from(["later_0.ybn".to_owned()]);
    let found =
      candidates_with(names.iter(), Some(&activity), &inactive, &newer, |name| Ok(boxes[name]))
        .unwrap();
    let files = |name: &str| {
      found[name].files.iter().map(|file| (file.file.as_str(), file.covers)).collect::<Vec<_>>()
    };
    assert_eq!(files("area_0.ybn"), [("new_area_0.ybn", 1.0)]);
    assert_eq!(
      files("area_1.ybn"),
      [("new_area_0.ybn", 0.5), ("new_area_1.ybn", 0.25), ("new_area_part.ybn", 0.25)]
    );
    assert_eq!(files("hi@area_0.ybn"), [("hi@new_area_0.ybn", 1.0)]);
    // One first shipped after the build looks among every loaded YBN.
    assert_eq!(files("later_0.ybn"), [("new_area_0.ybn", 0.5), ("new_area_1.ybn", 0.5)]);
    assert_eq!(found["area_0.ybn"].box_max, [10.0, 10.0, 5.0]);
    assert_eq!(found.len(), 4);
  }
}
