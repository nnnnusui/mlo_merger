use mlo_merger::{
  cli::command::{BuildYmapCache, Command, MergeYbn, Pipeline, parse_args},
  core::{
    codewalker::CodeWalker,
    extract::ExtractYmap,
    format::{
      gamefile::resource_convert,
      ymap::{model::Ymap, xml::XmlYmap},
    },
    gtav_cache::version_logger,
    merge::ybn_conflicts::MergeYbnConflicts,
    merge::{build_ymap_parent_cache, run::MergeYmapXml},
    xmlconvert::{Xml2Ymap, Ymap2Xml},
  },
};
use quick_xml::de::from_str;
use simplelog::*;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let opts = parse_args();

  // Archive existing log file if it exists
  let log_path = Path::new("mlo_merger.log");
  if log_path.exists() {
    let log_archive_dir = match &opts {
      Command::Pipeline(cmd) => cmd.log_dir.clone(),
      _ => std::path::PathBuf::from("asset/log"),
    };
    fs::create_dir_all(&log_archive_dir)?;

    // Get file's last modified time as timestamp
    let metadata = fs::metadata(log_path)?;
    let modified_time = metadata.modified()?;
    let datetime: chrono::DateTime<chrono::Local> = modified_time.into();
    let timestamp = datetime.format("%Y%m%d_%H%M%S");

    let archived_log_path = log_archive_dir.join(format!("mlo_merger_{}.log", timestamp));
    fs::rename(log_path, archived_log_path)?;
  }

  let active_log_path = match &opts {
    Command::Pipeline(cmd) => {
      fs::create_dir_all(&cmd.log_dir)?;
      cmd
        .log_dir
        .join(format!("mlo_merger_{}.log", chrono::Local::now().format("%Y%m%d_%H%M%S_%3f")))
    }
    _ => log_path.to_path_buf(),
  };

  CombinedLogger::init(vec![
    TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
    WriteLogger::new(LevelFilter::Info, Config::default(), File::create(&active_log_path)?),
    version_logger(),
  ])?;

  log::info!("Writing log to {}", active_log_path.display());
  log::info!("Options: {:?}", opts);

  match opts {
    Command::ParseYmapXml(cmd) => run_parse_ymap_xml(&cmd.input)?,
    Command::MergeYmapXml(cmd) => cmd.run()?,
    Command::BuildYmapCache(cmd) => build_ymap_cache(&cmd)?,
    Command::BuildGtavCache(cmd) => cmd.run(&init_codewalker()?)?,
    Command::BuildDiffCache(cmd) => cmd.run()?,
    Command::ExtractYmap(cmd) => cmd.run()?,
    Command::GetProp(cmd) => cmd.run()?,
    Command::CheckStreamConflicts(cmd) => cmd.run()?,
    Command::ToXml(cmd) => {
      let output_dir = match cmd.output_dir {
        Some(output_dir) => output_dir,
        None if cmd.input.is_file() => {
          cmd.input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
        }
        None => return Err("--output is required when --input is a directory".into()),
      };
      let (converted, failed) = resource_convert::convert_files_to_xml(&cmd.input, &output_dir)?;
      log::info!("Converted {converted} files to XML; {failed} failed.");
    }
    Command::FromXml(cmd) => {
      let (converted, failed) = resource_convert::convert_files_from_xml(
        &cmd.input,
        &cmd.output_dir,
        cmd.schema_dir.as_deref(),
      )?;
      log::info!("Converted {converted} XML files to binary; {failed} failed.");
    }
    Command::MergeYbn(cmd) => run_merge_ybn(&cmd)?,
    Command::Pipeline(cmd) => run_pipeline(&cmd)?,
  };

  log::info!("✓ Command completed successfully");
  Ok(())
}

fn build_ymap_cache(cmd: &BuildYmapCache) -> Result<(), Box<dyn std::error::Error>> {
  build_ymap_parent_cache(&cmd.vanilla_dir)?;
  Ok(())
}

fn run_merge_ybn(cmd: &MergeYbn) -> Result<(), Box<dyn std::error::Error>> {
  let codewalker = cmd.use_codewalker_dll.then(init_codewalker).transpose()?;
  MergeYbnConflicts {
    source_dir: cmd.source_dir.clone(),
    vanilla_dir: cmd.vanilla_dir.clone(),
    output_dir: cmd.output_dir.clone(),
    omitted_files_path: cmd.omitted_files_path.clone(),
  }
  .run(codewalker.as_ref())?;
  Ok(())
}

