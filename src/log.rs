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
