use std::time::Duration;

pub(crate) struct LogTime;

impl tracing_tree::time::FormatTime for LogTime {
  fn format_time(
    &self,
    writer: &mut impl std::fmt::Write,
  ) -> std::fmt::Result {
    write!(writer, "{}", chrono::Local::now().format("%H:%M:%S"))
  }

  fn style_timestamp(
    &self,
    ansi: bool,
    elapsed: Duration,
    writer: &mut impl std::fmt::Write,
  ) -> std::fmt::Result {
    tracing_tree::time::FormatTime::style_timestamp(
      &tracing_tree::time::Uptime::default(),
      ansi,
      elapsed,
      writer,
    )
  }
}

/// The shared console and log-file layout: an indented span tree with `HH:MM:SS` times.
pub(crate) fn tree_layer<W>(
  make_writer: W,
  ansi: bool,
) -> tracing_tree::HierarchicalLayer<W, LogTime>
where
  W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + 'static,
{
  tracing_tree::HierarchicalLayer::new(2)
    .with_writer(make_writer)
    .with_indent_amount(2)
    .with_timer(LogTime)
    .with_deferred_spans(true)
    .with_targets(false)
    .with_ansi(ansi)
}

/// Creates `asset/log/mlo_merger_<timestamp>.log` for this run.
///
/// Returns `None` after a warning when the file cannot be created, so logging stays optional.
pub(crate) fn file_writer() -> Option<(
  tracing_appender::non_blocking::NonBlocking,
  tracing_appender::non_blocking::WorkerGuard,
)> {
  let name = format!("mlo_merger_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"));
  match tracing_appender::rolling::RollingFileAppender::builder()
    .rotation(tracing_appender::rolling::Rotation::NEVER)
    .filename_prefix(name)
    .filename_suffix("log")
    .build("asset/log")
  {
    Ok(appender) => Some(tracing_appender::non_blocking(appender)),
    Err(error) => {
      eprintln!("Warning: log file is disabled: {error}");
      None
    }
  }
}
