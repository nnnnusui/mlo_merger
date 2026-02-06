use mlo_merger::{
  cli::command::{Command, parse_args},
  core::format::ymap::{model::Ymap, xml::XmlYmap},
};
use quick_xml::de::from_str;
use simplelog::*;
use std::fs::{self, File};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // ログ設定: ファイルとターミナル両方に出力
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
