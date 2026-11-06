//! FiveM game builds placed among vanilla stages by the DLCs they first include.

use crate::fs;
use std::{io, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The `[gamebuild]` table: FiveM build numbers in ascending order, each with the DLC names
/// that identify it. A build without names adds no DLC to the build before it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameBuilds(Vec<(u32, Vec<String>)>);

/// A requested game build, resolved against the table that places it among vanilla stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameBuild {
  /// FiveM game build number.
  pub build: u32,
  builds: GameBuilds,
}

impl GameBuilds {
  /// Loads `[gamebuild]` from optional TOML configuration; a missing file has no builds.
  ///
  /// ```no_run
  /// let builds =
  ///   mlo_merger::core::config::gamebuild::GameBuilds::load(std::path::Path::new("asset/config.toml"))?;
  /// assert_eq!(builds.resolve("mp2023_02")?.build, 3095);
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn load(path: &Path) -> io::Result<Self> {
    let text = match fs::read_to_string(path) {
      Ok(text) => text,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
      Err(error) => return Err(error),
    };
    Self::parse(&text)
      .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
  }

  /// Parses `[gamebuild]` from TOML configuration text.
  pub fn parse(text: &str) -> io::Result<Self> {
    let invalid = |message: String| io::Error::new(io::ErrorKind::InvalidData, message);
    let mut builds = Vec::new();
    for (key, names) in super::Config::parse(text)?.gamebuild {
      let build = key
        .parse::<u32>()
        .ok()
        .filter(|build| *build > 0)
        .ok_or_else(|| invalid(format!("gamebuild.{key} is not a game build number")))?;
      if names.iter().any(|name| name.trim().is_empty()) {
        return Err(invalid(format!("gamebuild.{key} has an empty DLC name")));
      }
      builds
        .push((build, names.into_iter().map(|name| name.trim().to_ascii_lowercase()).collect()));
    }
    builds.sort_by_key(|(build, _)| *build);
    Ok(Self(builds))
  }

  /// Resolves a build number or a DLC name listed in the table. A name listed under several
  /// builds resolves to the latest of them.
  pub fn resolve(
    &self,
    requested: &str,
  ) -> Result<GameBuild> {
    let requested = requested.trim().to_ascii_lowercase();
    let build = if let Ok(build) = requested.parse::<u32>() {
      self.0.iter().any(|(listed, _)| *listed == build).then_some(build)
    } else {
      self.0.iter().rev().find(|(_, names)| names.contains(&requested)).map(|(build, _)| *build)
    };
    let build = build.ok_or_else(|| {
      let known = self.0.iter().map(|(build, _)| build.to_string()).collect::<Vec<_>>();
      format!(
        "Unknown gamebuild: {requested}; expected a build number or DLC name from [gamebuild] in config.toml (builds: {})",
        if known.is_empty() { "none".into() } else { known.join(", ") }
      )
    })?;
    Ok(GameBuild {
      build,
      builds: self.clone(),
    })
  }
}

