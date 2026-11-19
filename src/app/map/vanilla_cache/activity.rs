//! Which latest vanilla YMAPs the game loads, from what each DLC pack's `setup2.xml` and
//! `content.xml` declare (see [`crate::core::game_archive::dlc_content`]).
//!
//! A YMAP is loaded from the archive (`.rpf`) holding it. The stages are applied in order, each
//! DLC pack's data files mounted and its change sets executed:
//!
//! - An archive of the base game, its title update included, is loaded unless a change set
//!   invalidates its path.
//! - A pack's archive is loaded when it is a data file mounted without a change set, or a change
//!   set enables it; one only a script's change set enables counts as loaded.
//! - An archive a change set enables while invalidating the base game path it has the same path
//!   below its device as stands for that path: a later change set invalidating the path unloads
//!   it, so of the packs replacing one base game archive in turn, such as their LOD lights, the
//!   last one's is loaded. So does an archive mounted over a valid base game path (an overlay);
//!   one mounted over an invalidated path by a change set that does not invalidate it is not
//!   loaded.
//! - A pack's archive that is neither a data file nor enabled, such as a title update patch of
//!   an archive the pack does not declare, is not loaded.
//! - An additive archive loads in addition to the archives standing for the path it is enabled
//!   in place of: a change set enabling it unloads none of them, and none invalidating the path
//!   later unloads it. Additive are the archives holding the LOD lights of an island an island
//!   hopper file defines, which the game loads while a script enables the island, and those the
//!   config's `[dlc_activity]` names.
//!
//! Files outside archives, and those of packs the archive records no content of, are loaded.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::game_archive::{CachedFile, VanillaCacheManifest};

/// The archive of a vanilla file: the first `.rpf` below its device's root, as
/// [`crate::core::game_archive::dlc_content::archive_path`] spells paths, with the DLC pack
/// (by its directory name) the device belongs to, `None` for the base game.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Archive {
  pack: Option<String>,
  /// The path below the device, such as `levels/gta5/_citye/scentral_01/scentral_metadata.rpf`.
  path: String,
}

/// The archive of the file at `source`, a game path the vanilla archive records; `None` for a
/// file outside archives below its device.
fn archive_of(source: &str) -> Option<Archive> {
  let lower = source.replace('\\', "/").to_ascii_lowercase();
  let (pack, rest) = if let Some(rest) = lower.strip_prefix("update/update.rpf/dlc_patch/") {
    let (pack, rest) = rest.split_once('/')?;
    (Some(pack.to_owned()), rest.to_owned())
  } else if let Some((_, rest)) = lower.split_once("dlcpacks/") {
    let (pack, rest) = rest.split_once('/')?;
    // Below the pack's `dlc.rpf` or subpack.
    let (_, rest) = rest.split_once('/')?;
    (Some(pack.to_owned()), rest.to_owned())
  } else if let Some(rest) = lower.strip_prefix("update/update.rpf/") {
    (None, rest.to_owned())
  } else {
    (None, lower.split_once('/')?.1.to_owned())
  };
  let rest = rest.strip_prefix("x64/").unwrap_or(&rest);
  let mut path = Vec::new();
  let mut parts = rest.split('/').peekable();
  while let Some(part) = parts.next() {
    parts.peek()?;
    path.push(part);
    if part.ends_with(".rpf") {
      return Some(Archive {
        pack,
        path: path.join("/"),
      });
    }
  }
  None
}

/// The change set of a pack that does something to an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ChangeSetRef {
  /// The pack, by its directory name, such as `mplowrider`.
  pub pack: String,
  pub change_set: String,
}

impl std::fmt::Display for ChangeSetRef {
  fn fmt(
    &self,
    f: &mut std::fmt::Formatter<'_>,
  ) -> std::fmt::Result {
    write!(f, "{} {}", self.pack, self.change_set)
  }
}

