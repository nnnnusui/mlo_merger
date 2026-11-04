//! Log lines collected from `tracing` for display in the window.

use std::{
  io,
  sync::{Arc, Mutex, OnceLock},
};

use eframe::egui;

/// Lines kept for display; older lines remain in the log file.
const MAX_LINES: usize = 2000;

/// A `tracing` writer that keeps recent log lines and repaints the window on new output.
#[derive(Clone, Default)]
pub struct LogView {
  inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
  lines: Mutex<Lines>,
  context: OnceLock<egui::Context>,
}

#[derive(Default)]
struct Lines {
  complete: Vec<String>,
  partial: String,
}

impl LogView {
  /// Repaints `context` whenever a line arrives.
  pub fn set_context(
    &self,
    context: egui::Context,
  ) {
    let _ = self.inner.context.set(context);
  }

  /// Calls `show` with the complete lines collected so far.
  pub fn with_lines(
    &self,
    show: impl FnOnce(&[String]),
  ) {
    show(&self.lock().complete);
  }

  /// Drops collected lines.
  pub fn clear(&self) {
    self.lock().complete.clear();
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, Lines> {
    self.inner.lines.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl io::Write for LogView {
  fn write(
    &mut self,
    buffer: &[u8],
  ) -> io::Result<usize> {
    {
      let mut lines = self.lock();
      lines.partial.push_str(&String::from_utf8_lossy(buffer));
      while let Some(end) = lines.partial.find('\n') {
        let line = lines.partial[..end].trim_end_matches('\r').to_string();
        lines.partial.drain(..=end);
        lines.complete.push(line);
      }
      let excess = lines.complete.len().saturating_sub(MAX_LINES);
      lines.complete.drain(..excess);
    }
    if let Some(context) = self.inner.context.get() {
      context.request_repaint();
    }
    Ok(buffer.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for LogView {
  type Writer = LogView;

  fn make_writer(&'writer self) -> Self::Writer {
    self.clone()
  }
}
