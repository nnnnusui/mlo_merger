use mlo_merger::cli::command::{parse_args, run};
use std::io::IsTerminal;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

mod gui;
mod log;

fn main() {
  // A launch without arguments, such as a double-click, opens the vanilla generator GUI.
  // Any argument, such as `--pipeline`, runs the command line.
  let result = if std::env::args_os().len() == 1 { gui::run() } else { run_cli() };
  if let Err(error) = result {
    eprintln!("{error}");
    std::process::exit(1);
  }
}

fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
  let args = parse_args();
  let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
  let terminal =
    log::tree_layer(std::io::stderr, std::io::stderr().is_terminal()).with_filter(filter);
  let (file, guard) = log::file_writer(log::log_file_path(&args.log_dir));
  let file = log::tree_layer(file, false).with_filter(EnvFilter::new("info,mlo_merger=debug"));
  tracing_subscriber::registry().with(terminal).with(file).try_init()?;

  let result = run(args);
  // Flush the background log writer; `process::exit` skips destructors.
  drop(guard);
  result
}
