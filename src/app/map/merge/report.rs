//! What the merge decided for each vanilla YMAP, for tools to draw: `report/index.json` and
//! `report/vanilla/{version}/ymap/{file name}.json` in the merge output.
//!
//! A vanilla YMAP is reported when sources compete for one of its entities or items of another
//! array, when changes to it are carried nowhere (see [`super::generations`]), or when it is not
//! deployed for a successor. Each change names its source by resource and stream path, or by
//! the carried document and the stage of the generation it comes from, and the entity or item
//! by its `index` in that source's diff, so that the diff and `--read` documents give the rest.
//!
//! `report/ybn_repeats.json` gives the polygons each merged YBN leaves out as repeats.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
  duplicates::{Change, Competition, Contribution, IgnoredReason, Target},
  generations::{GENERATIONS_DIR, LeftBehind},
};

/// The directory of the report in the merge output.
pub(super) const REPORT_DIR: &str = "report";
/// Bump when the report's layout changes.
const FORMAT_VERSION: u32 = 2;
/// The resource a vanilla overwrite's changes are recorded under.
pub(super) const VANILLA_OVERWRITE_SOURCE: &str = "overwrite/vanilla";

/// The reported vanilla YMAPs; `report/index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct MergeReportIndex {
  pub format_version: u32,
  /// Each reported vanilla YMAP, by lowercase file name.
  pub files: BTreeMap<String, ReportedFile>,
  /// The reported YMAPs each resource's changes are among, by resource.
  pub by_resource: BTreeMap<String, BTreeSet<String>>,
}

/// One reported vanilla YMAP in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ReportedFile {
  /// Its report, relative to the merge output.
  pub report: String,
  pub output: ReportedOutput,
  /// The YMAP the game loads in its place, when it is not deployed for it.
  pub superseded_by: Option<String>,
  /// How many of its entities and items sources compete for.
  pub conflicts: usize,
  /// How many changes to it are carried nowhere.
  pub left_behind: usize,
}

/// What the merge output holds of a vanilla YMAP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum ReportedOutput {
  /// The merged YMAP, under `ymap/`.
  Written,
  /// Nothing: the merged YMAP is vanilla's, or its source is deployed as it is.
  Unchanged,
  /// Nothing, and its sources are omitted: the game loads its successor.
  Omitted,
}

/// What the merge decided for one vanilla YMAP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapMergeReport {
  pub format_version: u32,
  /// The YMAP, in lowercase.
  pub file_name: String,
  /// The latest vanilla YMAP's path in the vanilla archive, `{version}/ymap/{file name}`.
  pub vanilla: Option<String>,
  pub output: ReportedOutput,
  /// The YMAP the game loads in its place, when it is not deployed for it.
  pub superseded_by: Option<String>,
  /// The entities sources compete for, with the change applied and those not.
  pub entities: Vec<EntityReport>,
  /// The items of other arrays sources compete for, likewise.
  pub items: Vec<ItemReport>,
  /// The changes to it carried nowhere.
  pub left_behind: Vec<LeftBehind>,
}

/// The changes sources make to one entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct EntityReport {
  pub guid: u32,
  pub applied: ReportedChange,
  pub ignored: Vec<IgnoredChange>,
}

/// The changes sources make to one item of an array other than the entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ItemReport {
  /// The array by its path in a diff, such as `lod_lights`, `distant_lod_lights.items` or
  /// `instanced_data.grass_instance_list`.
  pub array: String,
  /// The vanilla item's index; absent for an addition.
  pub base_index: Option<usize>,
  pub applied: ReportedChange,
  pub ignored: Vec<IgnoredChange>,
}

/// One source's change to an entity or item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ReportedChange {
  /// The resource, by its ID; `overwrite/vanilla` for a vanilla overwrite.
  pub resource: String,
  pub source: ChangeSource,
  pub change: ReportedKind,
  /// Where the source's diff names the entity or item: `base_index` for a removal, a
  /// modification or a replacement, the addition's `target_index` for an addition.
  pub index: Option<usize>,
}

/// What a change does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum ReportedKind {
  Added,
  Modified,
  /// An item put in place of the vanilla one.
  Replaced,
  Removed,
}

/// Where a change comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum ChangeSource {
  /// A resource's stream file, by its path below the resource's `stream` directory, whose
  /// source-cache diff document holds the change.
  Stream {
    stream_path: String,
  },
  /// A change carried over from another generation: its document in the merge output and the
  /// stage first shipping the generation it comes from.
  Carried {
    document: String,
    origin_version: String,
  },
  /// A vanilla overwrite.
  VanillaOverwrite,
}