/// Why the game does not load an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum Unloaded {
  /// A change set invalidates the archive's own path.
  Invalidated {
    by: ChangeSetRef,
  },
  /// A change set invalidates the base game path the archive stands for.
  Replaced {
    path: String,
    by: ChangeSetRef,
  },
  /// The archive is mounted over a base game path a change set invalidated before.
  MountedOverInvalidated {
    path: String,
    by: ChangeSetRef,
  },
  /// The pack declares the archive, but no change set the game executes enables it.
  NotEnabled,
  /// The pack does not declare the archive, such as a title update patch of it.
  NotDeclared,
}

impl Unloaded {
  /// The reason as a sentence about the archive `path`.
  fn describe(
    &self,
    path: &str,
  ) -> String {
    match self {
      Self::Invalidated {
        by,
      } => format!("{path} is invalidated by {by}"),
      Self::Replaced {
        path: replaced,
        by,
      } => format!("{path} stands for {replaced}, which {by} invalidates"),
      Self::MountedOverInvalidated {
        path: over,
        by,
      } => format!("{path} is mounted over {over}, which {by} invalidated before"),
      Self::NotEnabled => format!("{path} is not enabled"),
      Self::NotDeclared => format!("{path} is not a data file of its pack"),
    }
  }
}

/// The archives holding vanilla YMAPs and whether the game loads them, after the DLC packs'
/// change sets are applied, for tools to draw; `dlc_activity.json` in the vanilla cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct DlcActivity {
  pub format_version: u32,
  pub vanilla_manifest_sha256: String,
  /// The last stage applied.
  pub version: String,
  /// The packs applied, in stage order.
  pub packs: Vec<PackActivity>,
  /// The archives the config's `[dlc_activity] additive_archives` names.
  pub configured_additive: Vec<String>,
  /// The archives holding YMAPs of the stages applied, keyed by their path as `content.xml`
  /// spells it, such as `platform:/levels/gta5/lodlights.rpf` or
  /// `dlcmpheist:/levels/gta5/lodlights.rpf`.
  pub archives: BTreeMap<String, ArchiveActivity>,
  /// Every YMAP of the stages applied, each version keyed by its path in the vanilla archive,
  /// such as `0000-base/ymap/sc1_18_strm_0.ymap`, with the archive holding it.
  pub files: BTreeMap<String, FileActivity>,
}

/// One version of a vanilla YMAP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct FileActivity {
  /// The key of the archive holding it in [`DlcActivity::archives`].
  pub archive: String,
  /// Whether it is the latest version of its name, the one the game reads when it loads the
  /// archive.
  pub latest: bool,
}

/// One pack applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PackActivity {
  /// The stage, such as `0012-mpheist`.
  pub stage: String,
  pub device: String,
  /// The change sets the game executes, in order, and those only a script executes.
  pub change_sets: Vec<crate::core::game_archive::dlc_content::DlcChangeSet>,
  pub on_demand: Vec<crate::core::game_archive::dlc_content::DlcChangeSet>,
}

/// One archive holding YMAPs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ArchiveActivity {
  /// The pack of a pack's archive, by its directory name; absent for the base game's.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub pack: Option<String>,
  /// Why the game does not load it; absent when it does.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub unloaded: Option<Unloaded>,
  /// The base game path it stands for, when the change set enabling it replaces one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub stands_for: Option<String>,
  /// Whether only a script's change set enables it.
  pub on_demand: bool,
  /// Why it loads in addition to the archives standing for the path it is enabled in place of,
  /// when it does.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub additive: Option<Additive>,
  /// The latest YMAPs it holds, in lowercase.
  pub ymaps: Vec<String>,
}

/// Why an archive is additive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum Additive {
  /// It holds the LOD lights of the island `island`, which the game loads while a script
  /// enables the island.
  IslandHopper {
    island: String,
  },
  /// The config's `[dlc_activity] additive_archives` names it.
  Config,
}

/// File name of [`DlcActivity`] in the vanilla cache.
pub(crate) const DLC_ACTIVITY_FILE: &str = "dlc_activity.json";
/// Bump when [`DlcActivity`] changes.
pub(super) const FORMAT_VERSION: u32 = 2;

