use std::fs;
use std::path::PathBuf;

use crate::core::codewalker::CodeWalker;
use crate::core::common::function::collect_files_with_suffix;

/// Batch-converts binary `.ymap` files into `.ymap.xml`, using CodeWalker.Core
/// hosted in-process. Runs a preload pass first so that cross-references between
/// the converted files resolve to names instead of `hash_XXXXXXXX`.
#[derive(Debug)]
pub struct Ymap2Xml {
  pub input_dir: PathBuf,
  pub output_dir: PathBuf,
}

impl Ymap2Xml {
  pub fn run(
    &self,
    codewalker: &CodeWalker,
  ) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&self.output_dir)?;

    let inputs = collect_files_with_suffix(&self.input_dir, ".ymap");
    log::info!("Found {} .ymap files to convert to xml.", inputs.len());

    for input in &inputs {
      if let Err(e) = codewalker.preload_names(input) {
        log::warn!("Failed to preload names from {}: {e}", input.display());
      }
    }

    let mut converted = 0usize;
    for input in &inputs {
      let relative = input.strip_prefix(&self.input_dir)?;
      let output = self.output_dir.join(relative).with_extension("ymap.xml");
      if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
      }
      match codewalker.ymap_to_xml(input, &output) {
        Ok(()) => converted += 1,
        Err(e) => log::error!("Failed to convert {} to xml: {e}", input.display()),
      }
    }

    log::info!("Converted {converted}/{} ymap files to xml.", inputs.len());
    Ok(())
  }
}

/// Batch-converts `.ymap.xml` files back into binary `.ymap` files, using
/// CodeWalker.Core hosted in-process.
#[derive(Debug)]
pub struct Xml2Ymap {
  pub input_dir: PathBuf,
  pub output_dir: PathBuf,
}

impl Xml2Ymap {
  pub fn run(
    &self,
    codewalker: &CodeWalker,
  ) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&self.output_dir)?;

    let inputs = collect_files_with_suffix(&self.input_dir, ".ymap.xml");
    log::info!("Found {} .ymap.xml files to convert to ymap.", inputs.len());

    let mut converted = 0usize;
    for input in &inputs {
      let relative = input.strip_prefix(&self.input_dir)?;
      let output = self.output_dir.join(relative).with_extension("");
      if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
      }
      match codewalker.xml_to_ymap(input, &output) {
        Ok(()) => converted += 1,
        Err(e) => log::error!("Failed to convert {} to ymap: {e}", input.display()),
      }
    }

    log::info!("Converted {converted}/{} xml files to ymap.", inputs.len());
    Ok(())
  }
}