/// A change the merge did not apply, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct IgnoredChange {
  pub change: ReportedChange,
  pub reason: IgnoredReason,
}

/// The polygons each merged YBN leaves out as repeats of an earlier one; `report/ybn_repeats.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnRepeatsReport {
  pub format_version: u32,
  /// By lowercase file name, the YBNs leaving any out.
  pub files: BTreeMap<String, usize>,
}

/// The path of [`YbnRepeatsReport`] in the merge output.
pub(super) fn ybn_repeats_path() -> String {
  format!("{REPORT_DIR}/ybn_repeats.json")
}

/// What one vanilla YMAP's report is made of.
pub(super) struct ReportInput<'a> {
  pub file_name: &'a str,
  /// The latest vanilla YMAP's path in the vanilla archive.
  pub vanilla: Option<&'a str>,
  pub output: ReportedOutput,
  pub superseded_by: Option<&'a str>,
  pub duplicates: &'a [Competition],
  pub left_behind: Vec<LeftBehind>,
}

/// The report files, keyed by their path relative to the merge output, built from `inputs`;
/// `stream_paths` gives each source file's stream path by its path in its store.
pub(super) fn build(
  inputs: Vec<ReportInput<'_>>,
  stream_paths: &BTreeMap<String, String>,
) -> crate::common::Result<BTreeMap<String, Vec<u8>>> {
  let mut files = BTreeMap::new();
  let mut index = MergeReportIndex {
    format_version: FORMAT_VERSION,
    files: BTreeMap::new(),
    by_resource: BTreeMap::new(),
  };
  let change = |contribution: &Contribution| {
    let path = contribution.path.to_string_lossy().replace('\\', "/");
    let source = if contribution.resource == VANILLA_OVERWRITE_SOURCE {
      ChangeSource::VanillaOverwrite
    } else if path.starts_with(&format!("{GENERATIONS_DIR}/")) {
      ChangeSource::Carried {
        origin_version: contribution.origin.clone().unwrap_or_default(),
        document: path,
      }
    } else {
      ChangeSource::Stream {
        stream_path: stream_paths.get(&path).cloned().unwrap_or(path),
      }
    };
    ReportedChange {
      resource: contribution.resource.clone(),
      source,
      change: match contribution.change {
        Change::Added => ReportedKind::Added,
        Change::Modified => ReportedKind::Modified,
        Change::Replaced => ReportedKind::Replaced,
        Change::Removed => ReportedKind::Removed,
      },
      index: contribution.index,
    }
  };
  for input in inputs {
    if input.duplicates.is_empty() && input.left_behind.is_empty() && input.superseded_by.is_none()
    {
      continue;
    }
    let file_name = input.file_name.to_ascii_lowercase();
    let ignored = |duplicate: &Competition| {
      duplicate
        .ignored
        .iter()
        .map(|ignored| IgnoredChange {
          change: change(&ignored.source),
          reason: ignored.reason,
        })
        .collect::<Vec<_>>()
    };
    let (mut entities, mut items) = (Vec::new(), Vec::new());
    for duplicate in input.duplicates {
      match &duplicate.target {
        Target::Entity {
          guid,
        } => entities.push(EntityReport {
          guid: *guid,
          applied: change(&duplicate.applied),
          ignored: ignored(duplicate),
        }),
        Target::Item {
          array,
          base_index,
        } => items.push(ItemReport {
          array: array.clone(),
          base_index: *base_index,
          applied: change(&duplicate.applied),
          ignored: ignored(duplicate),
        }),
      }
    }
    let resources = entities
      .iter()
      .flat_map(|entity| {
        std::iter::once(&entity.applied.resource)
          .chain(entity.ignored.iter().map(|ignored| &ignored.change.resource))
      })
      .chain(items.iter().flat_map(|item| {
        std::iter::once(&item.applied.resource)
          .chain(item.ignored.iter().map(|ignored| &ignored.change.resource))
      }))
      .chain(input.left_behind.iter().map(|change| &change.resource))
      .filter(|resource| *resource != VANILLA_OVERWRITE_SOURCE)
      .cloned()
      .collect::<BTreeSet<_>>();
    for resource in resources {
      index.by_resource.entry(resource).or_default().insert(file_name.clone());
    }
    let version = input.vanilla.and_then(|path| path.split('/').next()).unwrap_or("added");
    let report_path = format!("{REPORT_DIR}/vanilla/{version}/ymap/{file_name}.json");
    index.files.insert(
      file_name.clone(),
      ReportedFile {
        report: report_path.clone(),
        output: input.output,
        superseded_by: input.superseded_by.map(str::to_owned),
        conflicts: entities.len() + items.len(),
        left_behind: input.left_behind.len(),
      },
    );
    let report = YmapMergeReport {
      format_version: FORMAT_VERSION,
      file_name,
      vanilla: input.vanilla.map(str::to_owned),
      output: input.output,
      superseded_by: input.superseded_by.map(str::to_owned),
      entities,
      items,
      left_behind: input.left_behind,
    };
    files.insert(report_path, serde_json::to_vec(&report)?);
  }
  files.insert(format!("{REPORT_DIR}/index.json"), serde_json::to_vec(&index)?);
  Ok(files)
}

