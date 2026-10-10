use bpaf::*;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct GenerateRTreeJson {
  pub output_dir: PathBuf,
}

pub fn parser() -> impl Parser<GenerateRTreeJson> {
  let command = long("generate-rtree-json").req_flag(());
  let output_dir = short('o')
    .long("output")
    .help("Directory for stream_file_rtree-ymap.json and stream_file_rtree-ybn.json")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/read_cache"));
  construct!(command, output_dir).map(|(_, output_dir)| GenerateRTreeJson {
    output_dir,
  })
}

pub fn run(command: GenerateRTreeJson) -> Result<(), Box<dyn std::error::Error>> {
  for output in crate::core::find::rtree::generate(&command.output_dir)? {
    println!("Wrote stream-file R-tree JSON to {}", output.display());
  }
  Ok(())
}
