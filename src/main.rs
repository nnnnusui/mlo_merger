use mlo_merger::cli::command::{parse_args, run};
use simplelog::{ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode};

fn main() {
  if let Err(error) = CombinedLogger::init(vec![
    TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Stderr, ColorChoice::Auto),
    mlo_merger::core::vanilla::version_logger(),
  ]) {
    eprintln!("{error}");
    std::process::exit(1);
  }

  if let Err(error) = run(parse_args()) {
    eprintln!("{error}");
    std::process::exit(1);
  }
}
