use crate::cli::MergeYmapXml;

pub fn run_merge_ymap_xml(args: MergeYmapXml) -> Result<(), Box<dyn std::error::Error>> {
  println!(
    "Merging YMAP XML files from {} and {} into {}",
    args.vanilla_dir.display(),
    args.mod_dir.display(),
    args.output_dir.display()
  );
  Ok(())
}
