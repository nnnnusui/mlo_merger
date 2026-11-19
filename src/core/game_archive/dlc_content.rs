//! What a DLC pack's `setup2.xml` and `content.xml` declare about the archives it mounts: its
//! device, its data files, and the content change sets the game executes, which enable some of
//! those archives and invalidate or disable others.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::Result;

/// The change set groups the game executes for a DLC pack in multiplayer without a script asking
/// for them; `GROUP_ON_DEMAND` sets run only when a script requests them.
const EXECUTED_GROUPS: &[&str] = &[
  "GROUP_STARTUP",
  "GROUP_EARLY_ON",
  "GROUP_MAP",
  "GROUP_TITLEUPDATE_STARTUP",
  "GROUP_UPDATE_STREAMING",
  "GROUP_UPDATE_TEXT",
];

/// The archives one DLC pack declares, and the change sets it executes, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct DlcContent {
  /// The pack's device, lowercase, such as `dlcmpheist`, which its paths start with.
  pub device: String,
  /// The data files the pack declares.
  pub data_files: Vec<DlcDataFile>,
  /// The change sets the game executes by default, in group order.
  pub change_sets: Vec<DlcChangeSet>,
  /// The change sets only a script executes, which may enable archives.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub on_demand: Vec<DlcChangeSet>,
}

/// One data file of a DLC pack's `content.xml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct DlcDataFile {
  /// The file, as [`archive_path`] normalizes it.
  pub path: String,
  /// Whether it is mounted over the game's file at the same path below `platform:`.
  pub overlay: bool,
  /// Whether it is mounted only when a change set enables it.
  pub disabled: bool,
}

/// One content change set: the files it invalidates, disables and enables, those of its map
/// change sets included, as [`archive_path`] normalizes them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct DlcChangeSet {
  pub name: String,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub invalidate: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub disable: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub enable: Vec<String>,
}

/// A path of `content.xml` as `{device}:/{path}`, both lowercase, with `\` as `/`, the platform
/// directory (`%PLATFORM%` or `x64`) left out, so that `dlcMPHeist:/%PLATFORM%/levels/a.rpf`
/// becomes `dlcmpheist:/levels/a.rpf` and `platform:/levels/a.rpf` stays as it is.
pub fn archive_path(path: &str) -> String {
  let path = path.trim().replace('\\', "/").to_ascii_lowercase();
  let Some((device, rest)) = path.split_once(':') else {
    return path;
  };
  let rest = rest.trim_start_matches('/');
  let rest = rest.strip_prefix("%platform%/").or_else(|| rest.strip_prefix("x64/")).unwrap_or(rest);
  format!("{device}:/{rest}")
}

/// Reads a pack's `setup2.xml` and `content.xml`.
pub fn parse(
  setup: &[u8],
  content: &[u8],
) -> Result<DlcContent> {
  let setup: SetupXml = quick_xml::de::from_str(&text(setup))?;
  let content: ContentXml = quick_xml::de::from_str(&text(content))?;
  let executed = setup
    .groups
    .items
    .iter()
    .filter(|group| EXECUTED_GROUPS.contains(&group.name.trim()))
    .flat_map(|group| group.change_sets.items.iter().map(|name| name.trim().to_owned()))
    .chain(setup.change_sets.items.iter().map(|name| name.trim().to_owned()))
    .collect::<Vec<_>>();
  let all = setup
    .groups
    .items
    .iter()
    .flat_map(|group| group.change_sets.items.iter().map(|name| name.trim().to_owned()))
    .collect::<BTreeSet<_>>();
  let change_set = |name: &str| {
    let found = content
      .change_sets
      .iter()
      .flat_map(|sets| &sets.items)
      .find(|set| set.name.trim().eq_ignore_ascii_case(name))?;
    let mut set = DlcChangeSet {
      name: found.name.trim().to_owned(),
      ..Default::default()
    };
    let lists = std::iter::once((&found.invalidate, &found.disable, &found.enable)).chain(
      found.map_change_sets.items.iter().map(|map| (&map.invalidate, &map.disable, &map.enable)),
    );
    for (invalidate, disable, enable) in lists {
      set.invalidate.extend(invalidate.items.iter().map(|path| archive_path(path)));
      set.disable.extend(disable.items.iter().map(|path| archive_path(path)));
      set.enable.extend(enable.items.iter().map(|path| archive_path(path)));
    }
    Some(set)
  };
  let mut seen = BTreeSet::new();
  let change_sets = executed
    .iter()
    .filter(|name| seen.insert(name.to_ascii_lowercase()))
    .filter_map(|name| change_set(name))
    .collect();
  let on_demand = all
    .iter()
    .filter(|name| !seen.contains(&name.to_ascii_lowercase()))
    .filter_map(|name| change_set(name))
    .collect();
  Ok(DlcContent {
    device: setup.device.trim().to_ascii_lowercase(),
    data_files: content
      .data_files
      .iter()
      .flat_map(|files| &files.items)
      .filter(|file| !file.filename.trim().is_empty())
      .map(|file| DlcDataFile {
        path: archive_path(&file.filename),
        overlay: file.overlay.as_ref().is_some_and(|value| value.value),
        disabled: file.disabled.as_ref().is_some_and(|value| value.value),
      })
      .collect(),
    change_sets,
    on_demand,
  })
}