/// The state of the archives after the stages are applied.
#[derive(Default)]
struct Mounts {
  /// Each pack's device, by the pack's directory name.
  devices: BTreeMap<String, String>,
  /// Whether each data file is mounted over the base game path.
  overlay: BTreeMap<String, bool>,
  /// The data files declared, mounted or not.
  declared: BTreeSet<String>,
  enabled: BTreeSet<String>,
  /// Archives only a script's change set enables.
  on_demand: BTreeSet<String>,
  /// Invalidated paths, with the change set invalidating them.
  invalidated: BTreeMap<String, ChangeSetRef>,
  /// The archives standing for each base game path.
  standing_for: BTreeMap<String, Vec<String>>,
  /// The base game path each archive enabled in place of one stands for.
  stands_for: BTreeMap<String, String>,
  /// Why each archive enabled is unloaded; absent for those loaded.
  unloaded: BTreeMap<String, Unloaded>,
  /// The additive archives.
  additive: BTreeMap<String, Additive>,
}

impl Mounts {
  fn apply(
    manifest: &VanillaCacheManifest,
    through_index: usize,
    additive: BTreeMap<String, Additive>,
    packs: &mut Vec<PackActivity>,
  ) -> Self {
    let mut mounts = Self {
      additive,
      ..Self::default()
    };
    for version in manifest.versions.iter().take(through_index + 1) {
      let Some(content) = &version.dlc else {
        continue;
      };
      let pack = version.id.split_once('-').map_or(version.id.as_str(), |(_, label)| label);
      let pack = pack.to_ascii_lowercase();
      packs.push(PackActivity {
        stage: version.id.clone(),
        device: content.device.clone(),
        change_sets: content.change_sets.clone(),
        on_demand: content.on_demand.clone(),
      });
      mounts.devices.insert(pack.clone(), content.device.clone());
      for file in &content.data_files {
        mounts.declared.insert(file.path.clone());
        mounts.overlay.insert(file.path.clone(), file.overlay);
        if !file.disabled {
          mounts.enable(&file.path, &BTreeSet::new());
        }
      }
      for set in &content.change_sets {
        let by = ChangeSetRef {
          pack: pack.clone(),
          change_set: set.name.clone(),
        };
        let invalidates = set.invalidate.iter().cloned().collect::<BTreeSet<_>>();
        for path in &set.invalidate {
          // A change set enabling only additive archives in place of the path keeps those
          // standing for it.
          let in_place = set.enable.iter().filter(|enabled| {
            enabled.split_once(":/").is_some_and(|(_, below)| format!("platform:/{below}") == *path)
          });
          let mut in_place = in_place.peekable();
          let additive_only = in_place.peek().is_some()
            && in_place.all(|enabled| mounts.additive.contains_key(enabled));
          if additive_only {
            mounts.invalidated.insert(path.clone(), by.clone());
            continue;
          }
          for archive in mounts.standing_for.remove(path).into_iter().flatten() {
            mounts.unloaded.insert(
              archive,
              Unloaded::Replaced {
                path: path.clone(),
                by: by.clone(),
              },
            );
          }
          mounts.invalidated.insert(path.clone(), by.clone());
        }
        for path in &set.disable {
          mounts.enabled.remove(path);
        }
        for path in &set.enable {
          mounts.enable(path, &invalidates);
        }
      }
      for set in &content.on_demand {
        mounts.on_demand.extend(set.enable.iter().cloned());
      }
    }
    mounts
  }

