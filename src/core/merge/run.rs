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

#[derive(Debug, Clone)]
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
  if let Some(error) = &xml_ymap.instanced_data.error {
    return Err(
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{} contains an instancedData error: {error}", file_path.display()),
      )
      .into(),
    );
  }
  Ok(xml_ymap.into())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    format::ymap::model::{GrassInstance, GrassInstanceBatch},
    merge::{ymap_instanced_data_diff::BatchKey, ymap_metadata_diff::reference_hash},
  };
  use std::collections::{BTreeSet, HashMap, HashSet};

  #[test]
  #[ignore = "requires local vanilla XML, extracted mods, and the original pipeline log"]
  fn merge_logged_unsupported_ymaps() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let old_log =
      fs::read_to_string(base.join("asset/log/mlo_merger_20261002_154241.log")).unwrap();
    let mut current = None;
    let mut targets = BTreeSet::new();
    for line in old_log.lines() {
      if let Some((_, path)) = line.split_once("Processing YMAP: ") {
        current = Some(Path::new(path).file_name().unwrap().to_str().unwrap().to_string());
      }
      if ["parent(", "physics_dictionaries(", "instanced_data("]
        .iter()
        .any(|field| line.contains(&format!("skip unsupported changes: {field}")))
      {
        targets.insert(current.clone().expect("warning without YMAP context"));
      }
    }
    assert!(!targets.is_empty(), "no unsupported-field fixtures found");
    let staging = std::env::temp_dir().join(format!("mlo_merge_metadata_{}", std::process::id()));
    let vanilla_dir = staging.join("vanilla.xml");
    let mod_dir = staging.join("mod.xml");
    fs::create_dir_all(&vanilla_dir).unwrap();
    fs::create_dir_all(&mod_dir).unwrap();
    let output_dir = base.join("asset/merge_validation/unsupported_fields/merged.xml");
    let report_dir = output_dir.parent().unwrap();
    fs::create_dir_all(report_dir).unwrap();
    simplelog::WriteLogger::init(
      log::LevelFilter::Info,
      simplelog::Config::default(),
      fs::File::create(report_dir.join("merge.log")).unwrap(),
    )
    .unwrap();
    for name in &targets {
      fs::copy(base.join("asset/vanilla/ymap.xml").join(name), vanilla_dir.join(name))
        .unwrap_or_else(|error| panic!("Native vanilla XML missing for {name}: {error}"));
    }
    let sources = collect_modded_ymaps_map(&base.join("asset/extracted.xml")).unwrap();
    let mut mod_count = 0;
    for name in &targets {
      let references = sources.get(name).unwrap_or_else(|| panic!("no mod references for {name}"));
      for source in references {
        fs::copy(&source.mod_ymap_path, mod_dir.join(source.mod_ymap_path.file_name().unwrap()))
          .unwrap();
        mod_count += 1;
      }
    }
    let runner = MergeYmapXml {
      vanilla_dir,
      mod_dir,
      mod_ymap_dir: base.join("asset/extracted"),
      output_dir: output_dir.clone(),
      rebuild_all: true,
      blacklist_config: Some(base.join("asset/blacklist.toml")),
    };
    runner.run().unwrap();
    let mut records = Vec::new();
    for name in &targets {
      let original = parse_ymap_xml(&runner.vanilla_dir.join(name)).unwrap();
      let mods = sources[name]
        .iter()
        .map(|source| parse_ymap_xml(&source.mod_ymap_path).unwrap())
        .collect::<Vec<_>>();
      let merged = parse_ymap_xml(&output_dir.join(name)).unwrap();
      let expected_parent = mods
        .iter()
        .find(|modified| reference_hash(&modified.parent) != reference_hash(&original.parent))
        .map(|modified| &modified.parent)
        .unwrap_or(&original.parent);
      assert_eq!(reference_hash(&merged.parent), reference_hash(expected_parent), "parent: {name}");
      let original_refs = original
        .physics_dictionaries
        .iter()
        .map(|name| reference_hash(name))
        .collect::<HashSet<_>>();
      let modified_refs = mods
        .iter()
        .map(|modified| {
          modified
            .physics_dictionaries
            .iter()
            .map(|name| reference_hash(name))
            .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
      let mut expected_refs = original_refs
        .iter()
        .filter(|hash| modified_refs.iter().all(|refs| refs.contains(hash)))
        .copied()
        .collect::<HashSet<_>>();
      for refs in &modified_refs {
        expected_refs.extend(refs.difference(&original_refs).copied());
      }
      let actual_refs =
        merged.physics_dictionaries.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
      assert_eq!(actual_refs, expected_refs, "physics dictionaries: {name}");
      assert_eq!(
        actual_refs.len(),
        merged.physics_dictionaries.len(),
        "duplicate dictionaries: {name}"
      );
      verify_grass(&original, &mods, &merged, name);
      let instances = merged
        .instanced_data
        .grass_instance_list
        .iter()
        .map(|batch| batch.instances.len())
        .sum::<usize>();
      records.push(serde_json::json!({
        "ymap": name, "mods": sources[name].iter().map(|source| &source.mod_name).collect::<Vec<_>>(),
        "parent": merged.parent, "physics_dictionaries": merged.physics_dictionaries.len(),
        "grass_batches": merged.instanced_data.grass_instance_list.len(), "grass_instances": instances,
      }));
    }
    fs::write(report_dir.join("results.json"), serde_json::to_vec_pretty(&records).unwrap())
      .unwrap();
    let merge_log = fs::read_to_string(report_dir.join("merge.log")).unwrap();
    assert!(
      !merge_log.contains("skip unsupported changes"),
      "unsupported changes remain in targeted merge log"
    );
    fs::remove_dir_all(staging).unwrap();
    eprintln!(
      "Verified {} targeted YMAPs across {mod_count} mod references; output: {}",
      targets.len(),
      output_dir.display()
    );
  }

  fn positions(batch: &GrassInstanceBatch) -> BTreeMap<Vec<u32>, GrassInstance> {
    batch
      .instances
      .iter()
      .map(|instance| {
        (
          instance
            .position
            .iter()
            .map(|value| if *value == 0.0 { 0 } else { value.to_bits() })
            .collect(),
          instance.clone(),
        )
      })
      .collect()
  }

  fn verify_grass(
    original: &Ymap,
    mods: &[Ymap],
    merged: &Ymap,
    name: &str,
  ) {
    let original_batches = original
      .instanced_data
      .grass_instance_list
      .iter()
      .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
      .collect::<HashMap<_, _>>();
    let mod_batches = mods
      .iter()
      .map(|modified| {
        modified
          .instanced_data
          .grass_instance_list
          .iter()
          .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
          .collect::<HashMap<_, _>>()
      })
      .collect::<Vec<_>>();
    let keys = original_batches
      .keys()
      .chain(mod_batches.iter().flat_map(|batches| batches.keys()))
      .cloned()
      .collect::<HashSet<_>>();
    let mut expected = HashMap::new();
    for key in keys {
      let before = original_batches.get(&key);
      if before.is_some() && mod_batches.iter().any(|batches| !batches.contains_key(&key)) {
        continue;
      }
      let empty = BTreeMap::new();
      let before = before.unwrap_or(&empty);
      let mut instances = before.clone();
      instances.retain(|position, _| {
        mod_batches.iter().all(|batches| {
          batches.get(&key).is_some_and(|instances| instances.contains_key(position))
        })
      });
      let removed = before
        .keys()
        .filter(|position| !instances.contains_key(*position))
        .cloned()
        .collect::<HashSet<_>>();
      let mut selected = HashSet::new();
      for batches in &mod_batches {
        if let Some(modified) = batches.get(&key) {
          for (position, instance) in modified {
            if !removed.contains(position)
              && before.get(position) != Some(instance)
              && selected.insert(position.clone())
            {
              instances.insert(position.clone(), instance.clone());
            }
          }
        }
      }
      expected.insert(key, instances);
    }
    let actual = merged
      .instanced_data
      .grass_instance_list
      .iter()
      .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
      .collect::<HashMap<_, _>>();
    assert!(
      actual == expected,
      "grass deltas differ for {name}: actual {} batches, expected {}",
      actual.len(),
      expected.len()
    );
    assert_eq!(
      actual.len(),
      merged.instanced_data.grass_instance_list.len(),
      "duplicate grass batches: {name}"
    );
  }
}
