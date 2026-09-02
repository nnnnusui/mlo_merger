use crate::core::common::function::collect_files_with_suffix;
use crate::core::config::blacklist::BlacklistConfig;
use crate::core::extract::ExtractYmap;
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::format::ymap::xml::XmlYmap;
use crate::core::merge::ymap_diff::YmapDiff;
use quick_xml::de::from_str;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct MergeYmapXml {
  pub vanilla_dir: PathBuf,
  pub mod_dir: PathBuf,
  pub mod_ymap_dir: PathBuf,
  pub output_dir: PathBuf,
  pub rebuild_all: bool,
  pub blacklist_config: Option<PathBuf>,
}

impl MergeYmapXml {
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    log::info!(
      "Merging YMAP XML files from {} and {} into {}",
      self.vanilla_dir.display(),
      self.mod_dir.display(),
      self.output_dir.display()
    );

    // Load blacklist configuration if provided
    let blacklist = if let Some(blacklist_path) = &self.blacklist_config {
      log::info!("Loading blacklist configuration from {}", blacklist_path.display());
      let blacklist_content = fs::read_to_string(blacklist_path)?;
      let config: BlacklistConfig = toml::from_str(&blacklist_content)?;
      log::info!("Loaded {} occlude model blacklist entries", config.occlude_models.len());
      Some(config)
    } else {
      None
    };

    let vanila_files = collect_files_with_suffix(&self.vanilla_dir, ".ymap.xml");
    log::info!("Found {} YMAP XML files in vanilla directory", vanila_files.len());

    let modded_ymaps_map = collect_modded_ymaps_map(&self.mod_dir)?;
    log::info!("Found {} unique YMAP files across mods", modded_ymaps_map.len());

    let mut copy_targets = Vec::new();

    for (ymap_name, mod_refs) in &modded_ymaps_map {
      if !self.rebuild_all && mod_refs.len() <= 1 {
        let mod_ref = mod_refs.first().unwrap();
        copy_targets.push(mod_ref);
        log::info!("Coppied YMAP: {} (only one mod reference: {})", ymap_name, mod_ref.mod_name);
        continue;
      }
      let vanilla_ymap_path = self.vanilla_dir.join(ymap_name);
      log::info!("Processing YMAP: {}", vanilla_ymap_path.display());

      let vanilla_ymap = parse_ymap_xml(&vanilla_ymap_path)?;

      let mut ymap_diffs = Vec::new();
      for mod_info in mod_refs {
        log::info!("  Mod: {} ({})", mod_info.mod_name, mod_info.mod_ymap_path.display());

        let mod_ymap = parse_ymap_xml(&mod_info.mod_ymap_path)?;
        let ymap_diff = YmapDiff::extract_from(&vanilla_ymap, &mod_ymap);
        ymap_diffs.push(ymap_diff);
      }

      let merged_diff = ymap_diffs.into_iter().reduce(|acc, d| acc.merge(d)).unwrap();
      let merged_ymap = merged_diff.apply_to(&vanilla_ymap, blacklist.as_ref());

      // Convert Ymap to XmlYmap and serialize to XML with 2-space indentation
      let xml_ymap: XmlYmap = merged_ymap.into();
      let mut xml_string = String::new();
      let mut serializer = quick_xml::se::Serializer::new(&mut xml_string);
      serializer.indent(' ', 2);
      xml_ymap.serialize(serializer)?;

      // Write to file
      let ymap_xml_path = self.output_dir.join(ymap_name);
      fs::create_dir_all(ymap_xml_path.parent().unwrap())?;
      fs::write(&ymap_xml_path, xml_string)?;
      log::info!("  [Success] Wrote merged YMAP to: {}", ymap_xml_path.display());
    }

    let copy_targets_txt = self.output_dir.join("_copy_targets.txt");
    fs::create_dir_all(copy_targets_txt.parent().unwrap())?;
    let clone_ymap_dir = self.output_dir.join("clone");
    fs::create_dir_all(&clone_ymap_dir)?;
    let mut copy_targets_file = fs::File::create(copy_targets_txt)?;
    for target in copy_targets {
      let ymap_xml_name = target.mod_ymap_path.file_name().unwrap().to_string_lossy();
      let extracted_ymap_name = ymap_xml_name.trim_end_matches(".xml");
      use std::io::Write;
      writeln!(copy_targets_file, "{}", ymap_xml_name)?;
      let dest_path = clone_ymap_dir.join(target.ymap_name.as_str());
      fs::copy(self.mod_ymap_dir.join(extracted_ymap_name), &dest_path)?;
    }

    Ok(())
  }
}

/// Information about a modded ymap file
#[derive(Debug, Clone)]
struct ModYmapReference {
  mod_name: String,
  ymap_name: String,
  mod_ymap_path: PathBuf,
}

fn collect_modded_ymaps_map(
  mod_dir: &Path
) -> Result<BTreeMap<String, Vec<ModYmapReference>>, Box<dyn std::error::Error>> {
  let mod_files = collect_files_with_suffix(mod_dir, ".ymap.xml");
  let mut map: BTreeMap<String, Vec<ModYmapReference>> = BTreeMap::new();

  for mod_file in mod_files {
    // Extract file name
    let file_name = mod_file.file_name().and_then(|n| n.to_str()).ok_or("Invalid file name")?;

    // Extract mod name and ymap name from "modname___ymapname.ymap.xml" format
    if let Some((mod_name, ymap_xml_name)) = file_name.split_once(ExtractYmap::FLATTEN_DELIMITER) {
      let ymap_name = ymap_xml_name.trim_end_matches(".xml");
      let info = ModYmapReference {
        mod_name: mod_name.to_string(),
        ymap_name: ymap_name.to_string(),
        mod_ymap_path: mod_file.clone(),
      };

      map.entry(ymap_xml_name.to_string()).or_default().push(info);
    } else {
      log::warn!(
        "Warning: Invalid file name format (expected 'mod___ymap.ymap.xml'): {}",
        file_name
      );
    }
  }

  Ok(map)
}

fn parse_ymap_xml(file_path: &Path) -> Result<Ymap, Box<dyn std::error::Error>> {
  let xml_content = fs::read_to_string(file_path)?;
  let xml_ymap: XmlYmap = from_str(&xml_content)?;
  Ok(xml_ymap.into())
}
