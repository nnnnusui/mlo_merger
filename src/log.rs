use std::{
  fs::{self, File},
  io::{self, Write},
  path::{Path, PathBuf},
  time::Duration,
};

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

/// The run's log file, created by the first write so a run that logs nothing leaves no file.
enum LazyLogFile {
  Pending(PathBuf),
  Open(File),
  Disabled,
}

impl LazyLogFile {
  fn open(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    File::options().create(true).append(true).open(path)
  }
}

impl Write for LazyLogFile {
  fn write(
    &mut self,
    buffer: &[u8],
  ) -> io::Result<usize> {
    if let Self::Pending(path) = self {
      *self = match Self::open(path) {
        Ok(file) => Self::Open(file),
        Err(error) => {
          eprintln!("Warning: log file is disabled: {error}");
          Self::Disabled
        }
      };
    }
    match self {
      Self::Open(file) => file.write(buffer),
      _ => Ok(buffer.len()),
    }
  }

  fn flush(&mut self) -> io::Result<()> {
    match self {
      Self::Open(file) => file.flush(),
      _ => Ok(()),
    }
  }
}

/// Writes this run's logs to `asset/log/mlo_merger_<timestamp>.log`.
///
/// The file only appears once something is logged. When it cannot be created, a warning is
/// printed and logs are discarded, so logging stays optional.
pub(crate) fn file_writer()
-> (tracing_appender::non_blocking::NonBlocking, tracing_appender::non_blocking::WorkerGuard) {
  let name = format!("mlo_merger_{}.log", chrono::Local::now().format("%Y%m%d_%H%M%S"));
  tracing_appender::non_blocking(LazyLogFile::Pending(Path::new("asset/log").join(name)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn log_file_is_created_by_the_first_write() {
    let root = std::env::temp_dir().join(format!("mlo_merger_log_{}", std::process::id()));
    let path = root.join("log/run.log");

    let mut unused = LazyLogFile::Pending(path.clone());
    unused.flush().unwrap();
    drop(unused);
    assert!(!root.exists());

    let mut used = LazyLogFile::Pending(path.clone());
    used.write_all(b"line\n").unwrap();
    used.flush().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"line\n");

    fs::remove_dir_all(root).unwrap();
  }
}
