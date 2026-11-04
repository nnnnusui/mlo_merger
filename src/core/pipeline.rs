use super::{
  deploy::{Deploy, DeploySummary},
  merge::Merge,
  source_cache::BuildSourceCache,
  vanilla::GenerateVanilla,
  vanilla_cache::BuildVanillaCache,
};
use crate::common::{changes::StoreChanges, path::absolute_path};
use crate::core::config::gamebuild::GameBuild;
use serde::Serialize;
use std::path::PathBuf;

/// Executes the cached vanilla, source, merge and deployment stages in order.
#[derive(Debug, Clone)]
pub struct Pipeline {
  /// Source resources, passed to source-cache generation and merge.
  pub source_dir: PathBuf,
  /// Installed game directory used only to create missing vanilla data.
  pub game_dir: PathBuf,
  /// CodeWalker's `magic.dat`, used only to create missing vanilla data.
  pub magic: PathBuf,
  /// Raw vanilla archive directory.
  pub vanilla_dir: PathBuf,
  /// Derived vanilla artifact directory.
  pub vanilla_cache_dir: PathBuf,
  /// Derived source artifact directory.
  pub source_cache_dir: PathBuf,
  /// Resource exclusions for this run; absent or empty means no exclusions.
  pub exclusions: Option<Vec<String>>,
  /// Intermediate merged artifact directory.
  pub merged_dir: PathBuf,
  /// Overwrites applied to source diffs before merging; `None` applies none.
  pub overwrite_dir: Option<PathBuf>,
  /// Final deployment directory.
  pub output_dir: PathBuf,
  /// Game build whose last stage the vanilla cache reflects; `None` is the last stage.
  pub gamebuild: Option<GameBuild>,
  /// Forces the derived stages without re-extracting an existing raw archive.
  pub force: bool,
}

/// Store files changed by each cached stage of a run.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PipelineChanges {
  /// Latest vanilla files in the derived vanilla cache.
  pub vanilla_cache: StoreChanges,
  /// Source stream files moved into the source cache.
  pub source_cache: StoreChanges,
  /// Merged stream files.
  pub merged: StoreChanges,
}

/// Outcome of a pipeline run.
#[derive(Debug)]
pub struct PipelineSummary {
  /// Store files changed by the cached stages.
  pub changes: PipelineChanges,
  /// File operations performed by the final deployment.
  pub deploy: DeploySummary,
}

