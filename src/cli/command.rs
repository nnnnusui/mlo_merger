use bpaf::*;

use super::{
  deploy::{self, Deploy},
  find::{self, Find},
  from_xml::{self, FromXml},
  generate_source_cache::{self, GenerateSourceCache},
  generate_vanilla::{self, GenerateVanilla},
  generate_vanilla_cache::{self, GenerateVanillaCache},
  get_diff::{self, GetDiff},
  merge::{self, Merge},
  pipeline::{self, Pipeline},
  to_xml::{self, ToXml},
};

/// A parsed top-level CLI operation.
#[derive(Debug, Clone)]
pub enum Command {
  /// Run the complete cached pipeline when no operation flag is supplied.
  Pipeline(Pipeline),
  /// Extract raw, versioned vanilla stream files.
  GenerateVanilla(GenerateVanilla),
  /// Build derived vanilla indexes and latest files.
  GenerateVanillaCache(GenerateVanillaCache),
  /// Build source resource conflict data.
  GenerateSourceCache(GenerateSourceCache),
  /// Merge source changes into vanilla stream files.
  Merge(Merge),
  /// Copy merged and unmerged cached files into a deployable stream layout.
  Deploy(Deploy),
  /// Convert native files to XML.
  ToXml(ToXml),
  /// Convert XML files to native files.
  FromXml(FromXml),
  /// Search generated merge data and optionally its recorded inputs.
  Find(Find),
  /// Compare two stream files.
  GetDiff(GetDiff),
}

fn parser() -> impl Parser<Command> {
  let generate_vanilla = generate_vanilla::parser().map(Command::GenerateVanilla);
  let generate_vanilla_cache = generate_vanilla_cache::parser().map(Command::GenerateVanillaCache);
  let generate_source_cache = generate_source_cache::parser().map(Command::GenerateSourceCache);
  let merge = merge::parser().map(Command::Merge);
  let deploy = deploy::parser().map(Command::Deploy);
  let to_xml = to_xml::parser().map(Command::ToXml);
  let from_xml = from_xml::parser().map(Command::FromXml);
  let get_diff = get_diff::parser().map(Command::GetDiff);
  let find = find::parser().map(Command::Find);
  let pipeline = pipeline::parser().map(Command::Pipeline);
  construct!([
    generate_vanilla,
    generate_vanilla_cache,
    generate_source_cache,
    merge,
    deploy,
    to_xml,
    from_xml,
    get_diff,
    find,
    pipeline,
  ])
}

/// Parses command-line arguments into one operation.
pub fn parse_args() -> Command {
  parser().to_options().run()
}

