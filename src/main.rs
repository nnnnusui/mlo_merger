use mlo_merger::{
  cli::command::{Command, Pipeline, parse_args},
  core::{
    codewalker::CodeWalker,
    extract::ExtractYmap,
    format::ymap::{model::Ymap, xml::XmlYmap},
    merge::run::MergeYmapXml,
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

  CombinedLogger::init(vec![
    TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
    WriteLogger::new(LevelFilter::Info, Config::default(), File::create("mlo_merger.log")?),
  ])?;

  log::info!("Options: {:?}", opts);

  match opts {
    Command::ParseYmapXml(cmd) => run_parse_ymap_xml(&cmd.input)?,
    Command::MergeYmapXml(cmd) => cmd.run()?,
    Command::ExtractYmap(cmd) => cmd.run()?,
    Command::GetProp(cmd) => cmd.run()?,
    Command::Pipeline(cmd) => run_pipeline(&cmd)?,
  };

  log::info!("✓ Command completed successfully");
  Ok(())
}

fn run_pipeline(cmd: &Pipeline) -> Result<(), Box<dyn std::error::Error>> {
  println!("This will run the full pipeline (workspace: {}):", cmd.workspace.display());
  println!("  1. extract   {} -> {}", cmd.source_dir.display(), cmd.extracted_dir.display());
  println!("  2. ymap2xml  {} -> {}", cmd.extracted_dir.display(), cmd.extracted_xml_dir.display());
  println!(
    "  3. mergexml  {} + {} -> {}",
    cmd.vanilla_xml_dir.display(),
    cmd.extracted_xml_dir.display(),
    cmd.merged_xml_dir.display()
  );
  println!("  4. xml2ymap  {} -> {}", cmd.merged_xml_dir.display(), cmd.merged_dir.display());
  if let Some(resource_dir) = &cmd.output_resource_dir {
    println!("  5. deploy    {} -> {}", cmd.merged_dir.display(), resource_dir.display());
  }
  print!("Proceed? [y/N] ");
  io::stdout().flush()?;
  let mut answer = String::new();
  io::stdin().read_line(&mut answer)?;
  if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
    log::info!("Pipeline cancelled by user.");
    return Ok(());
  }

  let bridge_dll =
    std::env::var("CODEWALKER_BRIDGE_DLL").map(std::path::PathBuf::from).unwrap_or_else(|_| {
      std::path::PathBuf::from("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll")
    });
  let codewalker = CodeWalker::init(&bridge_dll).map_err(|e| {
    format!(
      "Failed to load CodeWalker.Bridge from {} ({e}). Build it with `dotnet publish bridge/CodeWalker.Bridge -c Release -o bridge/CodeWalker.Bridge/bin/publish -p:CodeWalkerCoreDllPath=<path to CodeWalker.Core.dll>`, or set CODEWALKER_BRIDGE_DLL.",
      bridge_dll.display()
    )
  })?;

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
  if cmd.native_ymap_to_xml {
    ymap_to_xml.run_native()?;
  } else {
    ymap_to_xml.run(&codewalker)?;
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
  Xml2Ymap {
    input_dir: cmd.merged_xml_dir.clone(),
    output_dir: cmd.merged_dir.clone(),
  }
  .run(&codewalker)?;

  if let Some(resource_dir) = &cmd.output_resource_dir {
    log::info!("Step 5/5: deploy to resource dir");
    deploy_resource(cmd, resource_dir)?;
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

  let omit_src = cmd.extracted_dir.join("_extracted_ymaps.txt");
  let omit_dest = resource_dir.join("omit.txt");
  fs::create_dir_all(resource_dir)?;
  fs::copy(&omit_src, &omit_dest)?;
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
  fn deploy_resource_overwrites_stream_and_omit() {
    let tmp = std::env::temp_dir().join("mlo_merger_deploy_resource_test");
    let _ = fs::remove_dir_all(&tmp);

    let merged_dir = tmp.join("asset/merged");
    let merged_xml_dir = tmp.join("asset/merged.xml");
    let extracted_dir = tmp.join("asset/extracted");
    let resource_dir = tmp.join("asset/merged_mlo");

    fs::create_dir_all(merged_dir.join("sub")).unwrap();
    fs::write(merged_dir.join("foo.ymap"), "merged foo").unwrap();
    fs::write(merged_dir.join("sub/bar.ymap"), "merged bar").unwrap();

    fs::create_dir_all(merged_xml_dir.join("clone")).unwrap();
    fs::write(merged_xml_dir.join("clone/baz.ymap"), "clone baz").unwrap();

    fs::create_dir_all(&extracted_dir).unwrap();
    fs::write(extracted_dir.join("_extracted_ymaps.txt"), "list").unwrap();

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
      native_ymap_to_xml: false,
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
    assert_eq!(fs::read_to_string(resource_dir.join("omit.txt")).unwrap(), "list");
    assert!(!resource_dir.join("stream/ymap/merged/stale_dir").exists());

    fs::remove_dir_all(&tmp).unwrap();
  }
}
