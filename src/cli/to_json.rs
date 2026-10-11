use crate::core::lod_light::ExportLights;
use bpaf::*;
use std::path::PathBuf;

/// Parses a cached resource-local light export.
pub fn parser() -> impl Parser<ExportLights> {
  let input = long("to-json")
    .help("Export resource-local YMAP light editing JSON")
    .argument::<PathBuf>("YMAP");
  let resource =
    long("resource").help("Source resource ID, including bracket groups").argument::<String>("ID");
  let source_dir =
    short('i').long("input").argument::<PathBuf>("DIR").fallback("asset/source".into());
  let source_cache_dir =
    long("source-cache").argument::<PathBuf>("DIR").fallback("asset/source-cache".into());
  let output_dir =
    short('o').long("output").argument::<PathBuf>("DIR").fallback("asset/read_cache/ymap".into());
  let force = short('f').long("force").switch();
  construct!(input, resource, source_dir, source_cache_dir, output_dir, force).map(
    |(input, resource, source_dir, source_cache_dir, output_dir, force)| ExportLights {
      input,
      resource,
      source_dir,
      source_cache_dir,
      output_dir,
      force,
    },
  )
}

/// Writes or reuses the JSON and reports the exact output path.
pub fn run(command: ExportLights) -> Result<(), Box<dyn std::error::Error>> {
  let result = command.run()?;
  println!("{} {}", if result.skipped { "Reused" } else { "Exported" }, result.path.display());
  Ok(())
}