  /// Enables the archive `path` in a change set invalidating `invalidates`.
  fn enable(
    &mut self,
    path: &str,
    invalidates: &BTreeSet<String>,
  ) {
    self.enabled.insert(path.to_owned());
    self.unloaded.remove(path);
    if self.additive.contains_key(path) {
      return;
    }
    let Some((_, below)) = path.split_once(":/") else {
      return;
    };
    let target = format!("platform:/{below}");
    if invalidates.contains(&target) {
      self.standing_for.entry(target.clone()).or_default().push(path.to_owned());
      self.stands_for.insert(path.to_owned(), target);
    } else if self.overlay.get(path).copied().unwrap_or(false) {
      if let Some(by) = self.invalidated.get(&target) {
        self.unloaded.insert(
          path.to_owned(),
          Unloaded::MountedOverInvalidated {
            path: target,
            by: by.clone(),
          },
        );
      } else {
        self.standing_for.entry(target.clone()).or_default().push(path.to_owned());
        self.stands_for.insert(path.to_owned(), target);
      }
    }
  }

  /// The path of `archive` as `content.xml` spells it; `None` for a pack whose content the
  /// archive does not record.
  fn path(
    &self,
    archive: &Archive,
  ) -> Option<String> {
    match &archive.pack {
      None => Some(format!("platform:/{}", archive.path)),
      Some(pack) => Some(format!("{}:/{}", self.devices.get(pack)?, archive.path)),
    }
  }

  /// Why the archive at `path` (of a pack when `pack`) is not loaded; `None` when it is.
  fn not_loaded(
    &self,
    path: &str,
    pack: bool,
  ) -> Option<Unloaded> {
    if !pack {
      return self.invalidated.get(path).map(|by| Unloaded::Invalidated {
        by: by.clone(),
      });
    }
    if self.on_demand.contains(path) && !self.enabled.contains(path) {
      return None;
    }
    if !self.declared.contains(path) && !self.enabled.contains(path) {
      return Some(Unloaded::NotDeclared);
    }
    if !self.enabled.contains(path) {
      return Some(Unloaded::NotEnabled);
    }
    if let Some(by) = self.invalidated.get(path) {
      return Some(Unloaded::Invalidated {
        by: by.clone(),
      });
    }
    self.unloaded.get(path).cloned()
  }
}

/// The additive archives of the stages through `through_index` of `manifest`: those holding the
/// LOD lights YMAP of an island an island hopper file of those stages defines, and
/// `configured`.
fn additive_archives(
  manifest: &VanillaCacheManifest,
  through_index: usize,
  configured: &[String],
) -> BTreeMap<String, Additive> {
  let stages = &manifest.versions[..=through_index];
  let devices = stages
    .iter()
    .filter_map(|version| {
      let pack = version.id.split_once('-').map_or(version.id.as_str(), |(_, label)| label);
      Some((pack.to_ascii_lowercase(), version.dlc.as_ref()?.device.clone()))
    })
    .collect::<BTreeMap<_, _>>();
  let mut additive =
    configured.iter().map(|path| (path.clone(), Additive::Config)).collect::<BTreeMap<_, _>>();
  let islands = stages
    .iter()
    .filter_map(|version| version.dlc.as_ref())
    .flat_map(|content| &content.island_hoppers)
    .map(|island| (format!("{}.ymap", island.lod_lights), &island.name))
    .collect::<BTreeMap<_, _>>();
  for version in stages {
    for (name, change) in &version.changes {
      let Some(island) = islands.get(&name.to_ascii_lowercase()) else {
        continue;
      };
      let Some(archive) = archive_of(&change.file.source) else {
        continue;
      };
      let path = match &archive.pack {
        None => format!("platform:/{}", archive.path),
        Some(pack) => match devices.get(pack) {
          Some(device) => format!("{device}:/{}", archive.path),
          None => continue,
        },
      };
      additive.entry(path).or_insert_with(|| Additive::IslandHopper {
        island: (*island).clone(),
      });
    }
  }
  additive
}