impl Pipeline {
  /// Runs necessary extraction, derived caches, incremental merge and deployment.
  ///
  /// The executable initializes the shared logger before calling this method.
  ///
  /// ```no_run
  /// use mlo_merger::core::pipeline::Pipeline;
  /// Pipeline {
  ///   source_dir: "asset/source".into(), game_dir: "/mnt/gtav".into(), magic: "magic.dat".into(),
  ///   vanilla_dir: "asset/vanilla".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  ///   source_cache_dir: "asset/source-cache".into(), merged_dir: "asset/merged".into(),
  ///   exclusions: None, overwrite_dir: Some("asset/overwrite".into()),
  ///   output_dir: "merged_mlo".into(), gamebuild: None, force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<PipelineSummary, Box<dyn std::error::Error>> {
    crate::core::config::matching::with_config(|| self.run_steps())?
  }

  fn run_steps(&self) -> Result<PipelineSummary, Box<dyn std::error::Error>> {
    if !self.source_dir.is_dir() {
      return Err(
        format!("Source resource directory does not exist: {}", self.source_dir.display()).into(),
      );
    }
    self.validate_paths()?;

    tracing::info!("Pipeline: generate-vanilla");
    GenerateVanilla {
      game_dir: self.game_dir.clone(),
      output_dir: self.vanilla_dir.clone(),
      magic: self.magic.clone(),
      reuse_existing: true,
    }
    .run()?;

    tracing::info!("Pipeline: generate-vanilla-cache");
    let vanilla_cache = BuildVanillaCache {
      vanilla_dir: self.vanilla_dir.clone(),
      output_dir: self.vanilla_cache_dir.clone(),
      gamebuild: self.gamebuild.clone(),
      force: self.force,
    }
    .run()?;
    tracing::info!("{}", vanilla_cache.summary());

    tracing::info!("Pipeline: generate-source-cache");
    let source_cache = BuildSourceCache {
      source_dir: self.source_dir.clone(),
      output_dir: self.source_cache_dir.clone(),
      vanilla_dir: self.vanilla_dir.clone(),
      vanilla_cache_dir: self.vanilla_cache_dir.clone(),
      resource: None,
      exclusions: self.exclusions.clone(),
      force: self.force,
    }
    .run()?;

    tracing::info!("Pipeline: merge");
    let merged = Merge {
      source_dir: self.source_dir.clone(),
      vanilla_cache_dir: self.vanilla_cache_dir.clone(),
      source_cache_dir: self.source_cache_dir.clone(),
      output_dir: self.merged_dir.clone(),
      overwrite_dir: self.overwrite_dir.clone(),
      force: self.force,
    }
    .run()?;

    tracing::info!("Pipeline: deploy");
    let deploy = Deploy {
      merged_dir: self.merged_dir.clone(),
      source_cache_dir: self.source_cache_dir.clone(),
      output_dir: self.output_dir.clone(),
      force: self.force,
    }
    .run()?;
    Ok(PipelineSummary {
      changes: PipelineChanges {
        vanilla_cache: vanilla_cache.changes.under(&self.vanilla_cache_dir),
        source_cache: source_cache.changes.under(&self.source_cache_dir),
        merged: merged.under(&self.merged_dir),
      },
      deploy,
    })
  }

  /// Source nesting is intentional: source can be `resources` while execution and
  /// generated artifacts live under `resources/mlo_merged`. Only artifact
  /// paths are checked for overlap; the source root is excluded from this list.
  fn validate_paths(&self) -> Result<(), Box<dyn std::error::Error>> {
    let paths = [
      ("vanilla", &self.vanilla_dir),
      ("vanilla-cache", &self.vanilla_cache_dir),
      ("source-cache", &self.source_cache_dir),
      ("merged", &self.merged_dir),
      ("deployment", &self.output_dir),
    ]
    .into_iter()
    .map(|(name, path)| Ok((name, absolute_path(path)?)))
    .collect::<Result<Vec<_>, std::io::Error>>()?;
    let workspace = std::env::current_dir()?.canonicalize()?;
    for (index, (name, path)) in paths.iter().enumerate() {
      if *name != "vanilla" && workspace.starts_with(path) {
        return Err(format!("Pipeline {name} output overlaps the workspace").into());
      }
      for (other_name, other) in paths.iter().skip(index + 1) {
        if path.starts_with(other) || other.starts_with(path) {
          return Err(format!("Pipeline paths overlap: {name} and {other_name}").into());
        }
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::vanilla::{
    CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
  };
  use sha2::{Digest, Sha256};
  use std::collections::BTreeMap;
  use std::fs;

  #[test]
  fn pipeline_routes_resources_through_merge_and_deploy_and_reuses_vanilla() {
    let root = std::env::temp_dir().join(format!("default_pipeline_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let pipeline = Pipeline {
      source_dir: root.join("input"),
      game_dir: root.join("missing-game"),
      magic: "magic.dat".into(),
      vanilla_dir: root.join("vanilla"),
      vanilla_cache_dir: root.join("vanilla-cache"),
      source_cache_dir: root.join("source-cache"),
      exclusions: None,
      merged_dir: root.join("merged"),
      output_dir: root.join("merged_mlo"),
      gamebuild: None,
      force: false,
      overwrite_dir: None,
    };
    let bytes = crate::core::format::ybn::xml::xml_to_ybn(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
    )))
    .unwrap();
    for name in ["resource_a", "resource_b"] {
      let resource = pipeline.source_dir.join(name);
      fs::create_dir_all(resource.join("stream")).unwrap();
      fs::write(resource.join("fxmanifest.lua"), []).unwrap();
      fs::write(resource.join("stream/collision.ybn"), &bytes).unwrap();
    }
    fs::write(pipeline.source_dir.join("resource_a/stream/unique.ybn"), &bytes).unwrap();
    fs::write(pipeline.source_dir.join("resource_a/stream/custom.ytyp"), b"unmatched source")
      .unwrap();
    fs::create_dir_all(pipeline.vanilla_dir.join("0000-base/ybn")).unwrap();
    let mut changes = BTreeMap::new();
    for name in ["collision.ybn", "unique.ybn"] {
      let object = format!("0000-base/ybn/{name}");
      fs::write(pipeline.vanilla_dir.join(&object), &bytes).unwrap();
      changes.insert(
        name.into(),
        FileChange {
          previous_sha256: None,
          file: CachedFile {
            sha256: hex::encode(Sha256::digest(&bytes)),
            object,
            source: format!("base.rpf/{name}"),
          },
        },
      );
    }
    write_json(
      &pipeline.vanilla_dir.join("cache_info.json"),
      &VanillaCacheManifest {
        format_version: 1,
        game_dir: pipeline.game_dir.clone(),
        versions: vec![CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          changes,
          unchanged: 0,
        }],
      },
    )
    .unwrap();
    let vanilla_manifest = fs::read(pipeline.vanilla_dir.join("cache_info.json")).unwrap();
    let first = pipeline.run().unwrap();
    assert_eq!(
      first.changes.vanilla_cache.added,
      ["latest/ybn/collision.ybn", "latest/ybn/unique.ybn"]
        .map(|path| pipeline.vanilla_cache_dir.join(path))
        .into()
    );
    assert_eq!(
      first.changes.source_cache.added,
      [
        "resources/resource_a/collision.ybn",
        "resources/resource_a/unique.ybn",
        "resources/resource_b/collision.ybn",
      ]
      .map(|path| pipeline.source_cache_dir.join(path))
      .into()
    );
    assert_eq!(first.changes.merged.added, [pipeline.merged_dir.join("ybn/collision.ybn")].into());
    assert_eq!(first.deploy.copied, 2);
    assert!(pipeline.vanilla_cache_dir.join("cache_info.json").is_file());
    assert!(pipeline.source_cache_dir.join("cache_info.json").is_file());
    assert!(pipeline.merged_dir.join("cache_info.json").is_file());
    for store in [&pipeline.vanilla_cache_dir, &pipeline.source_cache_dir, &pipeline.merged_dir] {
      assert!(store.join("stream_file_rtree.json").is_file(), "{}", store.display());
    }
    assert!(!pipeline.output_dir.join("stream/stream_file_rtree.json").exists());
    assert!(pipeline.merged_dir.join("ybn/collision.ybn").is_file());
    assert!(!pipeline.merged_dir.join("ybn/unique.ybn").exists());
    assert_eq!(
      fs::read(pipeline.output_dir.join("stream/ybn/merged/collision.ybn")).unwrap(),
      fs::read(pipeline.merged_dir.join("ybn/collision.ybn")).unwrap()
    );
    assert_eq!(
      fs::read(pipeline.output_dir.join("stream/ybn/clone/resource_a/unique.ybn")).unwrap(),
      bytes
    );
    assert!(pipeline.output_dir.join("files.txt").is_file());
    assert!(pipeline.source_dir.join("resource_a/stream/custom.ytyp").is_file());
    assert!(!pipeline.source_dir.join("resource_a/stream/collision.ybn").exists());
    assert_eq!(fs::read(pipeline.vanilla_dir.join("cache_info.json")).unwrap(), vanilla_manifest);
    let second = pipeline.run().unwrap();
    assert_eq!(second.deploy.copied, 0);
    assert_eq!(second.deploy.skipped, 2);
    assert_eq!(second.changes, PipelineChanges::default());
    let mut forced = pipeline.clone();
    forced.force = true;
    let forced = forced.run().unwrap();
    assert_eq!(forced.deploy.copied, 2);
    assert_eq!(forced.changes, PipelineChanges::default());
    assert_eq!(fs::read(pipeline.vanilla_dir.join("cache_info.json")).unwrap(), vanilla_manifest);
    let replacement = crate::core::format::ybn::xml::xml_to_ybn(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/ybn_conflicts/resource_a.ybn.xml"
    )))
    .unwrap();
    fs::write(pipeline.source_dir.join("resource_a/stream/collision.ybn"), &replacement).unwrap();
    fs::remove_dir_all(pipeline.source_dir.join("resource_b")).unwrap();
    let changed = pipeline.run().unwrap().changes;
    assert_eq!(changed.vanilla_cache, StoreChanges::default());
    assert_eq!(
      changed.source_cache,
      StoreChanges {
        added: Default::default(),
        modified: [pipeline.source_cache_dir.join("resources/resource_a/collision.ybn")].into(),
        removed: [pipeline.source_cache_dir.join("resources/resource_b/collision.ybn")].into(),
      }
    );
    assert_eq!(
      changed.merged,
      StoreChanges {
        added: Default::default(),
        modified: Default::default(),
        removed: [pipeline.merged_dir.join("ybn/collision.ybn")].into(),
      }
    );
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn pipeline_reuses_vanilla_and_applies_exclusions_only_for_this_run() {
    let root = std::env::temp_dir().join(format!("pipeline_generation_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let pipeline = Pipeline {
      source_dir: root.join("source"),
      game_dir: root.join("game"),
      magic: "magic.dat".into(),
      vanilla_dir: root.join("vanilla"),
      vanilla_cache_dir: root.join("vanilla-cache"),
      source_cache_dir: root.join("source-cache"),
      exclusions: Some(vec!["unused-resource".into()]),
      merged_dir: root.join("merged"),
      output_dir: root.join("output"),
      gamebuild: None,
      force: false,
      overwrite_dir: None,
    };
    fs::create_dir_all(pipeline.source_dir.join("resource")).unwrap();
    fs::write(pipeline.source_dir.join("resource/fxmanifest.lua"), []).unwrap();
    fs::create_dir_all(&pipeline.game_dir).unwrap();
    let excluded_resource = pipeline.source_dir.join("unused-resource");
    fs::create_dir_all(&excluded_resource).unwrap();
    fs::write(excluded_resource.join("fxmanifest.lua"), []).unwrap();
    fs::create_dir_all(&pipeline.vanilla_dir).unwrap();
    write_json(
      &pipeline.vanilla_dir.join("cache_info.json"),
      &VanillaCacheManifest {
        format_version: 1,
        game_dir: pipeline.game_dir.clone(),
        versions: vec![CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          changes: BTreeMap::new(),
          unchanged: 0,
        }],
      },
    )
    .unwrap();
    pipeline.run().unwrap();
    assert!(pipeline.output_dir.join("deploy_cache_info.json").is_file());
    pipeline.run().unwrap();
    let exclusions_path = pipeline.source_cache_dir.join("source_exclusions.json");
    assert!(!exclusions_path.exists());
    let contains_excluded = || {
      let metadata: serde_json::Value = serde_json::from_reader(
        fs::File::open(pipeline.source_cache_dir.join("cache_info.json")).unwrap(),
      )
      .unwrap();
      metadata["resources"].get("unused-resource").is_some()
    };
    assert!(!contains_excluded());
    let mut unfiltered = pipeline.clone();
    unfiltered.exclusions = None;
    unfiltered.run().unwrap();
    assert!(contains_excluded());
    pipeline.run().unwrap();
    assert!(!contains_excluded());
    unfiltered.exclusions = Some(Vec::new());
    unfiltered.run().unwrap();
    assert!(contains_excluded());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn missing_vanilla_requires_a_game_directory_without_destroying_existing_data() {
    let root =
      std::env::temp_dir().join(format!("pipeline_missing_vanilla_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let pipeline = Pipeline {
      source_dir: root.join("input"),
      game_dir: root.join("missing-game"),
      magic: "magic.dat".into(),
      vanilla_dir: root.join("vanilla"),
      vanilla_cache_dir: root.join("vanilla-cache"),
      source_cache_dir: root.join("source-cache"),
      exclusions: None,
      merged_dir: root.join("merged"),
      output_dir: root.join("output"),
      gamebuild: None,
      force: false,
      overwrite_dir: None,
    };
    fs::create_dir_all(&pipeline.source_dir).unwrap();
    assert!(pipeline.run().unwrap_err().to_string().contains("specify --game-dir"));
    assert!(!pipeline.vanilla_cache_dir.exists());
    assert!(!pipeline.output_dir.exists());
    fs::create_dir_all(&pipeline.vanilla_dir).unwrap();
    fs::write(pipeline.vanilla_dir.join("keep.txt"), b"keep").unwrap();
    assert!(pipeline.run().unwrap_err().to_string().contains("refusing automatic replacement"));
    assert_eq!(fs::read(pipeline.vanilla_dir.join("keep.txt")).unwrap(), b"keep");
    let mut overlapping = pipeline.clone();
    overlapping.output_dir = pipeline.source_dir.clone();
    assert!(overlapping.validate_paths().is_ok());
    let mut nested = pipeline.clone();
    nested.source_dir = root.clone();
    assert!(nested.validate_paths().is_ok());
    assert!(!pipeline.vanilla_cache_dir.exists());
    let mut overlapping = pipeline.clone();
    overlapping.merged_dir = pipeline.source_cache_dir.join("nested");
    assert!(overlapping.run().unwrap_err().to_string().contains("Pipeline paths overlap"));
    assert!(!pipeline.output_dir.exists());
    fs::remove_dir_all(root).unwrap();
  }
}