fn init_codewalker() -> Result<CodeWalker, Box<dyn std::error::Error>> {
  let bridge_dll =
    std::env::var("CODEWALKER_BRIDGE_DLL").map(std::path::PathBuf::from).unwrap_or_else(|_| {
      std::path::PathBuf::from("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll")
    });
  CodeWalker::init(&bridge_dll).map_err(|e| {
    format!(
      "Failed to load CodeWalker.Bridge from {} ({e}). Build it with `dotnet publish bridge/CodeWalker.Bridge -c Release -o bridge/CodeWalker.Bridge/bin/publish -p:CodeWalkerCoreDllPath=<path to CodeWalker.Core.dll>`, or set CODEWALKER_BRIDGE_DLL.",
      bridge_dll.display()
    )
    .into()
  })
}

fn run_pipeline(cmd: &Pipeline) -> Result<(), Box<dyn std::error::Error>> {
  let vanilla_ybn_dir = cmd.workspace.join("vanilla/ybn");
  MergeYbnConflicts::has_conflicts(&cmd.source_dir, &vanilla_ybn_dir)?;
  println!("This will run the full pipeline (workspace: {}):", cmd.workspace.display());
  println!("  1. extract   {} -> {}", cmd.source_dir.display(), cmd.extracted_dir.display());
  let backend = if cmd.use_codewalker_dll { "CodeWalker" } else { "Native" };
  println!(
    "  2. ymap2xml  {} -> {} ({backend})",
    cmd.extracted_dir.display(),
    cmd.extracted_xml_dir.display()
  );
  println!(
    "  3. mergexml  {} + {} -> {}",
    cmd.vanilla_xml_dir.display(),
    cmd.extracted_xml_dir.display(),
    cmd.merged_xml_dir.display()
  );
  println!(
    "  4. xml2ymap  {} -> {} ({backend})",
    cmd.merged_xml_dir.display(),
    cmd.merged_dir.display()
  );
  println!("  5. merge YBN conflicts -> {}/merged_ybn", cmd.workspace.display());
  if let Some(resource_dir) = &cmd.output_resource_dir {
    println!("  6. deploy    {} -> {}", cmd.merged_dir.display(), resource_dir.display());
  }
  let mut answer = String::new();
  if !cmd.yes {
    let existing =
      pipeline_output_dirs(cmd).into_iter().filter(|path| path.exists()).collect::<Vec<_>>();
    if !existing.is_empty() {
      println!("The following generated outputs will be deleted and recreated:");
      for path in existing {
        println!("  {}", path.display());
      }
    }
    print!("Proceed? [y/N] ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
      log::info!("Pipeline cancelled by user.");
      return Ok(());
    }
  } else {
    log::info!("Skipping pipeline confirmation because --yes was specified.");
  }

  let codewalker = if cmd.use_codewalker_dll { Some(init_codewalker()?) } else { None };
  clear_pipeline_outputs(cmd)?;

  log::info!("Step 1/4: extract");
  ExtractYmap {
    input_dir: cmd.source_dir.clone(),
    output_dir: cmd.extracted_dir.clone(),
    flatten: true,
    vanilla_dir: Some(cmd.vanilla_xml_dir.clone()),
  }
  .run()?;

  log::info!("Step 2/4: ymap -> xml");
  let ymap_to_xml = Ymap2Xml {
    input_dir: cmd.extracted_dir.clone(),
    output_dir: cmd.extracted_xml_dir.clone(),
  };
  if cmd.use_codewalker_dll {
    ymap_to_xml.run(codewalker.as_ref().expect("CodeWalker backend was initialized"))?;
  } else {
    ymap_to_xml.run_native()?;
  }

  log::info!("Step 3/4: merge xml");
  MergeYmapXml {
    vanilla_dir: cmd.vanilla_xml_dir.clone(),
    mod_dir: cmd.extracted_xml_dir.clone(),
    mod_ymap_dir: cmd.extracted_dir.clone(),
    output_dir: cmd.merged_xml_dir.clone(),
    rebuild_all: false,
    blacklist_config: cmd.blacklist_config.clone(),
  }
  .run()?;

  log::info!("Step 4/4: xml -> ymap");
  let xml_to_ymap = Xml2Ymap {
    input_dir: cmd.merged_xml_dir.clone(),
    output_dir: cmd.merged_dir.clone(),
  };
  if cmd.use_codewalker_dll {
    xml_to_ymap.run(codewalker.as_ref().expect("CodeWalker backend was initialized"))?;
  } else {
    xml_to_ymap.run_native(&cmd.extracted_dir)?;
  }

  log::info!("Step 5/5: merge YBN conflicts against vanilla bounds");
  MergeYbnConflicts {
    source_dir: cmd.source_dir.clone(),
    vanilla_dir: vanilla_ybn_dir,
    output_dir: cmd.workspace.join("merged_ybn"),
    omitted_files_path: cmd.extracted_dir.join("_extracted_ybns.txt"),
  }
  .run(codewalker.as_ref())?;

  if let Some(resource_dir) = &cmd.output_resource_dir {
    log::info!("Step 6/6: deploy to resource dir");
    deploy_resource(cmd, resource_dir)?;
  }

  Ok(())
}