/// The archives holding the YMAPs of the stages through `through_index` of `manifest`, whose
/// latest files are `current` (keyed by file name), and whether the game loads them at that
/// stage, the archives `configured_additive` names additive; `None` when the archive records no
/// DLC content, as archives built before it was recorded.
pub(crate) fn dlc_activity(
  manifest: &VanillaCacheManifest,
  through_index: usize,
  current: &BTreeMap<String, (String, CachedFile)>,
  configured_additive: &[String],
  revision: &str,
) -> Option<DlcActivity> {
  if manifest.versions.iter().all(|version| version.dlc.is_none()) {
    return None;
  }
  let mut packs = Vec::new();
  let additive = additive_archives(manifest, through_index, configured_additive);
  let mounts = Mounts::apply(manifest, through_index, additive, &mut packs);
  let latest = current
    .iter()
    .map(|(name, (_, file))| (name.to_ascii_lowercase(), file.object.as_str()))
    .collect::<BTreeMap<_, _>>();
  let mut archives = BTreeMap::<String, ArchiveActivity>::new();
  let mut files = BTreeMap::new();
  for version in manifest.versions.iter().take(through_index + 1) {
    for (name, change) in &version.changes {
      let name = name.to_ascii_lowercase();
      if !name.ends_with(".ymap") {
        continue;
      }
      let Some(archive) = archive_of(&change.file.source) else {
        continue;
      };
      let Some(path) = mounts.path(&archive) else {
        continue;
      };
      let entry = archives.entry(path.clone()).or_insert_with(|| ArchiveActivity {
        pack: archive.pack.clone(),
        unloaded: mounts.not_loaded(&path, archive.pack.is_some()),
        stands_for: mounts.stands_for.get(&path).cloned(),
        on_demand: mounts.on_demand.contains(&path) && !mounts.enabled.contains(&path),
        additive: mounts.additive.get(&path).cloned(),
        ymaps: Vec::new(),
      });
      let is_latest = latest.get(&name) == Some(&change.file.object.as_str());
      if is_latest {
        entry.ymaps.push(name);
      }
      files.insert(
        change.file.object.clone(),
        FileActivity {
          archive: path,
          latest: is_latest,
        },
      );
    }
  }
  Some(DlcActivity {
    format_version: FORMAT_VERSION,
    vanilla_manifest_sha256: revision.to_owned(),
    version: manifest.versions[through_index].id.clone(),
    packs,
    configured_additive: configured_additive.to_vec(),
    archives,
    files,
  })
}

