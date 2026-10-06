use mlo_merger::cli::command::{parse_args, run};

fn main() {
  if let Err(error) = run(parse_args()) {
    eprintln!("{error}");
    std::process::exit(1);
  }
}
