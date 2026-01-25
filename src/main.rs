use mlo_merger::cli::{Command, parse_args};
use mlo_merger::merge::run_merge_ymap_xml;
use mlo_merger::ymap::{model::Ymap, xml::XmlYmap};
use quick_xml::de::from_str;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let opts = parse_args();
  println!("Options: {:?}", opts);

  match opts {
    Command::ParseYmapXml(cmd) => run_parse_ymap_xml(&cmd.input)?,
    Command::ParseYmap(cmd) => run_parse_ymap(&cmd.example)?,
    Command::MergeYmapXml(cmd) => run_merge_ymap_xml(cmd)?,
    Command::ExtractYmap(cmd) => cmd.run()?,
  }

  Ok(())
}

fn run_parse_ymap_xml(file_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
  println!("ファイルを読み込み中: {}", file_path.display());
  let xml = fs::read_to_string(file_path)?;

  // XML専用 struct に deserialize
  let xml_data: XmlYmap = from_str(&xml)?;

  // ドメイン struct に変換
  let data: Ymap = xml_data.into();

  println!("{:#?}", data);

  Ok(())
}

fn run_parse_ymap(file_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
  println!("YMAPファイルを読み込み中: {}", file_path.display());
  // TODO: YMAP binary format parsing
  println!("YMAP parsing is not yet implemented");
  Ok(())
}