fn pipeline_output_dirs(cmd: &Pipeline) -> Vec<std::path::PathBuf> {
  let mut outputs = vec![
    cmd.extracted_dir.clone(),
    cmd.extracted_xml_dir.clone(),
    cmd.merged_dir.clone(),
    cmd.workspace.join("merged_ybn"),
    cmd.merged_xml_dir.clone(),
    cmd.workspace.join("merged_mlo/stream"),
  ];
  if let Some(resource_dir) = &cmd.output_resource_dir {
    outputs.push(resource_dir.join("stream"));
  }
  outputs
}

fn clear_pipeline_outputs(cmd: &Pipeline) -> io::Result<()> {
  let mut cleared = Vec::new();
  for path in pipeline_output_dirs(cmd) {
    if cleared.contains(&path) {
      continue;
    }
    if path.exists() {
      fs::remove_dir_all(&path)?;
      log::info!("Removed previous generated output: {}", path.display());
    }
    cleared.push(path);
  }
  Ok(())
}

/// Overwrites a FiveM resource's `stream/ymap/{merged,clone}` and `omit.txt` with
/// this run's output.
fn deploy_resource(
  cmd: &Pipeline,
  resource_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
  let stream_ymap_dir = resource_dir.join("stream/ymap");

  copy_dir_overwrite(&cmd.merged_dir, &stream_ymap_dir.join("merged"))?;
  copy_dir_overwrite(&cmd.merged_xml_dir.join("clone"), &stream_ymap_dir.join("clone"))?;
  copy_dir_overwrite(&cmd.workspace.join("merged_ybn"), &resource_dir.join("stream/ybn/merged"))?;

  let omit_src = cmd.extracted_dir.join("_extracted_ymaps.txt");
  let omit_dest = resource_dir.join("omit.txt");
  fs::create_dir_all(resource_dir)?;
  let mut omit = fs::read_to_string(&omit_src)?;
  let omitted_ybns = fs::read_to_string(cmd.extracted_dir.join("_extracted_ybns.txt"))?;
  if !omitted_ybns.is_empty() {
    if !omit.is_empty() && !omit.ends_with('\n') {
      omit.push('\n');
    }
    omit.push_str(&omitted_ybns);
  }
  fs::write(&omit_dest, omit)?;
  log::info!("  [Success] Copied {} -> {}", omit_src.display(), omit_dest.display());

  Ok(())
}

/// Recursively replaces `dest` with a copy of `src`.
fn copy_dir_overwrite(
  src: &Path,
  dest: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
  if dest.exists() {
    fs::remove_dir_all(dest)?;
  }
  fs::create_dir_all(dest)?;
  for entry in fs::read_dir(src)? {
    let entry = entry?;
    let dest_path = dest.join(entry.file_name());
    if entry.file_type()?.is_dir() {
      copy_dir_overwrite(&entry.path(), &dest_path)?;
    } else {
      fs::copy(entry.path(), &dest_path)?;
    }
  }
  log::info!("  [Success] Copied {} -> {}", src.display(), dest.display());
  Ok(())
}

