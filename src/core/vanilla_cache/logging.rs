//! Scoped generation logs and application logger integration.

use std::cell::RefCell;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::Result;

struct LogSink {
  file: BufWriter<fs::File>,
  error: Option<std::io::Error>,
}

thread_local! {
  static VERSION_LOG: RefCell<Option<LogSink>> = const { RefCell::new(None) };
}

struct VersionLogger;

impl log::Log for VersionLogger {
  fn enabled(
    &self,
    metadata: &log::Metadata<'_>,
  ) -> bool {
    metadata.level() <= log::Level::Debug
  }

  fn log(
    &self,
    record: &log::Record<'_>,
  ) {
    if !self.enabled(record.metadata()) {
      return;
    }
    VERSION_LOG.with_borrow_mut(|sink| {
      if let Some(sink) = sink
        && let Err(error) = writeln!(
          sink.file,
          "{} [{}] {}: {}",
          chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
          record.level(),
          record.target(),
          record.args()
        )
      {
        sink.error = Some(error);
      }
    });
  }

  fn flush(&self) {
    VERSION_LOG.with_borrow_mut(|sink| {
      if let Some(sink) = sink
        && let Err(error) = sink.file.flush()
      {
        sink.error = Some(error);
      }
    });
  }
}

impl simplelog::SharedLogger for VersionLogger {
  fn level(&self) -> log::LevelFilter {
    log::LevelFilter::Debug
  }
  fn config(&self) -> Option<&simplelog::Config> {
    None
  }
  fn as_log(self: Box<Self>) -> Box<dyn log::Log> {
    self
  }
}

/// Adds per-thread cache log routing to an application's existing CombinedLogger.
///
/// ```no_run
/// simplelog::CombinedLogger::init(vec![mlo_merger::core::vanilla_cache::version_logger()])?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn version_logger() -> Box<dyn simplelog::SharedLogger> {
  Box::new(VersionLogger)
}

/// Routes the current thread's cache records to a scoped generation log.
pub(crate) struct VersionLog;

impl VersionLog {
  pub(crate) fn start(directory: &Path) -> Result<Self> {
    let file = fs::File::create(directory.join("create_cache.log"))?;
    VERSION_LOG.with_borrow_mut(|sink| {
      *sink = Some(LogSink {
        file: BufWriter::new(file),
        error: None,
      });
    });
    Ok(Self)
  }

  pub(crate) fn finish(&self) -> Result<()> {
    VERSION_LOG.with_borrow_mut(|sink| -> Result<()> {
      if let Some(sink) = sink {
        sink.file.flush()?;
        if let Some(error) = sink.error.take() {
          return Err(error.into());
        }
      }
      Ok(())
    })
  }
}

impl Drop for VersionLog {
  fn drop(&mut self) {
    VERSION_LOG.with_borrow_mut(|sink| {
      *sink = None;
    });
  }
}

#[cfg(test)]
pub(crate) fn init_test_version_logger() {
  static LOG_INIT: std::sync::Once = std::sync::Once::new();
  LOG_INIT.call_once(|| {
    log::set_boxed_logger(version_logger().as_log()).unwrap();
    log::set_max_level(log::LevelFilter::Debug);
  });
}