/// The XML text of `bytes`, without a byte order mark.
fn text(bytes: &[u8]) -> String {
  let text = String::from_utf8_lossy(bytes);
  text.trim_start_matches('\u{feff}').to_owned()
}

#[derive(Deserialize)]
struct SetupXml {
  #[serde(rename = "deviceName", default)]
  device: String,
  #[serde(rename = "contentChangeSetGroups", default)]
  groups: Items<SetupGroup>,
  #[serde(rename = "contentChangeSets", default)]
  change_sets: Items<String>,
}

#[derive(Deserialize)]
struct SetupGroup {
  #[serde(rename = "NameHash", default)]
  name: String,
  #[serde(rename = "ContentChangeSets", default)]
  change_sets: Items<String>,
}

#[derive(Deserialize)]
struct ContentXml {
  /// Every `dataFiles` list, as some packs have several.
  #[serde(rename = "dataFiles", default)]
  data_files: Vec<Items<DataFileXml>>,
  #[serde(rename = "contentChangeSets", default)]
  change_sets: Vec<Items<ChangeSetXml>>,
}

#[derive(Deserialize)]
struct DataFileXml {
  #[serde(default)]
  filename: String,
  #[serde(default)]
  overlay: Option<BoolValue>,
  #[serde(default)]
  disabled: Option<BoolValue>,
}

#[derive(Deserialize)]
struct BoolValue {
  #[serde(rename = "@value")]
  value: bool,
}

#[derive(Deserialize)]
struct ChangeSetXml {
  #[serde(rename = "changeSetName", default)]
  name: String,
  #[serde(rename = "mapChangeSetData", default)]
  map_change_sets: Items<FileLists>,
  #[serde(rename = "filesToInvalidate", default)]
  invalidate: Items<String>,
  #[serde(rename = "filesToDisable", default)]
  disable: Items<String>,
  #[serde(rename = "filesToEnable", default)]
  enable: Items<String>,
}

#[derive(Deserialize)]
struct FileLists {
  #[serde(rename = "filesToInvalidate", default)]
  invalidate: Items<String>,
  #[serde(rename = "filesToDisable", default)]
  disable: Items<String>,
  #[serde(rename = "filesToEnable", default)]
  enable: Items<String>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Items<T> {
  #[serde(rename = "Item", default)]
  items: Vec<T>,
}

impl<T> Default for Items<T> {
  fn default() -> Self {
    Self {
      items: Vec::new(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_devices_data_files_and_executed_change_sets() {
    let setup = "\u{feff}<SSetupData><deviceName>dlcMPHeist</deviceName>
      <contentChangeSetGroups>
        <Item><NameHash>GROUP_MAP</NameHash><ContentChangeSets><Item>HEIST_MAP</Item></ContentChangeSets></Item>
        <Item><NameHash>GROUP_ON_DEMAND</NameHash><ContentChangeSets><Item>HEIST_LATER</Item></ContentChangeSets></Item>
        <Item><NameHash>GROUP_STARTUP</NameHash><ContentChangeSets /></Item>
      </contentChangeSetGroups><contentChangeSets /></SSetupData>";
    let content = "<CDataFileMgr__ContentsOfDataFileXml><dataFiles>
        <Item><filename>dlcMPHeist:/%PLATFORM%/levels/gta5/a.rpf</filename><overlay value=\"true\" /><disabled value=\"true\" /></Item>
        <Item><filename>dlcMPHeist:/common/data/b.meta</filename></Item>
      </dataFiles><contentChangeSets>
        <Item><changeSetName>HEIST_MAP</changeSetName>
          <mapChangeSetData><Item><associatedMap>MO</associatedMap>
            <filesToInvalidate><Item>platform:/levels/gta5/a.rpf</Item></filesToInvalidate>
            <filesToDisable />
            <filesToEnable><Item>dlcMPHeist:/%PLATFORM%/levels/gta5/a.rpf</Item></filesToEnable>
          </Item></mapChangeSetData>
          <filesToInvalidate /><filesToDisable /><filesToEnable />
        </Item>
        <Item><changeSetName>HEIST_LATER</changeSetName>
          <filesToEnable><Item>dlcMPHeist:/x64/levels/gta5/c.rpf</Item></filesToEnable>
        </Item>
      </contentChangeSets></CDataFileMgr__ContentsOfDataFileXml>";
    let parsed = parse(setup.as_bytes(), content.as_bytes()).unwrap();
    assert_eq!(parsed.device, "dlcmpheist");
    assert_eq!(
      parsed.data_files,
      [
        DlcDataFile {
          path: "dlcmpheist:/levels/gta5/a.rpf".into(),
          overlay: true,
          disabled: true,
        },
        DlcDataFile {
          path: "dlcmpheist:/common/data/b.meta".into(),
          overlay: false,
          disabled: false,
        },
      ]
    );
    assert_eq!(
      parsed.change_sets,
      [DlcChangeSet {
        name: "HEIST_MAP".into(),
        invalidate: vec!["platform:/levels/gta5/a.rpf".into()],
        disable: vec![],
        enable: vec!["dlcmpheist:/levels/gta5/a.rpf".into()],
      }]
    );
    assert_eq!(parsed.on_demand[0].enable, ["dlcmpheist:/levels/gta5/c.rpf"]);
  }
}
