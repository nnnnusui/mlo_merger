use mlo_merger::cli::command::{parse_args, run};
use std::io::IsTerminal;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

mod log;

fn main() {
  let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
  let terminal =
    log::tree_layer(std::io::stderr, std::io::stderr().is_terminal()).with_filter(filter);
  let (file, guard) = log::file_writer().unzip();
  let file = file.map(|writer| {
    log::tree_layer(writer, false).with_filter(EnvFilter::new("info,mlo_merger=debug"))
  });
  if let Err(error) = tracing_subscriber::registry().with(terminal).with(file).try_init() {
    eprintln!("{error}");
    std::process::exit(1);
  }

  let result = run(parse_args());
  // Flush the background log writer; `process::exit` skips destructors.
  drop(guard);
  if let Err(error) = result {
    eprintln!("{error}");
    std::process::exit(1);
  }
}
