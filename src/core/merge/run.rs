use crate::core::common::function::collect_files_with_suffix;
use crate::core::format::ymap::model::Ymap;
use crate::core::format::ymap::model::ymap::YmapStructDiffEnum;
use crate::core::format::ymap::xml::XmlYmap;
use quick_xml::de::from_str;
use std::fs;
use std::path::{Path, PathBuf};
use structdiff::StructDiff;

#[derive(Debug)]
pub struct MergeYmapXml {
  pub vanilla_dir: PathBuf,
  pub mod_dir: PathBuf,
  pub output_dir: PathBuf,
}

impl MergeYmapXml {
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    println!(
      "Merging YMAP XML files from {} and {} into {}",
      self.vanilla_dir.display(),
      self.mod_dir.display(),
      self.output_dir.display()
    );

    let vanila_files = collect_files_with_suffix(&self.vanilla_dir, ".ymap.xml");
    println!(
      "Found {} YMAP XML files in vanilla directory",
      vanila_files.len()
    );

    // Phase 1.1: mod_dir 内の .ymap.xml ファイルを検出
    let mod_files = collect_files_with_suffix(&self.mod_dir, ".ymap.xml");
    println!("Found {} YMAP XML files in mod directory", mod_files.len());

    // Phase 1.2 & 1.3: vanilla_dir 内に対応するファイルが存在するかチェックし、ペアを作成
    let file_pairs = create_file_pairs(&self.vanilla_dir, &self.mod_dir, &mod_files)?;
    println!("Found {} matching file pairs", file_pairs.len());

    for pair in &file_pairs {
      let file_name = pair.vanilla.file_name().unwrap_or_default();
      println!("Processing: {}", pair.relative_path.display());
      println!("  Vanilla: {}", pair.vanilla.display());
      println!("  Mod:     {}", pair.mod_file.display());

      // Phase 2: XMLをパースして Ymap に変換
      let vanilla_ymap = parse_ymap_xml(&pair.vanilla)?;
      let mod_ymap = parse_ymap_xml(&pair.mod_file)?;

      println!("  Vanilla entities: {}", vanilla_ymap.entity_map.len());
      println!("  Mod entities:     {}", mod_ymap.entity_map.len());

      let diffs = vanilla_ymap.diff(&mod_ymap);
      for diff in &diffs {
        match diff {
          YmapStructDiffEnum::entity_map(_) => (),
          it => println!("[warning] found unsupported changes: {:?}", it),
        }
      }
    }

    Ok(())
  }
}

/// vanilla と mod のファイルペアを作成
#[derive(Debug)]
struct FilePair {
  vanilla: PathBuf,
  mod_file: PathBuf,
  relative_path: PathBuf,
}

fn create_file_pairs(
  vanilla_dir: &Path,
  mod_dir: &Path,
  mod_files: &[PathBuf],
) -> Result<Vec<FilePair>, Box<dyn std::error::Error>> {
  let mut pairs = Vec::new();

  for mod_file in mod_files {
    // mod_dir からの相対パスを取得
    let relative_path = mod_file.strip_prefix(mod_dir)?;

    // vanilla_dir 内の対応するファイルパスを構築
    let vanilla_file = vanilla_dir.join(relative_path);

    // vanilla ファイルが存在する場合のみペアに追加
    if vanilla_file.exists() {
      pairs.push(FilePair {
        vanilla: vanilla_file,
        mod_file: mod_file.clone(),
        relative_path: relative_path.to_path_buf(),
      });
    } else {
      println!(
        "Warning: No corresponding vanilla file found for {}",
        relative_path.display()
      );
    }
  }

  Ok(pairs)
}

/// YMAP XML ファイルをパースして Ymap 構造体に変換
fn parse_ymap_xml(file_path: &Path) -> Result<Ymap, Box<dyn std::error::Error>> {
  let xml_content = fs::read_to_string(file_path)?;
  let xml_ymap: XmlYmap = from_str(&xml_content)?;
  Ok(xml_ymap.into())
}