fn run_parse_ymap_xml(file_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
  log::info!("ファイルを読み込み中: {}", file_path.display());
  let xml = fs::read_to_string(file_path)?;

  // XML専用 struct に deserialize
  let xml_data: XmlYmap = from_str(&xml)?;

  // ドメイン struct に変換
  let data: Ymap = xml_data.into();

  log::debug!("{:#?}", data);

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn clears_generated_pipeline_outputs_and_stream_directories() {
    let root = std::env::temp_dir().join(format!("pipeline_outputs_{}", std::process::id()));
    let workspace = root.join("asset");
    let resource_dir = root.join("custom_resource");
    let cmd = Pipeline {
      workspace: workspace.clone(),
      source_dir: workspace.join("source"),
      output_resource_dir: Some(resource_dir),
      vanilla_xml_dir: workspace.join("vanilla/ymap.xml"),
      extracted_dir: workspace.join("extracted"),
      extracted_xml_dir: workspace.join("extracted.xml"),
      merged_xml_dir: workspace.join("merged.xml"),
      merged_dir: workspace.join("merged"),
      log_dir: workspace.join("log"),
      blacklist_config: None,
      use_codewalker_dll: false,
      yes: false,
    };

    let outputs = pipeline_output_dirs(&cmd);
    for output in &outputs {
      fs::create_dir_all(output).unwrap();
      fs::write(output.join("stale"), "old output").unwrap();
    }
    clear_pipeline_outputs(&cmd).unwrap();

    assert!(outputs.iter().all(|output| !output.exists()));
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn deploy_resource_overwrites_stream_and_omit() {
    let tmp = std::env::temp_dir().join("mlo_merger_deploy_resource_test");
    let _ = fs::remove_dir_all(&tmp);

    let merged_dir = tmp.join("asset/merged");
    let merged_ybn_dir = tmp.join("asset/merged_ybn");
    let merged_xml_dir = tmp.join("asset/merged.xml");
    let extracted_dir = tmp.join("asset/extracted");
    let resource_dir = tmp.join("asset/merged_mlo");

    fs::create_dir_all(merged_dir.join("sub")).unwrap();
    fs::write(merged_dir.join("foo.ymap"), "merged foo").unwrap();
    fs::write(merged_dir.join("sub/bar.ymap"), "merged bar").unwrap();
    fs::create_dir_all(&merged_ybn_dir).unwrap();
    fs::write(merged_ybn_dir.join("sc1_18_0.ybn"), "merged collision").unwrap();

    fs::create_dir_all(merged_xml_dir.join("clone")).unwrap();
    fs::write(merged_xml_dir.join("clone/baz.ymap"), "clone baz").unwrap();

    fs::create_dir_all(&extracted_dir).unwrap();
    fs::write(extracted_dir.join("_extracted_ymaps.txt"), "list").unwrap();
    fs::write(extracted_dir.join("_extracted_ybns.txt"), "collision source path").unwrap();

    // pre-existing stale content that must be removed by the overwrite
    fs::create_dir_all(resource_dir.join("stream/ymap/merged/stale_dir")).unwrap();
    fs::write(resource_dir.join("stream/ymap/merged/stale_dir/old.txt"), "stale").unwrap();

    let cmd = Pipeline {
      workspace: tmp.join("asset"),
      source_dir: tmp.join("asset/source"),
      output_resource_dir: Some(resource_dir.clone()),
      vanilla_xml_dir: tmp.join("asset/vanilla/ymap.xml"),
      extracted_dir,
      extracted_xml_dir: tmp.join("asset/extracted.xml"),
      merged_xml_dir,
      merged_dir,
      log_dir: tmp.join("asset/log"),
      blacklist_config: None,
      use_codewalker_dll: false,
      yes: false,
    };

    deploy_resource(&cmd, &resource_dir).unwrap();

    assert_eq!(
      fs::read_to_string(resource_dir.join("stream/ymap/merged/foo.ymap")).unwrap(),
      "merged foo"
    );
    assert_eq!(
      fs::read_to_string(resource_dir.join("stream/ymap/merged/sub/bar.ymap")).unwrap(),
      "merged bar"
    );
    assert_eq!(
      fs::read_to_string(resource_dir.join("stream/ymap/clone/baz.ymap")).unwrap(),
      "clone baz"
    );
    assert_eq!(
      fs::read_to_string(resource_dir.join("stream/ybn/merged/sc1_18_0.ybn")).unwrap(),
      "merged collision"
    );
    assert_eq!(
      fs::read_to_string(resource_dir.join("omit.txt")).unwrap(),
      "list\ncollision source path"
    );
    assert!(!resource_dir.join("stream/ymap/merged/stale_dir").exists());

    fs::remove_dir_all(&tmp).unwrap();
  }
}
