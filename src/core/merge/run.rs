use crate::core::common::Position;
use crate::core::common::function::collect_files_with_suffix;
use crate::core::extract::ExtractYmap;
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::format::ymap::model::ymap::YmapStructDiffEnum;
use crate::core::format::ymap::xml::XmlYmap;
use crate::core::merge::check_box_occluder_diff::check_box_occluder_diff;
use crate::core::merge::check_entity_diff::check_entity_diff;
use crate::core::merge::check_occlude_models_diff;
use crate::core::merge::check_occlude_models_diff::check_occlude_models_diff;
use quick_xml::de::from_str;
use std::collections::HashMap;
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
    println!("Found {} YMAP XML files in vanilla directory", vanila_files.len());

    let modded_ymaps_map = collect_modded_ymaps_map(&self.mod_dir)?;
    println!("Found {} unique YMAP files across mods", modded_ymaps_map.len());

    for (ymap_name, mod_refs) in &modded_ymaps_map {
      let vanilla_ymap_path = self.vanilla_dir.join(ymap_name);
      println!("\nProcessing YMAP: {}", vanilla_ymap_path.display());

      let vanilla_ymap = parse_ymap_xml(&vanilla_ymap_path)?;

      for mod_info in mod_refs {
        println!("    Mod: {} ({})", mod_info.mod_name, mod_info.mod_ymap_path.display());

        let mod_ymap = parse_ymap_xml(&mod_info.mod_ymap_path)?;
        let diffs = vanilla_ymap.diff(&mod_ymap);
        for diff in &diffs {
          match diff {
            YmapStructDiffEnum::entity_map(_) => {
              let entity_diffs = check_entity_diff(&vanilla_ymap, &mod_ymap);
              println!("    Found {} entity differences", entity_diffs.len());
              for entity_diff in entity_diffs {
                match entity_diff {
                  crate::core::merge::check_entity_diff::YmapEntityDiff::Added(e) => {
                    println!("      [Added] Entity: {} {}", e.guid, e.archetype_name);
                  }
                  crate::core::merge::check_entity_diff::YmapEntityDiff::Removed(e) => {
                    println!("      [Removed] Entity: {} {}", e.guid, e.archetype_name);
                  }
                  crate::core::merge::check_entity_diff::YmapEntityDiff::Modified {
                    vanilla: _,
                    diffs,
                  } => {
                    println!("      [Modified] Diffs: {:?}", diffs);
                  }
                }
              }
            }
            YmapStructDiffEnum::box_occluders(_) => {
              let box_occluder_diffs = check_box_occluder_diff(&vanilla_ymap, &mod_ymap);
              println!("    Found {} box occluder differences", box_occluder_diffs.len());
              for box_occluder_diff in box_occluder_diffs {
                match box_occluder_diff {
                  crate::core::merge::check_box_occluder_diff::YmapBoxOccluderDiff::Added(b) => {
                    println!(
                      "      [Added] Box Occluder: {:?}",
                      Position {
                        x: b.i_center_x as f32,
                        y: b.i_center_y as f32,
                        z: b.i_center_z as f32,
                      }
                    );
                  }
                  crate::core::merge::check_box_occluder_diff::YmapBoxOccluderDiff::Removed(b) => {
                    println!(
                      "      [Removed] Box Occluder: {:?}",
                      Position {
                        x: b.i_center_x as f32,
                        y: b.i_center_y as f32,
                        z: b.i_center_z as f32,
                      }
                    );
                  }
                  crate::core::merge::check_box_occluder_diff::YmapBoxOccluderDiff::Modified {
                    vanilla: _,
                    diffs,
                  } => {
                    println!("      [Modified] Diffs: {:?}", diffs);
                  }
                }
              }
            }
            YmapStructDiffEnum::occlude_models(_) => {
              let occlude_model_diffs = check_occlude_models_diff(&vanilla_ymap, &mod_ymap);
              println!("    Found {} occlude model  differences", occlude_model_diffs.len());
            }
            YmapStructDiffEnum::block(_) => (),
            it => println!("    [warning] skip unsupported changes: {:?}", it),
          }
        }
      }
    }

    Ok(())
  }
}

/// Information about a modded ymap file
#[derive(Debug, Clone)]
struct ModYmapReference {
  mod_name: String,
  mod_ymap_path: PathBuf,
}

fn collect_modded_ymaps_map(
  mod_dir: &Path
) -> Result<HashMap<String, Vec<ModYmapReference>>, Box<dyn std::error::Error>> {
  let mod_files = collect_files_with_suffix(mod_dir, ".ymap.xml");
  let mut map: HashMap<String, Vec<ModYmapReference>> = HashMap::new();

  for mod_file in mod_files {
    // Extract file name
    let file_name = mod_file.file_name().and_then(|n| n.to_str()).ok_or("Invalid file name")?;

    // Extract mod name and ymap name from "modname___ymapname.ymap.xml" format
    if let Some((mod_name, ymap_name)) = file_name.split_once(ExtractYmap::FLATTEN_DELIMITER) {
      let info = ModYmapReference {
        mod_name: mod_name.to_string(),
        mod_ymap_path: mod_file.clone(),
      };

      map.entry(ymap_name.to_string()).or_default().push(info);
    } else {
      println!("Warning: Invalid file name format (expected 'mod___ymap.ymap.xml'): {}", file_name);
    }
  }

  Ok(map)
}

fn parse_ymap_xml(file_path: &Path) -> Result<Ymap, Box<dyn std::error::Error>> {
  let xml_content = fs::read_to_string(file_path)?;
  let xml_ymap: XmlYmap = from_str(&xml_content)?;
  Ok(xml_ymap.into())
}
