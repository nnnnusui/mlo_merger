use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use quick_xml::de::from_str;

use crate::core::common::function::collect_files_with_suffix;
use crate::core::format::ymap::xml::XmlYmap;

#[derive(Debug)]
pub struct GetProp {
  pub input: PathBuf,
}

impl GetProp {
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    let files = if self.input.is_file() {
      vec![self.input.clone()]
    } else {
      collect_files_with_suffix(&self.input, ".ymap.xml")
    };

    let mut names: BTreeSet<String> = BTreeSet::new();

    for path in &files {
      let xml = fs::read_to_string(path)?;
      let ymap: XmlYmap = from_str(&xml)?;
      for entity in &ymap.entities.items {
        names.insert(entity.archetype_name.clone());
      }
    }

    for name in &names {
      println!("{}", name);
    }

    log::info!("Found {} unique prop names from {} file(s).", names.len(), files.len());
    Ok(())
  }
}