impl GameBuild {
  /// Index of the last stage the build includes, given stage labels in load order (`base`,
  /// `update`, then DLCs in dlclist.xml order).
  ///
  /// A build starts at the first stage named by its table entry and includes every stage
  /// before the start of the next build found among the stages. A build without names
  /// includes what the build before it does; with no such build, no DLC follows `update`.
  pub fn last_stage(
    &self,
    labels: &[&str],
  ) -> Result<usize> {
    let update =
      labels.iter().position(|label| *label == "update").ok_or("Vanilla stages have no update")?;
    let starts = self
      .builds
      .0
      .iter()
      .map(|(build, names)| {
        let start = labels
          .iter()
          .enumerate()
          .skip(update + 1)
          .find(|(_, label)| names.iter().any(|name| label.eq_ignore_ascii_case(name)))
          .map(|(index, _)| index);
        (*build, names, start)
      })
      .collect::<Vec<_>>();
    let mut found = starts.iter().filter_map(|(build, _, start)| start.map(|start| (build, start)));
    if let Some(mut previous) = found.next() {
      for next in found {
        if next.1 <= previous.1 {
          return Err(format!(
            "gamebuild {} starts at {} but gamebuild {} starts later at {}; check [gamebuild] in config.toml",
            next.0, labels[next.1], previous.0, labels[previous.1]
          )
          .into());
        }
        previous = next;
      }
    }

    let Some((anchor, names, start)) =
      starts.iter().rev().find(|(build, names, _)| *build <= self.build && !names.is_empty())
    else {
      return Ok(update);
    };
    if start.is_none() {
      return Err(format!(
        "gamebuild {anchor} ({}) is not among the vanilla stages; the installed game may be older",
        names.join(", ")
      )
      .into());
    }
    Ok(
      starts
        .iter()
        .filter(|(build, _, _)| *build > self.build)
        .find_map(|(_, _, start)| *start)
        .map_or(labels.len() - 1, |next| next - 1),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const TABLE: &str = "[gamebuild]\n1 = []\n1604 = [\"xm18\", \"mpchristmas2018\"]\n\
    2060 = [\"mpvinewood\", \"sum\", \"mpsum\"]\n2189 = [\"h4\", \"mpheist4\"]\n2200 = []\n\
    2372 = [\"mptuner\"]\n";
  const LABELS: [&str; 10] = [
    "base",
    "update",
    "mpbeach",
    "mpchristmas2018",
    "mpvinewood",
    "mpsum",
    "patchday23ng",
    "mpheist4",
    "patchday24ng",
    "mptuner",
  ];

  fn last(requested: &str) -> Result<usize> {
    GameBuilds::parse(TABLE)?.resolve(requested)?.last_stage(&LABELS)
  }

  #[test]
  fn builds_and_dlc_names_resolve_to_the_last_stage_before_the_next_build() {
    assert_eq!(last("1").unwrap(), 1);
    assert_eq!(last("1604").unwrap(), 3);
    assert_eq!(last("XM18").unwrap(), 3);
    assert_eq!(last("mpsum").unwrap(), 6);
    assert_eq!(last("2189").unwrap(), 8);
    assert_eq!(last("2200").unwrap(), 8);
    assert_eq!(last("mptuner").unwrap(), 9);
    let latest = GameBuilds::parse("[gamebuild]\n1 = [\"a\"]\n2 = [\"a\"]").unwrap();
    assert_eq!(latest.resolve("a").unwrap().build, 2);
  }

  #[test]
  fn unknown_builds_names_and_contradicting_orders_are_rejected() {
    for requested in ["2000", "0", "mpheist3", "0005-mpsum", ""] {
      assert!(last(requested).is_err(), "{requested}");
    }
    let missing = GameBuilds::parse(TABLE).unwrap().resolve("2372").unwrap();
    assert!(missing.last_stage(&LABELS[..8]).unwrap_err().to_string().contains("2372"));
    assert_eq!(
      GameBuilds::parse(TABLE).unwrap().resolve("2189").unwrap().last_stage(&LABELS[..8]).unwrap(),
      7
    );
    let swapped = GameBuilds::parse("[gamebuild]\n1 = [\"mpsum\"]\n2 = [\"mpbeach\"]").unwrap();
    assert!(swapped.resolve("1").unwrap().last_stage(&LABELS).is_err());
    for text in ["[gamebuild]\nx = []", "[gamebuild]\n0 = []", "[gamebuild]\n1 = [\" \"]"] {
      assert!(GameBuilds::parse(text).is_err(), "{text}");
    }
    assert!(GameBuilds::parse("[tolerance]\nybn = 0.1").unwrap().resolve("1").is_err());
  }

  #[test]
  fn shipped_config_lists_builds_and_dlc_names() {
    let builds = GameBuilds::load(Path::new("asset/config.toml")).unwrap();
    assert_eq!(builds.resolve("mp2023_02").unwrap().build, 3095);
    assert_eq!(builds.resolve("3323").unwrap().build, 3323);
  }
}
