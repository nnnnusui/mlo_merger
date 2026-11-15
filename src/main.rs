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
  let filter =
    terminal_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))?;
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

fn terminal_filter(filter: EnvFilter) -> Result<EnvFilter, Box<dyn std::error::Error>> {
  Ok(
    filter
      .add_directive("mlo_merger::core::merge=warn".parse()?)
      .add_directive("mlo_merger::core::format::ymap::compare=warn".parse()?),
  )
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use tracing_subscriber::{Layer, layer::Context, layer::SubscriberExt};

  use super::terminal_filter;

  #[test]
  fn terminal_keeps_merge_warnings_but_suppresses_details() {
    let targets = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
      Capture(Arc::clone(&targets))
        .with_filter(terminal_filter(tracing_subscriber::EnvFilter::new("info")).unwrap()),
    );
    tracing::subscriber::with_default(subscriber, || {
      tracing::info!(target: "mlo_merger::core::merge::run::engine::plan", "merge detail");
      tracing::warn!(target: "mlo_merger::core::merge::run::engine::plan", "merge warning");
      tracing::info!(target: "mlo_merger::core::format::ymap::compare::merge", "comparison detail");
      tracing::info!(target: "mlo_merger::cli::merge", "progress");
    });

    assert_eq!(
      *targets.lock().unwrap(),
      ["mlo_merger::core::merge::run::engine::plan", "mlo_merger::cli::merge"]
    );
  }

  struct Capture(Arc<Mutex<Vec<String>>>);

  impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(
      &self,
      event: &tracing::Event<'_>,
      _: Context<'_, S>,
    ) {
      self.0.lock().unwrap().push(event.metadata().target().to_string());
    }
  }
}
