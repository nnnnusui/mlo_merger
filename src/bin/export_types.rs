use bpaf::*;
use std::path::PathBuf;

fn parser() -> impl Parser<PathBuf> {
  short('o')
    .long("output")
    .help("Output TypeScript file (default: asset/gen_mlo_merger.ts)")
    .argument::<PathBuf>("PATH")
    .fallback(PathBuf::from("asset/gen_mlo_merger.ts"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let output = parser().to_options().run();
  mlo_merger::typescript::export_types(&output)?;
  println!("Generated {}", output.display());
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn output_path_uses_short_long_and_default_options() {
    assert_eq!(
      parser().to_options().run_inner(&["-o", "types/custom.d.ts"]).unwrap(),
      PathBuf::from("types/custom.d.ts")
    );
    assert_eq!(
      parser().to_options().run_inner(&["--output", "types/other.ts"]).unwrap(),
      PathBuf::from("types/other.ts")
    );
    assert_eq!(
      parser().to_options().run_inner(&[] as &[&str]).unwrap(),
      PathBuf::from("asset/gen_mlo_merger.ts")
    );
    assert!(parser().to_options().run_inner(&["types/positional.ts"]).is_err());
  }
}
