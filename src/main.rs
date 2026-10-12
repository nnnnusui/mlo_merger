use mlo_merger::cli::command::{parse_args, run};
use std::io::IsTerminal;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod log;

use log::LogTime;

fn main() {
  let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
  let subscriber = tracing_subscriber::registry().with(filter).with(
    tracing_tree::HierarchicalLayer::new(2)
      .with_indent_amount(2)
      .with_timer(LogTime)
      .with_deferred_spans(true)
      .with_targets(false)
      .with_ansi(std::io::stderr().is_terminal()),
  );
  if let Err(error) = subscriber.try_init() {
    eprintln!("{error}");
    std::process::exit(1);
  }

  if let Err(error) = run(parse_args()) {
    eprintln!("{error}");
    std::process::exit(1);
  }
}
