use mlo_merger::{
  cli::command::{Command, parse_args},
  core::format::ymap::{model::Ymap, xml::XmlYmap},
};
use quick_xml::de::from_str;
use simplelog::*;
use std::fs::{self, File};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Archive existing log file if it exists
  let log_path = Path::new("mlo_merger.log");
  if log_path.exists() {
    let log_archive_dir = Path::new("asset/log");
    fs::create_dir_all(log_archive_dir)?;

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

  let opts = parse_args();
  log::info!("Options: {:?}", opts);

  match opts {
    Command::ParseYmapXml(cmd) => run_parse_ymap_xml(&cmd.input)?,
    Command::MergeYmapXml(cmd) => cmd.run()?,
    Command::ExtractYmap(cmd) => cmd.run()?,
  };

  log::info!("✓ Command completed successfully");
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