/// Whether the merge output at `output` holds the report `files` and no other report file.
pub(super) fn is_current(
  output: &std::path::Path,
  files: &BTreeMap<String, Vec<u8>>,
) -> crate::common::Result<bool> {
  let directory = output.join(REPORT_DIR);
  if !crate::platform::fs::is_dir(&directory) {
    return Ok(false);
  }
  let mut found = 0;
  for entry in crate::platform::fs::WalkDir::new(&directory) {
    let entry = entry?;
    if !entry.file_type().is_file() {
      continue;
    }
    let relative = entry.path().strip_prefix(output)?.to_string_lossy().replace('\\', "/");
    match files.get(&relative) {
      Some(bytes) if crate::platform::fs::read(entry.path())? == *bytes => found += 1,
      _ => return Ok(false),
    }
  }
  Ok(found == files.len())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::map::merge::duplicates::IgnoredContribution;

  #[test]
  fn reports_name_sources_by_stream_path_or_carried_document() {
    let contribution = |resource: &str, path: &str, origin: Option<&str>| Contribution {
      resource: resource.into(),
      path: path.into(),
      change: Change::Modified,
      index: Some(4),
      origin: origin.map(str::to_owned),
    };
    let duplicates = [
      Competition {
        target: Target::Entity {
          guid: 9,
        },
        applied: contribution("res", "resources/res/x/lr_a.ymap", None),
        ignored: vec![IgnoredContribution {
          source: contribution(
            "res",
            "generations/res/0012-mpheist/lr_a.ymap.diff.json",
            Some("0012-mpheist"),
          ),
          reason: IgnoredReason::NewerGenerationWins,
        }],
      },
      Competition {
        target: Target::Item {
          array: "lod_lights".into(),
          base_index: Some(2),
        },
        applied: Contribution {
          change: Change::Replaced,
          ..contribution("res", "resources/res/x/lr_a.ymap", None)
        },
        ignored: vec![IgnoredContribution {
          source: Contribution {
            change: Change::Removed,
            ..contribution("other", "resources/other/lr_a.ymap", None)
          },
          reason: IgnoredReason::EarlierSourceWins,
        }],
      },
    ];
    let files = build(
      vec![ReportInput {
        file_name: "lr_a.ymap",
        vanilla: Some("0026-mplowrider/ymap/lr_a.ymap"),
        output: ReportedOutput::Written,
        superseded_by: None,
        duplicates: &duplicates,
        left_behind: Vec::new(),
      }],
      &BTreeMap::from([("resources/res/x/lr_a.ymap".into(), "x/lr_a.ymap".into())]),
    )
    .unwrap();
    let report: YmapMergeReport =
      serde_json::from_slice(&files["report/vanilla/0026-mplowrider/ymap/lr_a.ymap.json"]).unwrap();
    assert_eq!(
      report.entities[0].applied.source,
      ChangeSource::Stream {
        stream_path: "x/lr_a.ymap".into(),
      }
    );
    assert_eq!(
      report.entities[0].ignored[0].change.source,
      ChangeSource::Carried {
        document: "generations/res/0012-mpheist/lr_a.ymap.diff.json".into(),
        origin_version: "0012-mpheist".into(),
      }
    );
    assert_eq!(
      (report.items[0].array.as_str(), report.items[0].base_index),
      ("lod_lights", Some(2))
    );
    assert_eq!(report.items[0].applied.change, ReportedKind::Replaced);
    let index: MergeReportIndex = serde_json::from_slice(&files["report/index.json"]).unwrap();
    assert_eq!(index.files["lr_a.ymap"].conflicts, 2);
    assert_eq!(index.by_resource["res"], BTreeSet::from(["lr_a.ymap".into()]));
    assert_eq!(index.by_resource["other"], BTreeSet::from(["lr_a.ymap".into()]));
  }
}