/// Dispatches an operation to its implementation or placeholder.
pub fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
  match command {
    Command::Pipeline(command) => pipeline::run(command)?,
    Command::GenerateVanilla(command) => generate_vanilla::run(command)?,
    Command::GenerateVanillaCache(command) => generate_vanilla_cache::run(command)?,
    Command::GenerateSourceCache(command) => generate_source_cache::run(command)?,
    Command::Merge(command) => merge::run(command)?,
    Command::Deploy(command) => deploy::run(command)?,
    Command::ToXml(command) => to_xml::run(command)?,
    Command::FromXml(command) => from_xml::run(command)?,
    Command::Find(command) => find::run(command)?,
    Command::GetDiff(command) => get_diff::run_mock(command),
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  #[test]
  fn default_pipeline_parses_source_and_deployment_paths() {
    let Command::Pipeline(command) = parser().to_options().run_inner(&[] as &[&str]).unwrap()
    else {
      panic!("Expected pipeline")
    };
    assert_eq!(command.source_dir, PathBuf::from("asset/source"));
    assert_eq!(command.merged_dir, PathBuf::from("asset/merged"));
    assert_eq!(command.common.output, PathBuf::from("asset/merged_mlo"));
    let Command::Pipeline(command) = parser()
      .to_options()
      .run_inner(&[
        "-i",
        "resources",
        "-o",
        "merged_mlo",
        "--game-dir",
        "game",
        "--merged",
        "merged",
        "--vanilla",
        "vanilla",
        "--vanilla-cache",
        "vanilla-cache",
        "--source-cache",
        "source-cache",
        "-f",
        "-y",
      ])
      .unwrap()
    else {
      panic!("Expected pipeline")
    };
    assert_eq!(command.source_dir, PathBuf::from("resources"));
    assert_eq!(command.common.output, PathBuf::from("merged_mlo"));
    assert_eq!(command.game_dir, PathBuf::from("game"));
    assert_eq!(command.merged_dir, PathBuf::from("merged"));
    assert!(command.common.force);
    assert!(command.common.yes);
  }

  #[test]
  fn generation_command_accepts_shared_pipeline_options() {
    let Command::GenerateVanilla(command) = parser()
      .to_options()
      .run_inner(&[
        "--generate-vanilla",
        "-i",
        "game",
        "--vanilla",
        "custom-vanilla",
        "--gamebuild",
        "3095",
        "-f",
        "-y",
        "--step-name",
        "vanilla",
        "--step-name",
        "merge",
        "-o",
        "out",
      ])
      .unwrap()
    else {
      panic!("Expected vanilla generation command")
    };

    assert_eq!(command.game_dir, PathBuf::from("game"));
    assert_eq!(command.common.vanilla, PathBuf::from("custom-vanilla"));
    assert_eq!(command.common.gamebuild.as_deref(), Some("3095"));
    assert!(command.common.force);
    assert!(command.common.yes);
    assert_eq!(command.common.step_names, ["vanilla", "merge"]);
    assert_eq!(command.common.output, PathBuf::from("out"));

    let Command::GenerateVanilla(command) =
      parser().to_options().run_inner(&["--generate-vanilla", "-i", "game"]).unwrap()
    else {
      panic!("Expected vanilla generation command")
    };
    assert_eq!(command.common.output, PathBuf::from("asset/vanilla"));
  }

  #[test]
  fn conversion_commands_use_path_arguments_and_current_directory_output_by_default() {
    let Command::ToXml(command) =
      parser().to_options().run_inner(&["--to-xml", "input.ymap"]).unwrap()
    else {
      panic!("Expected to-xml command")
    };
    assert_eq!(command.input, PathBuf::from("input.ymap"));
    assert_eq!(command.common.output, PathBuf::from("."));

    let Command::FromXml(command) = parser()
      .to_options()
      .run_inner(&["--from-xml", "input.ymap.xml", "-o", "rebuilt", "--vanilla", "schemas"])
      .unwrap()
    else {
      panic!("Expected from-xml command")
    };
    assert_eq!(command.input, PathBuf::from("input.ymap.xml"));
    assert_eq!(command.common.vanilla, PathBuf::from("schemas"));
    assert_eq!(command.common.output, PathBuf::from("rebuilt"));
  }

  #[test]
  fn cache_and_merge_commands_parse_their_inputs_and_defaults() {
    let Command::GenerateVanillaCache(command) = parser()
      .to_options()
      .run_inner(&["--generate-vanilla-cache", "--vanilla", "vanilla", "-o", "cache"])
      .unwrap()
    else {
      panic!("Expected vanilla cache command")
    };
    assert_eq!(command.common.vanilla, PathBuf::from("vanilla"));
    assert_eq!(command.common.output, PathBuf::from("cache"));

    let Command::GenerateVanillaCache(command) =
      parser().to_options().run_inner(&["--generate-vanilla-cache"]).unwrap()
    else {
      panic!("Expected vanilla cache command")
    };
    assert_eq!(command.common.output, PathBuf::from("asset/vanilla-cache"));

    let Command::GenerateSourceCache(command) = parser()
      .to_options()
      .run_inner(&["--generate-source-cache", "-i", "resources", "--source-cache", "source-cache"])
      .unwrap()
    else {
      panic!("Expected source cache command")
    };
    assert_eq!(command.source_dir, PathBuf::from("resources"));
    assert_eq!(command.common.source_cache, PathBuf::from("source-cache"));
    assert_eq!(command.common.output, PathBuf::from("asset/source-cache"));

    let Command::Merge(command) = parser().to_options().run_inner(&["--merge"]).unwrap() else {
      panic!("Expected merge command")
    };
    assert_eq!(command.source_dir, PathBuf::from("asset/source"));
    assert_eq!(command.common.vanilla_cache, PathBuf::from("asset/vanilla-cache"));
    assert_eq!(command.common.source_cache, PathBuf::from("asset/source-cache"));
    assert_eq!(command.common.output, PathBuf::from("asset/merged"));

    let Command::Merge(command) =
      parser().to_options().run_inner(&["--merge", "-i", "resources", "-o", "merged"]).unwrap()
    else {
      panic!("Expected merge command")
    };
    assert_eq!(command.source_dir, PathBuf::from("resources"));
    assert_eq!(command.common.output, PathBuf::from("merged"));
  }

  #[test]
  fn source_cache_accepts_optional_resource_and_force() {
    let Command::GenerateSourceCache(command) = parser()
      .to_options()
      .run_inner(&["--generate-source-cache", "resource_a", "-i", "resources", "-f"])
      .unwrap()
    else {
      panic!("Expected source cache command")
    };
    assert_eq!(command.resource.as_deref(), Some("resource_a"));
    assert!(command.common.force);
  }

  #[test]
  fn deploy_parses_paths_and_force() {
    let Command::Deploy(command) = parser()
      .to_options()
      .run_inner(&["--deploy", "-i", "merged", "--source-cache", "cache", "-o", "merged_mlo", "-f"])
      .unwrap()
    else {
      panic!("Expected deploy command")
    };
    assert_eq!(command.merged_dir, PathBuf::from("merged"));
    assert_eq!(command.common.source_cache, PathBuf::from("cache"));
    assert_eq!(command.common.output, PathBuf::from("merged_mlo"));
    assert!(command.common.force);
    let Command::Deploy(command) = parser().to_options().run_inner(&["--deploy"]).unwrap() else {
      panic!("Expected deploy command")
    };
    assert_eq!(command.merged_dir, PathBuf::from("asset/merged"));
    assert_eq!(command.common.output, PathBuf::from("asset/merged_mlo"));
  }

  #[test]
  fn find_entity_guid_accepts_filter_and_before_inputs() {
    let Command::Find(command) = parser()
      .to_options()
      .run_inner(&[
        "--find",
        "entity-guid",
        "2443198849",
        "--filter",
        "lr_cs4_10_strm_0.ymap",
        "--diff-all",
      ])
      .unwrap()
    else {
      panic!("Expected find command")
    };
    assert_eq!(command.query, crate::core::find::EntityQuery::Guid(2443198849));
    assert_eq!(command.filter.as_deref(), Some("lr_cs4_10_strm_0.ymap"));
    assert!(command.diff_all);
    assert_eq!(command.merged_dir, PathBuf::from("asset/merged"));
    let Command::Find(command) = parser()
      .to_options()
      .run_inner(&["--find", "entity-guid", "2443198849", "-i", "merged"])
      .unwrap()
    else {
      panic!("Expected find command")
    };
    assert_eq!(command.merged_dir, PathBuf::from("merged"));
    assert!(!command.diff_all);
    let Command::Find(command) = parser()
      .to_options()
      .run_inner(&[
        "--find",
        "entity-guid",
        "2443198849",
        "--filter",
        "*cs4_10_strm_0.ymap",
        "--diff-all",
      ])
      .unwrap()
    else {
      panic!("Expected find command")
    };
    assert_eq!(command.filter.as_deref(), Some("*cs4_10_strm_0.ymap"));
    assert!(command.diff_all);
    assert!(parser().to_options().run_inner(&["--find", "unknown", "2443198849"]).is_err());
    assert!(parser().to_options().run_inner(&["--find", "entity-guid", "4294967296"]).is_err());
  }

  #[test]
  fn find_entity_position_accepts_radius_and_rejects_invalid_numbers() {
    let Command::Find(command) = parser()
      .to_options()
      .run_inner(&[
        "--find",
        "entity-position",
        "3.5,4.2,0.0",
        "--round",
        "1.0",
        "--filter",
        "*.ymap",
        "--diff-all",
      ])
      .unwrap()
    else {
      panic!("Expected position search")
    };
    assert_eq!(
      command.query,
      crate::core::find::EntityQuery::Position {
        position: [3.5, 4.2, 0.0],
        radius: 1.0
      }
    );
    assert!(command.diff_all);
    let Command::Find(command) =
      parser().to_options().run_inner(&["--find", "entity-position", "-3.5,4.2,-2.0"]).unwrap()
    else {
      panic!("Expected position search")
    };
    assert_eq!(
      command.query,
      crate::core::find::EntityQuery::Position {
        position: [-3.5, 4.2, -2.0],
        radius: 1.0
      }
    );
    for args in [
      vec!["--find", "entity-position", "3.5,4.2"],
      vec!["--find", "entity-position", "NaN,0,0"],
      vec!["--find", "entity-position", "0,0,0", "--round", "-1"],
      vec!["--find", "entity-position", "0,0,0", "--round", "inf"],
      vec!["--find", "entity-guid", "2443198849", "--round", "1"],
    ] {
      assert!(parser().to_options().run_inner(args.as_slice()).is_err(), "{args:?}");
    }
  }

  #[test]
  fn find_ybn_position_accepts_type_radius_and_rejects_invalid_options() {
    let Command::Find(command) = parser()
      .to_options()
      .run_inner(&[
        "--find",
        "ybn-position",
        "1211.126,-507.5948,67.54723",
        "--radius",
        "1.0",
        "--type",
        "box",
        "--filter",
        "id2_21_c_0.ybn",
        "--diff-all",
      ])
      .unwrap()
    else {
      panic!("Expected YBN position search")
    };
    assert_eq!(
      command.query,
      crate::core::find::EntityQuery::YbnPosition {
        position: [1211.126, -507.5948, 67.54723],
        radius: 1.0,
        kind: Some("box".into()),
      }
    );
    assert_eq!(command.filter.as_deref(), Some("id2_21_c_0.ybn"));
    assert!(command.diff_all);
    for kind in ["box", "triangle", "sphere", "capsule", "cylinder", "BOX"] {
      let args = ["--find", "ybn-position", "-1,2,-3", "--type", kind, "--radius", "0"];
      assert!(parser().to_options().run_inner(args.as_slice()).is_ok());
    }
    let Command::Find(defaults) =
      parser().to_options().run_inner(&["--find", "ybn-position", "-1,2,-3"]).unwrap()
    else {
      panic!("Expected YBN position search")
    };
    assert_eq!(
      defaults.query,
      crate::core::find::EntityQuery::YbnPosition {
        position: [-1.0, 2.0, -3.0],
        radius: 1.0,
        kind: None,
      }
    );
    for args in [
      vec!["--find", "ybn-position", "1,2"],
      vec!["--find", "ybn-position", "NaN,0,0"],
      vec!["--find", "ybn-position", "1e100,0,0"],
      vec!["--find", "ybn-position", "0,0,0", "--radius", "-1"],
      vec!["--find", "ybn-position", "0,0,0", "--radius", "inf"],
      vec!["--find", "ybn-position", "0,0,0", "--type", "unknown"],
      vec!["--find", "ybn-position", "0,0,0", "--round", "1"],
      vec!["--find", "entity-position", "0,0,0", "--radius", "1"],
      vec!["--find", "entity-guid", "1", "--type", "box"],
    ] {
      assert!(parser().to_options().run_inner(args.as_slice()).is_err(), "{args:?}");
    }
  }

  #[test]
  fn get_diff_accepts_two_positional_inputs() {
    let Command::GetDiff(command) =
      parser().to_options().run_inner(&["--get-diff", "before.ybn", "after.ybn"]).unwrap()
    else {
      panic!("Expected get-diff command")
    };
    assert_eq!(command.before, PathBuf::from("before.ybn"));
    assert_eq!(command.after, PathBuf::from("after.ybn"));
    assert_eq!(command.common.output, PathBuf::from("."));
  }
}