/// The latest YMAPs `activity` names that the game does not load, keyed by lowercase file name,
/// with why.
pub(crate) fn inactive_ymaps(activity: Option<&DlcActivity>) -> BTreeMap<String, String> {
  activity
    .into_iter()
    .flat_map(|activity| &activity.archives)
    .filter_map(|(path, archive)| Some((path, archive.unloaded.as_ref()?, &archive.ymaps)))
    .flat_map(|(path, unloaded, ymaps)| {
      let why = unloaded.describe(path);
      ymaps.iter().map(move |name| (name.clone(), why.clone()))
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::game_archive::{
    CacheVersion, FileChange,
    dlc_content::{DlcChangeSet, DlcContent, DlcDataFile},
  };

  #[test]
  fn archives_are_found_below_their_devices() {
    let archive = |pack: Option<&str>, path: &str| Archive {
      pack: pack.map(str::to_owned),
      path: path.into(),
    };
    assert_eq!(
      archive_of("x64k.rpf/levels/gta5/_citye/scentral_01/scentral_metadata.rpf/sc1_18.ymap"),
      Some(archive(None, "levels/gta5/_citye/scentral_01/scentral_metadata.rpf"))
    );
    assert_eq!(
      archive_of("update/update.rpf/dlc_patch/mpheist/x64/levels/gta5/a/b.rpf/hei_c.ymap"),
      Some(archive(Some("mpheist"), "levels/gta5/a/b.rpf"))
    );
    assert_eq!(
      archive_of("update/x64/dlcpacks/mpheist4/dlc1.rpf/x64/levels/gta5/a.rpf/b/c.ymap"),
      Some(archive(Some("mpheist4"), "levels/gta5/a.rpf"))
    );
    assert_eq!(
      archive_of("update/update.rpf/x64/levels/gta5/a.rpf/b.ymap"),
      Some(archive(None, "levels/gta5/a.rpf"))
    );
    assert_eq!(archive_of("update/x64/dlcpacks/mpheist/dlc.rpf/x64/levels/a.ymap"), None);
  }

  #[test]
  fn overlays_invalidations_and_undeclared_archives_decide_what_is_loaded() {
    let data_file = |path: &str, overlay: bool| DlcDataFile {
      path: path.into(),
      overlay,
      disabled: true,
      file_type: "RPF_FILE".into(),
    };
    let set = |name: &str, invalidate: &[&str], enable: &[&str]| DlcChangeSet {
      name: name.into(),
      invalidate: invalidate.iter().map(|path| (*path).to_owned()).collect(),
      disable: vec![],
      enable: enable.iter().map(|path| (*path).to_owned()).collect(),
    };
    // Each version's object names its source, which is unique.
    let file = |source: &str| CachedFile {
      sha256: String::new(),
      object: source.into(),
      source: source.into(),
    };
    let version = |id: &str, dlc: Option<DlcContent>, files: &[(&str, &str)]| CacheVersion {
      id: id.into(),
      parent: None,
      archives: vec![],
      changes: files
        .iter()
        .map(|(name, source)| {
          (
            (*name).to_owned(),
            FileChange {
              previous_sha256: None,
              file: file(source),
            },
          )
        })
        .collect(),
      file_kinds: BTreeMap::new(),
      unchanged: 0,
      dlc,
    };
    let manifest = VanillaCacheManifest {
      format_version: 2,
      contents: Default::default(),
      game_dir: ".".into(),
      versions: vec![
        version(
          "0000-base",
          None,
          &[
            ("a.ymap", "x64a.rpf/levels/gta5/a.rpf/a.ymap"),
            ("v.ymap", "x64a.rpf/levels/gta5/v.rpf/v.ymap"),
            ("l.ymap", "x64a.rpf/levels/gta5/l.rpf/l.ymap"),
          ],
        ),
        version(
          "0001-heist",
          Some(DlcContent {
            device: "dlcheist".into(),
            data_files: vec![
              data_file("dlcheist:/levels/gta5/a.rpf", true),
              data_file("dlcheist:/levels/gta5/v.rpf", false),
              data_file("dlcheist:/levels/gta5/l.rpf", false),
            ],
            change_sets: vec![
              set("A", &["platform:/levels/gta5/a.rpf"], &["dlcheist:/levels/gta5/a.rpf"]),
              set("V", &["platform:/levels/gta5/v.rpf"], &["dlcheist:/levels/gta5/v.rpf"]),
              set("L", &["platform:/levels/gta5/l.rpf"], &["dlcheist:/levels/gta5/l.rpf"]),
            ],
            on_demand: vec![],
            island_hoppers: vec![],
          }),
          &[
            ("hei_a.ymap", "update/update.rpf/dlc_patch/heist/x64/levels/gta5/a.rpf/hei_a.ymap"),
            ("hei_v.ymap", "update/update.rpf/dlc_patch/heist/x64/levels/gta5/v.rpf/hei_v.ymap"),
            ("hei_l.ymap", "update/x64/dlcpacks/heist/dlc.rpf/x64/levels/gta5/l.rpf/hei_l.ymap"),
          ],
        ),
        version(
          "0002-lowrider",
          Some(DlcContent {
            device: "dlclow".into(),
            data_files: vec![
              data_file("dlclow:/levels/gta5/a.rpf", true),
              data_file("dlclow:/levels/gta5/l.rpf", false),
            ],
            change_sets: vec![
              set("A", &["platform:/levels/gta5/a.rpf"], &["dlclow:/levels/gta5/a.rpf"]),
              set("L", &["platform:/levels/gta5/l.rpf"], &["dlclow:/levels/gta5/l.rpf"]),
            ],
            on_demand: vec![],
            island_hoppers: vec![],
          }),
          &[
            ("lr_a.ymap", "update/x64/dlcpacks/lowrider/dlc.rpf/x64/levels/gta5/a.rpf/lr_a.ymap"),
            ("lr_l.ymap", "update/x64/dlcpacks/lowrider/dlc.rpf/x64/levels/gta5/l.rpf/lr_l.ymap"),
            ("lr_x.ymap", "update/update.rpf/dlc_patch/lowrider/x64/levels/gta5/x.rpf/lr_x.ymap"),
          ],
        ),
        version(
          "0003-patch",
          Some(DlcContent {
            device: "dlcpatch".into(),
            data_files: vec![
              data_file("dlcpatch:/levels/gta5/v.rpf", true),
              data_file("dlcpatch:/levels/gta5/a.rpf", true),
            ],
            change_sets: vec![set(
              "P",
              &[],
              &["dlcpatch:/levels/gta5/v.rpf", "dlcpatch:/levels/gta5/a.rpf"],
            )],
            on_demand: vec![],
            island_hoppers: vec![],
          }),
          &[("v.ymap", "update/x64/dlcpacks/patch/dlc.rpf/x64/levels/gta5/v.rpf/v.ymap")],
        ),
        version(
          "0004-island",
          Some(DlcContent {
            device: "dlcisland".into(),
            data_files: vec![
              data_file("dlcisland:/levels/gta5/l.rpf", false),
              data_file("dlcisland:/levels/gta5/v.rpf", false),
            ],
            change_sets: vec![
              set("L", &["platform:/levels/gta5/l.rpf"], &["dlcisland:/levels/gta5/l.rpf"]),
              set("V", &["platform:/levels/gta5/v.rpf"], &["dlcisland:/levels/gta5/v.rpf"]),
            ],
            on_demand: vec![],
            island_hoppers: vec![crate::core::game_archive::dlc_content::IslandHopper {
              name: "Island".into(),
              lod_lights: "island_l".into(),
            }],
          }),
          &[
            (
              "island_l.ymap",
              "update/x64/dlcpacks/island/dlc.rpf/x64/levels/gta5/l.rpf/island_l.ymap",
            ),
            (
              "island_v.ymap",
              "update/x64/dlcpacks/island/dlc.rpf/x64/levels/gta5/v.rpf/island_v.ymap",
            ),
          ],
        ),
      ],
    };
    let mut current = BTreeMap::new();
    for version in &manifest.versions {
      for (name, change) in &version.changes {
        current.insert(name.clone(), (version.id.clone(), change.file.clone()));
      }
    }
    let configured = ["dlcisland:/levels/gta5/v.rpf".to_owned()];
    let activity = dlc_activity(&manifest, 4, &current, &configured, "revision");
    let inactive = inactive_ymaps(activity.as_ref());
    // The base game's version of the patched YMAP stays indexed, in its invalidated archive.
    let activity = activity.unwrap();
    let base = &activity.files["x64a.rpf/levels/gta5/v.rpf/v.ymap"];
    assert!(!base.latest);
    assert!(activity.archives[&base.archive].unloaded.is_some());
    assert_eq!(
      inactive.keys().map(String::as_str).collect::<Vec<_>>(),
      ["a.ymap", "hei_a.ymap", "hei_l.ymap", "l.ymap", "lr_x.ymap", "v.ymap"]
    );
    // Lowrider's archives replace heist's, laid beside the base game's or mounted over it, and
    // stay loaded under the patch mounted over their path after them; heist's archive no later
    // pack replaces stays loaded.
    assert!(!inactive.contains_key("hei_v.ymap"));
    assert!(!inactive.contains_key("lr_l.ymap"));
    assert!(!inactive.contains_key("lr_a.ymap"));
    // The island's LOD lights, and the archive the config names, load beside those they would
    // otherwise replace.
    assert_eq!(
      activity.archives["dlcisland:/levels/gta5/l.rpf"].additive,
      Some(Additive::IslandHopper {
        island: "Island".into()
      })
    );
    assert_eq!(activity.archives["dlcisland:/levels/gta5/v.rpf"].additive, Some(Additive::Config));
    assert!(!inactive.contains_key("island_l.ymap") && !inactive.contains_key("island_v.ymap"));
  }
}
