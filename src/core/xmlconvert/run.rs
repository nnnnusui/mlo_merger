use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::core::codewalker::CodeWalker;
use crate::core::common::function::collect_files_with_suffix;
use crate::core::format::gamefile::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  meta_xml::ymap_to_xml,
  resource_file::Rsc7Resource,
  xml_meta_builder::meta_from_xml,
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
    crate::core::format::gamefile::resource_convert::prune_managed_ymap_outputs(
      &self.input_dir,
      &self.output_dir,
    )?;

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

  /// Batch-converts YMAP XML to binary using schemas gathered from binary YMAP files.
  pub fn run_native(
    &self,
    schema_dir: &std::path::Path,
  ) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&self.output_dir)?;
    crate::core::format::gamefile::resource_convert::prune_managed_ymap_outputs(
      &self.input_dir,
      &self.output_dir,
    )?;

    let inputs = collect_files_with_suffix(&self.input_dir, ".ymap.xml");
    log::info!("Found {} .ymap.xml files to convert to ymap natively.", inputs.len());

    let mut catalog = MetaSchemaCatalog::default();
    for source in collect_files_with_suffix(schema_dir, ".ymap") {
      let result = fs::read(&source)
        .and_then(|bytes| Rsc7Resource::decode(&bytes))
        .and_then(|resource| MetaResource::parse(&resource));
      match result {
        Ok(meta) => catalog.add_resource(&meta),
        Err(error) => {
          log::warn!("Failed to preload native META schemas from {}: {error}", source.display())
        }
      }
    }

    let mut converted = 0usize;
    for input in &inputs {
      let relative = input.strip_prefix(&self.input_dir)?;
      let output = self.output_dir.join(relative).with_extension("");
      if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
      }
      let result = fs::read_to_string(input).and_then(|xml| {
        let meta = meta_from_xml(&xml, &catalog)?;
        let resource = meta.to_rsc7(2)?;
        resource.encode()
      });
      match result {
        Ok(bytes) => {
          fs::write(&output, bytes)?;
          converted += 1;
        }
        Err(error) => {
          log::error!("Failed to convert {} to ymap natively: {error}", input.display())
        }
      }
    }

    log::info!("Natively converted {converted}/{} xml files to ymap.", inputs.len());
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::{Xml2Ymap, Ymap2Xml};

  #[test]
  fn managed_output_cleanup_preserves_rebuilt_and_unmanaged_files() {
    let temp =
      std::env::temp_dir().join(format!("mlo_managed_output_cleanup_{}", std::process::id()));
    let input = temp.join("input");
    let output = temp.join("output");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(input.join("_managed_ymaps.txt"), "clone.ymap\nrebuilt.ymap\nvanilla.ymap\n")
      .unwrap();
    std::fs::write(input.join("rebuilt.ymap.xml"), "xml").unwrap();
    for name in ["clone.ymap", "rebuilt.ymap", "vanilla.ymap", "unmanaged.ymap"] {
      std::fs::write(output.join(name), "binary").unwrap();
    }
    let prune = || {
      crate::core::format::gamefile::resource_convert::prune_managed_ymap_outputs(&input, &output)
    };
    prune().unwrap();
    assert!(!output.join("clone.ymap").exists());
    assert!(!output.join("vanilla.ymap").exists());
    assert!(output.join("rebuilt.ymap").exists());
    assert!(output.join("unmanaged.ymap").exists());
    std::fs::write(input.join("_managed_ymaps.txt"), "unmanaged.ymap\n../escape.ymap\n").unwrap();
    assert!(prune().is_err());
    assert!(
      output.join("unmanaged.ymap").exists(),
      "validate the complete manifest before removing files"
    );
    std::fs::remove_dir_all(temp).unwrap();
  }

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

  #[test]
  fn native_batch_conversion_rebuilds_binary_ymap() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = base.join("asset/extracted.xml/brofx_mansion_06___apa_ch2_occl_05.ymap.xml");
    let schema_dir = base.join("asset/extracted");
    let temp_dir =
      std::env::temp_dir().join(format!("mlo_merger_native_xml2ymap_{}", std::process::id()));
    let input_dir = temp_dir.join("input");
    let output_dir = temp_dir.join("output");
    std::fs::create_dir_all(&input_dir).unwrap();
    std::fs::copy(&fixture, input_dir.join(fixture.file_name().unwrap())).unwrap();

    Xml2Ymap {
      input_dir,
      output_dir: output_dir.clone(),
    }
    .run_native(&schema_dir)
    .unwrap();

    let output = output_dir.join("brofx_mansion_06___apa_ch2_occl_05.ymap");
    let bytes = std::fs::read(output).unwrap();
    let resource =
      crate::core::format::gamefile::resource_file::Rsc7Resource::decode(&bytes).unwrap();
    let meta =
      crate::core::format::gamefile::meta_resource::MetaResource::parse(&resource).unwrap();
    assert_eq!(meta.root_block_index, 1);

    std::fs::remove_dir_all(temp_dir).unwrap();
  }
}
