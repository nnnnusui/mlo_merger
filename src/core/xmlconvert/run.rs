use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::core::codewalker::CodeWalker;
use crate::core::common::function::collect_files_with_suffix;
use crate::core::format::gamefile::{
  meta_resource::MetaResource, meta_xml::ymap_to_xml, resource_file::Rsc7Resource,
};

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

  /// Batch-converts binary `.ymap` files to XML using the native Rust parser.
  pub fn run_native(&self) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&self.output_dir)?;

    let inputs = collect_files_with_suffix(&self.input_dir, ".ymap");
    log::info!("Found {} .ymap files to convert to xml natively.", inputs.len());

    let mut shared_hash_names = HashMap::new();
    for input in &inputs {
      let result = fs::read(input)
        .and_then(|bytes| Rsc7Resource::decode(&bytes))
        .and_then(|resource| MetaResource::parse(&resource));
      match result {
        Ok(meta) => shared_hash_names.extend(meta.hash_names()),
        Err(error) => {
          log::warn!("Failed to preload native hash names from {}: {error}", input.display())
        }
      }
    }

    let mut converted = 0usize;
    for input in &inputs {
      let relative = input.strip_prefix(&self.input_dir)?;
      let output = self.output_dir.join(relative).with_extension("ymap.xml");
      if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
      }
      match fs::read(input).and_then(|bytes| ymap_to_xml(&bytes, &shared_hash_names)) {
        Ok(xml) => {
          fs::write(&output, xml)?;
          converted += 1;
        }
        Err(error) => log::error!("Failed to convert {} to xml natively: {error}", input.display()),
      }
    }

    log::info!("Natively converted {converted}/{} ymap files to xml.", inputs.len());
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

#[cfg(test)]
mod tests {
  use super::Ymap2Xml;

  #[test]
  fn native_batch_conversion_writes_ymap_xml() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/extracted/brofx_mansion_06___apa_ch2_occl_05.ymap");
    let temp_dir =
      std::env::temp_dir().join(format!("mlo_merger_native_batch_{}", std::process::id()));
    let input_dir = temp_dir.join("input");
    let output_dir = temp_dir.join("output");
    std::fs::create_dir_all(&input_dir).unwrap();
    std::fs::copy(&fixture, input_dir.join(fixture.file_name().unwrap())).unwrap();

    Ymap2Xml {
      input_dir,
      output_dir: output_dir.clone(),
    }
    .run_native()
    .unwrap();

    let output = output_dir.join("brofx_mansion_06___apa_ch2_occl_05.ymap.xml");
    let xml = std::fs::read_to_string(output).unwrap();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<CMapData>"));

    std::fs::remove_dir_all(temp_dir).unwrap();
  }
}
